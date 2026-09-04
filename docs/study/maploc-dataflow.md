# maploc — data flow inside robotd

Source: upstream PR 127 (`maploc/` subcrate + `robotd/src/maploc.rs`,
API v17) read on 2026-09-04, plus the `odometry`, `tof` and
`kinematics::tof` crates on main. Diagram: `maploc-dataflow.mermaid`.
Everything below describes the PR as it stood that day; it is unmerged
and may change.

The one-paragraph version: **only two sensors feed the map — the head
ToF and the legs.** Every 20 ms the control loop hands the mapper one
small struct (contact odometry, gravity, head joints, three verdicts);
fifteen times a second a separate thread hands it one 8×8 depth frame
from tofd. The mapper only inks when the robot stands still, votes a
whole stop's frames into one wide scan, checks that scan against the map
before believing it, and paints it into a 4×4 m submap. Freezing a
submap triggers loop closure and graph optimization. What comes out is a
trinary grid plus a pose, once a second, to whoever subscribed. The
camera plays no part.

## 1. What enters robotd

| Source | Path | Rate | Content used by maploc |
|---|---|---|---|
| Dynamixel bus (`/dev/ttyS2`) | one `sync_read` per tick in the control loop | 50 Hz | 15 servo positions (head joints are positions 5–8: neck_pitch, head_pitch, head_yaw, head_roll); trunk IMU quaternion and projected gravity |
| contact odometry (`odometry` crate, a struct inside the loop) | foot FK on the MJCF model + IMU yaw | 50 Hz | `(x, y, yaw)` in the odometry frame ("wherever the robot looked at boot", no magnetometer) and trunk height `z` |
| loop verdicts | policy label / controller / safety | 50 Hz | `moving` (policy says "walk" or a scripted move is busy), `sitting`, `fallen` |
| head ToF VL53L5CX/L8CX | I²C bus shared with the audio codec → **tofd**, its own daemon | 15 Hz | `TofFrame { seq, at_us, rows, cols, distance_mm[64], status[64] }` over `tof.stream` on `/run/tofd/tof.sock` |
| `/etc/robot/robotd.toml` `[maploc]` | read at start | once | `enabled`, `mode` (`stop_and_scan` \| `continuous`), `map_path`, `wipe_on_boot`, `search_sweep`, `record_dir` |
| `/var/lib/robot/maploc.session` | loaded at worker start | once | previous submaps, pose graph, tracked pose (bincode) |
| IPC clients | `/run/robotd.sock` | on demand | `robot.map` (subscribe), `robot.map_wipe` |

Not inputs: the camera and mediad, the second IMU, WebRTC. The ToF has a
45°×45° field of view and its useful range is capped at 2 m by the
accumulator.

## 2. How it gets to the worker

Two threads besides the control loop, both reniced to +10 so the loop
wins every contest for a core:

- **`maploc-tof`**: connects to tofd like any other client (`hello` +
  `tof.stream`), reconnects with backoff up to 10 s, and pushes every
  frame into the worker's channel. tofd down or absent means mapping
  idles, nothing else.
- **`maploc` worker**: owns the pipeline. Fed by one `mpsc` channel of
  128 events (`Odom`, `Frame`, `Wipe`, `Shutdown`). The control loop
  pays exactly one `try_send` per tick; a full channel drops the sample
  (odometry deltas re-fold on the next accepted one, a dropped depth
  frame is one of fifteen a second). Mapping lag can never become loop
  backpressure.

When `record_dir` is set, the worker also writes everything it consumes
to a `.mdlg` file (odom 45 bytes per tick + raw ToF frames, ~6 KB/s):
the offline bench replays it through the same `Mapper` byte for byte.

## 3. The per-tick path (odometry, 50 Hz)

`Mapper::observe(t, sample)`:

1. **Compose the delta.** Raw odometry lives in its own frame; only the
   body-frame delta between consecutive readings is applied to the
   **tracked pose**, which lives in the MAP frame. The two frames
   coincide until a loop closure or a relocalization says otherwise.
2. **Stillness.** A 0.5 s window of odometry: still when translation
   < 1 cm, |yaw| < 0.05 rad, and not moving/sitting/fallen. Stillness
   comes from odometry itself, so a robot pushed by hand is not still.
3. **Window flush.** When a stand ends, or 3 s after the window opened,
   the accumulator closes and the composite goes to `absorb_window`
   (§5).
4. **Stand begins → snapshot.** The current global render is frozen as
   `stand_grid`: the watchdog judges this stand's windows against the
   map *as it was before the stand*, so a kidnapped robot cannot vouch
   for itself with ink it just painted.
5. **Sit or fall → suspicion.** The pose becomes suspect (`lost = true`)
   with "I was not moved" as a soft seed. Nothing inks until a window
   confirms the pose.
6. **If tracking: `Slam::tick`.** The submap manager freezes the current
   submap when it is older than 8 s (and the robot moved ≥ 15 cm) or
   the pose travelled 0.8 m from its anchor; a new submap opens anchored
   at the tracked pose and its graph node is chained to the previous one
   immediately. On a freeze the loop closer runs (§6).

## 4. The per-frame path (ToF, 15 Hz)

1. **Decode.** 64 zones; a zone counts only with status 5 or 9 and a
   positive distance. Millimetres become metres.
2. **Posture.** Projected gravity and the odometry's trunk height from
   the latest tick (fallback to the model's rest height when the robot
   is not standing).
3. **Reproject** (`kinematics::tof::Reprojector::flatten`, already on
   main): the 8×8 beam table (45° FOV, half-zone inset) is rotated
   through the head forward kinematics *per frame*, so a panning head
   reprojects correctly. Returns under 0.10 m are discarded (cover-glass
   crosstalk); a beam whose downward reach covers 85 % of the sensor's
   height above the floor is the floor, not an obstacle. Survivors are
   expressed in the gravity-levelled body frame as azimuth + horizontal
   range **measured from the sensor**, plus the sensor's body-frame
   position. Per-beam origins are what let frames from different head
   yaws merge exactly.
4. **Scan.** `Scan::from_polar` → `Mapper::frame`:
   - `continuous` mode and tracking: ink directly at the tracked pose.
   - `stop_and_scan` (default), or continuous while lost: only if the
     robot is still, push `(tracked pose, scan)` into the accumulator.
     Frames while walking are dropped.

## 5. The window path (one per stop, or every 3 s of a stand)

`WindowAccumulator::finish` then `Mapper::absorb_window`:

1. **Vote.** Every frame's endpoints are binned into 5 cm cells; a beam
   survives only if its cell was hit by ≥ 3 distinct frames. Beams
   longer than 2 m are dropped. Survivors merge into one composite scan
   at the window's middle pose. Windows with fewer than 6 frames pass
   through unfiltered. A walking passer-by is somewhere else in each
   frame and loses the vote; a wall wins it.
2. **Thin windows** (< 60 beams) are discarded. A seated robot's
   floor-clutter windows measured 2–27 beams; a real stop measures in
   the hundreds.
3. **If lost:** the soft seed is checked first and must agree with two
   consecutive windows before tracking resumes on it; then the last
   search candidate, which the current window must confirm (≤ 0.10 m
   mean residual over ≥ 30 % of its beams); otherwise a brute-force
   relocalize (`relocalize_against_grid`, the composite decimated to
   256 beams, coarse grid + fine refinement) proposes a new candidate
   for the *next* window to judge. After 10 windows the map could not
   judge at all, soft suspicion gives up and tracking resumes at the
   odometry-carried pose, unverified. A window the map *refutes*
   removes that escape.
4. **If tracking: watchdog.** The composite is scored against
   `stand_grid`. If the map can judge ≥ 100 beams and ≥ 5 % of them,
   and the mean residual exceeds 0.25 m, the window is quarantined (not
   inked); two consecutive contradictions declare tracking lost. An
   explorer in a new room lands in territory the map cannot judge and
   keeps mapping; a kidnapped robot lands where the map knows and
   disagrees everywhere.
5. **Ink.** Two log-odds passes into the current submap (+85 per pass
   per wall cell, a wall starts at 150, so one vetted window makes a
   wall); free space along each ray is decremented. `windows` counts
   up; the pipeline is marked dirty.

## 6. Loop closure and optimization (on submap freeze)

- Each submap is a 4×4 m local log-odds grid at 5 cm, anchored in the
  map frame, with its raw scans retained.
- On freeze, older submaps within the loop closer's radius (not the
  immediate predecessor) are matched: coarse correlative search
  (±0.5 m, ±20°) then Gauss-Newton refinement on the cached distance
  field. A closure needs per-scan residual, beam and coverage gates,
  **two agreeing strong witnesses** (≥ 150 beams; the 12-beam scraps
  used to veto the composites), a correction plausible for the odometry
  drift over the gap, and a correction floor so noise edges are dropped.
- Accepted closures add edges to the SE(2) pose graph, weighted by
  match residual; a dense Gauss-Newton optimizer relaxes the graph;
  every submap anchor **and the tracked pose** move with their nodes
  (the prototype forgot the tracked pose; the port pins it with a test).
  Frames already sitting in the accumulator are dropped, because their
  poses are pre-correction.

## 7. What comes out

| Output | Path | Cadence | Content |
|---|---|---|---|
| `map.frame` notifications | `robot.map` subscribers on `/run/robotd.sock`, broadcast buffer 4 | 1 Hz, only while someone is subscribed | `seq`, pose `x, y, yaw` in the map frame, `tracking`, grid origin `x_min, y_min`, `cell_m` (0.05), `rows × cols`, `cells` base64 (0 unknown, 1 free, 2 wall: log-odds > 150 wall, < −50 free), `n_submaps`, `n_loops`, `windows`, `still`, `seated` |
| head yaw override | control loop reads `Host::searching()` | every tick while standing, if `search_sweep` and (searching **or** stop-and-scan mode) | triangle wave ±0.9 rad over 6 s on head yaw only — a 45° wedge becomes a ~150° composite. **This is the feedback edge that makes the flow cyclic**: the mapper's state moves the head, the head moves the sensor, the sensor feeds the mapper |
| session file | `map_path` | autosave every 60 s when dirty, on shutdown, on panic teardown | submaps + pose graph + tracked pose, atomic write |
| `.mdlg` recording | `record_dir/<unix time>.mdlg` | continuous while enabled | everything the mapper consumed |
| journal | tracing | status every 5 s + one line per note | `odom/frames/kept/windows/still/tracking/moving/sitting/fallen/window_frames/submaps`; notes: window integrated / discarded / quarantined, suspect after sit/fall, relocalize candidate / relocalized / rejected, tracking lost, loop closed, resumed unverified |
| `robotctl monitor` | subscribes `robot.map` | 1 Hz | path panel becomes the map (walls in braille, free space stippled, robot marker, magenta `?` while searching); `m` for fullscreen |
| `robot.map_wipe` answer | RPC | on demand | accepted / refused ("mapper overloaded" or "mapping not enabled") |

Dormant, no RPC yet: the A* planner (8-connected, obstacle inflation,
line-of-sight simplification) and the turn-then-go follower that emits
body-frame velocities `(vx, wz)`. `robot.look` (gaze IK) exists on main
independently.

## 8. Timing at a glance

| Clock | Period |
|---|---|
| control loop tick | 20 ms |
| ToF frame | 66 ms |
| stillness window | 0.5 s |
| still-window flush | on stand end or 3 s |
| head sweep | 6 s per triangle |
| submap freeze | 8 s (if moved ≥ 15 cm) or 0.8 m travel |
| map publish | 1 s |
| status log | 5 s |
| session autosave | 60 s |
| relocalize search | "a few hundred ms" one-shot on a 4×4 m grid |

## 9. What this means for quacksat

- **We consume one stream.** Subscribe to `robot.map`, keep the newest
  `map.frame`, decode the base64 grid only when needed. A robotd older
  than v17 answers METHOD_NOT_FOUND: the feature stays off, no error.
- **The pose is only meaningful with `tracking = true`.** While
  `seated` the mapper refuses to map or relocalize; while searching the
  head sweeps on its own and `robot.head` intents from us would fight
  it. `where_am_i` must say "not sure" in those states.
- **The map frame can move.** A loop closure shifts every anchor and
  the tracked pose; a wipe resets everything; a resumed session starts
  suspect. Places must be stored in the map frame of a given session
  and re-validated after a wipe. There is no session id on the wire
  today: `seq` restarts at 1 and `n_submaps` drops to 0 after a wipe,
  which is the signal we have.
- **A mapping tour is deliberate.** Nothing inks while walking. The
  robot must stop, stand ≥ 0.5 s, and let the sweep run; `windows` on
  the wire tells whether stops are reaching the map. The guided tour
  narrates exactly that.
- **CPU.** Worker and feed are niced; the loop keeps its 50 Hz. The
  relocalize search costs hundreds of ms and the render grows with the
  map. The wake word shares the cores; December measures it.
- **Nothing to navigate with yet.** `go_to` waits for a goal RPC that
  wires the planner and follower; the raw grid is on the wire if we
  ever needed to plan ourselves, but ADR 0005 says we do not.

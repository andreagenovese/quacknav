# TODO — House map and localization

Status: idea, not started. Prerequisite: quacksat talks (core + backend).
Principle: **everything off-board**. The duck streams camera, IMU, odometry and
ToF; the server reconstructs, localizes and reasons. On board, intents only.

## 0. Preliminary study
- [ ] Read `maploc` in `apirrone/microduck_runtime` (mapping, MCL,
      planning): what it did, with which sensors, why it is "unowned" in the
      new stack. Decide whether to port it or supersede it.
- [ ] Check what `mediad` exposes today: WebRTC stream, `get_frame`
      JPEG, camera resolution/fps, timestamps shared with IMU/odometry.
- [ ] Check format and frequency of `tof.stream` (8×8, 15 Hz) and of
      `odom` in the `robot.state` stream.
- [ ] Evaluate foundational monocular SLAM models: MASt3R-SLAM,
      VGGT-SLAM, SLAM3R, FoundationSLAM, OpenMonoGS-SLAM (geometry +
      semantics). Criteria: real-time on consumer GPU, license, code
      maturity, injectable metric scale.
- [ ] Evaluate open-vocabulary scene graphs (ConceptGraphs, HOV-SG,
      Hydra) as a representation readable by the agent.
- [ ] ADR: mapping/localization architecture (sensors, model,
      representation, what runs where).

## 1. Infrastructure
- [ ] Always-on server with a GPU (≥12 GB VRAM) — the MacBook is not enough.
- [ ] Recording pipeline: save synchronized video + IMU + odom + ToF
      of a walk around by the duck (dataset for working offline).
- [ ] Measure Wi-Fi bandwidth and stream latency while moving.

## 2. Phase A — Cheap relocalization (unblocks the agent right away)
- [ ] AprilTags on door frames; detection on the server from the frames.
- [ ] Topological map: room/passage graph in a file (YAML/JSON) with
      natural-language labels, enriched by the agent ("kitchen:
      oven, fridge").
- [ ] Tools for the agent: `where_am_i()`, `go_to(room)` → sequence of
      `robot.move`/`robot.head` intents with visual confirmation of the tag.
- [ ] "Find the tag" behavior: rotation + ToF/camera scan.

## 3. Phase B — Offline metric geometry
- [ ] Run MASt3R-SLAM (or the chosen alternative) on the recorded videos.
- [ ] Inject the metric scale from ToF/odometry; measure the drift.
- [ ] Understand what a camera at 25 cm off the ground really sees (chair
      legs, not tables): evaluate stationary keyframes (stand policy) to
      reduce motion blur.
- [ ] Map persistence and reload between sessions (~1 h battery).

## 4. Phase C — Online localization
- [ ] Live stream → SLAM on the server → pose published (topic/RPC) towards
      the agent and towards quacksat.
- [ ] Fusion with on-board odometry for the estimate between frames.
- [ ] Cold relocalization: persisted map + Phase A AprilTags.
- [ ] Local obstacle avoidance from the ToF (reactive, outside the LLM loop);
      evaluate neck oscillation for a fan-shaped scan.

## 5. Phase D — Semantics and agent
- [ ] Open-vocabulary 3D scene graph: rooms, objects, relationships,
      coordinates.
- [ ] Tools for [Arkimede](https://github.com/andreagenovese/yesSir): `where_is(object)`, `go_to(place)`, `look_at(x)`,
      `describe_surroundings()`; waypoint planning on the server.
- [ ] Closed loop: goal from the agent → waypoints → intents → updated
      pose → visual verification → answer to the user.
- [ ] Trial missions: "go to the kitchen and tell me if the oven is on",
      "where is the ball", "return to base".

## Risks and open questions
- Motion blur and low camera angles of the bipedal gait.
- Contact odometry drift (no magnetometer): world frame =
  where the robot was looking at startup.
- GPU cost and power consumption of an always-on server.
- Privacy: home video runs on the local server, never leaves it (the
  "local processing" constraint).
- Charging base: without one, every session starts from an unknown pose.
- Multiple ducks (chorale!): shared map and multi-robot localization.

# A textured 3D house (experimental)

Off board: a 3D house with the head camera's own colours, from what the
duck already sends — mediad's frames, robotd's `frames.camera` /
`frames.tof` and IMU, tofd's 8x8 ranges and quack-navd's pose on the map.
Italian: [README.it.md](README.it.md).

```sh
python3 scripts/map3d/record.py $STATE rec/ 300 [--sim-port 7872]   # beside an exploring duck
python3 scripts/map3d/build.py rec/ out/ --voxel 0.02               # iso_*.png, top.png, cloud.ply, house3d.bin
python3 scripts/map3d/accuracy.py out/ rec/ <house>.truth.json      # the twin only
```

- `record.py` keeps a frame each half second while the duck stands.
- `build.py`: Depth Anything V2 **Small** (Apache-2.0; Base/Large are
  non-commercial, not used) gives each frame a relative depth; the depth
  sensor's returns seen by the camera fit it to metres, frame by frame;
  the frames are fused in a TSDF on the map's frame with the camera's
  colours, and drawn isometric. Needs torch, transformers, Pillow.
- `house3d.bin` is the house for quack-control's **3D** tab (its `house3d`
  service, `/var/lib/quack-control/house3d.bin` on the duck): ~0.5 MB for
  the first run, with the map's name, so the page draws the live duck on
  it only on that map. Copy it as quack-control's README says.
- First run (2026-10-14, casa_arredata, velstand, 5 minutes exploring,
  42 % of the house mapped): 248 of 296 frames fused; against the twin's
  truth, the floor within 1 cm (median), walls and furniture 2.3 cm
  median, 96 % within 10 cm.
- In the simulator only, the head camera looks backwards: microduck_rl's
  MJCF has `head_camera` 180 degrees from the `tof` site (the duck itself
  is fine: robotd's `frames.camera` looks ahead). Pollen know it and turn
  it in their own body_server (`sim/camera.py`, 2026-09-02), leaving the
  MJCF alone; the old body_server the twin runs does not, so
  `viewer/body_with_map.py` makes the same half turn about the camera's
  x, only if the camera still looks away from the sensor
  (`TWIN_CAMERA_BACKWARD=1` keeps the model's).
- On the duck: the camera's axes against `frames.camera` and the
  intrinsics (`media.video`) must be checked with the same overlay of the
  depth sensor's returns on the picture.

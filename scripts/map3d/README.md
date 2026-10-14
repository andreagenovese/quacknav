# A textured 3D house (experimental)

Off board: a 3D house with the head camera's own colours, from what the
duck already sends — mediad's frames, robotd's `frames.camera` /
`frames.tof` and IMU, tofd's 8x8 ranges and quack-navd's pose on the map.
Italian: [README.it.md](README.it.md).

```sh
python3 scripts/map3d/record.py $STATE rec/ 300 [--sim-port 7872]   # beside an exploring duck
python3 scripts/map3d/build.py rec/ out/ --voxel 0.02               # iso_*.png, top.png, cloud.ply
python3 scripts/map3d/accuracy.py out/ rec/ <house>.truth.json      # the twin only
```

- `record.py` keeps a frame each half second while the duck stands.
- `build.py`: Depth Anything V2 **Small** (Apache-2.0; Base/Large are
  non-commercial, not used) gives each frame a relative depth; the depth
  sensor's returns seen by the camera fit it to metres, frame by frame;
  the frames are fused in a TSDF on the map's frame with the camera's
  colours, and drawn isometric. Needs torch, transformers, Pillow.
- First run (2026-10-14, casa_arredata, velstand, 5 minutes exploring,
  42 % of the house mapped): 248 of 296 frames fused; against the twin's
  truth, the floor within 1 cm (median), walls and furniture 2.3 cm
  median, 96 % within 10 cm.
- In the simulator only, the head camera looked backwards: the MJCF
  camera in microduck_rl's model, not the duck's (robotd's
  `frames.camera` looks ahead, beside the depth sensor). The fix is half
  a turn about the camera's x, exactly the step from the OpenCV camera
  frame (z ahead, y down) to MuJoCo's (looking along -z, y up), so the
  model's `quat` was most likely written for the former.
  `viewer/body_with_map.py` applies it (`TWIN_CAMERA_BACKWARD=1` keeps
  the model's).
- On the duck: the camera's axes against `frames.camera` and the
  intrinsics (`media.video`) must be checked with the same overlay of the
  depth sensor's returns on the picture.

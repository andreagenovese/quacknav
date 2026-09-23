#!/bin/zsh
# The MuJoCo twin on Pollen's released robotd, with quack-navd hosting the
# mapper — no robotd fork anywhere.
#
#   scripts/twin/twin.sh up      simulator + tofd + robotd + quack-navd
#   scripts/twin/twin.sh enable  stand the duck up (it boots seated)
#   scripts/twin/twin.sh down    stop what `up` started, and only that
#
# Needs (see README.md):
#   MICRODUCK     pollen-robotics/microduck at daemon-v0.14.4, with
#                 `cargo build -p robotd -p tof` done (target/debug)
#   MICRODUCK_RL  pollen-robotics/microduck_rl, with its .venv
#   POLICY_DIR    alpha_walking.onnx, alpha_stand.onnx, alpha_sitstand.onnx,
#                 alpha_ground_pick.onnx, ball_kick_left/right.onnx, roulade.onnx
# Optional:
#   VIEWER        on (default): the viewer draws the map, the route, the ToF
#                 rays and the guard's lane (scripts/twin/viewer); off: the
#                 plain body server, the duck alone
#   VIEWER_DIR    another body_with_map.py + maploc_overlay.py to use instead
#   STATE         runtime directory (default /tmp/quack-twin)
#   PORT          the simulator's port (default 7872)
#   MAPLOC_MODE   stop_and_scan (default) or localize
#   HOMECOMING    on or off (default off)
#   WIPE          on (default: a fresh map each boot) or off
#   SCENE         the MuJoCo scene (default: the apartment). Other scenes may
#                 include another robot model (scene_walk.xml does, and the duck
#                 tips over on it); keep robot_allcollisions.xml
#   ASK_PHRASE    what the explorer asks at a nameless area (default
#                 "Qui dove siamo?"; the daemon's own default is English)
set -eu
HERE=${0:A:h}
REPO=${HERE:h:h}
STATE=${STATE:-/tmp/quack-twin}
PORT=${PORT:-7872}
SOCK=$STATE/robotd.sock
TOFSOCK=$STATE/tof.sock
detach() { /usr/bin/python3 $HERE/detach.py "$@"; }
need() { [ -n "${(P)1:-}" ] || { echo "set $1 (see scripts/twin/README.md)" >&2; exit 2; }; }

case ${1:-up} in
up)
  need MICRODUCK; need MICRODUCK_RL; need POLICY_DIR
  # A unix socket's path is at most 104 bytes on macOS (108 on Linux); past
  # that robotd, tofd and quack-navd all fail to bind, one by one.
  [ ${#STATE} -le 80 ] || { echo "STATE is too long for a socket path (${#STATE} > 80): use a short one" >&2; exit 2; }
  mkdir -p $STATE/maps $STATE/rec
  if /usr/bin/python3 -c "import socket,sys;s=socket.socket();s.settimeout(.3);sys.exit(s.connect_ex(('127.0.0.1',$PORT)))" 2>/dev/null; then
    echo "port $PORT is already serving — '$0 down' first" >&2; exit 1
  fi
  ort=( $MICRODUCK_RL/.venv/lib/python*/site-packages/onnxruntime/capi/libonnxruntime*.(dylib|so*)(N) )
  ORT=${ort[1]:-}
  [ -n "$ORT" ] || { echo "no onnxruntime library in $MICRODUCK_RL/.venv" >&2; exit 2; }
  cat > $STATE/robotd.toml <<TOML
[policy]
enabled = true
walk = "$POLICY_DIR/alpha_walking.onnx"
stand = "$POLICY_DIR/alpha_stand.onnx"
sitstand = "$POLICY_DIR/alpha_sitstand.onnx"
ground_pick = "$POLICY_DIR/alpha_ground_pick.onnx"
kick_left = "$POLICY_DIR/ball_kick_left.onnx"
kick_right = "$POLICY_DIR/ball_kick_right.onnx"
roulade = "$POLICY_DIR/roulade.onnx"

[chorale]
accept = false

[audio]
enabled = false
TOML
  cat > $STATE/quack-nav.toml <<TOML
socket = "$STATE/nav.sock"
robotd_socket = "$SOCK"

[map]
enabled = true
tof_socket = "$TOFSOCK"
places_path = "$STATE/places.json"
ask_phrase = "${ASK_PHRASE:-Qui dove siamo?}"

[gait]
yaw_trim = 0.08
yaw_gain_left = 1.34
yaw_gain_right = 1.58

[homecoming]
enabled = $([ "${HOMECOMING:-off}" = on ] && echo true || echo false)
start_delay_s = 10.0
boot_search_s = 240.0
recognize_every_s = 120.0
explore_max_s = 720.0

[maploc]
enabled = true
mode = "${MAPLOC_MODE:-stop_and_scan}"
map_path = "$STATE/maploc.session"
wipe_on_boot = $([ "${WIPE:-on}" = on ] && echo true || echo false)
search_sweep = true
record_dir = "$STATE/rec"
socket = "$STATE/map.sock"
TOML
  scene=${SCENE:-$MICRODUCK_RL/src/mjlab_microduck/robot/microduck/scene_apartment.xml}
  VIEWER_DIR=${VIEWER_DIR:-$HERE/viewer}
  if [ "${VIEWER:-on}" = on ]; then
    # The overlay reads the map on quack-navd's map socket (robotd's own
    # dialect) and the plan, the rays and the guard's lane from
    # quack-navd's robot.map_status; both come up later, and both retry.
    ( cd $MICRODUCK_RL && PYTHONPATH=src MAP_FALLBACK= QUACK_NAV_SOCKET=$STATE/nav.sock \
        detach $STATE/body.log $MICRODUCK_RL/.venv/bin/mjpython $VIEWER_DIR/body_with_map.py \
        --port $PORT --ducks 1 --keyframe SIT --scene $scene --robot-socket $STATE/map.sock ) > $STATE/body.pid
  else
    ( cd $MICRODUCK_RL && PYTHONPATH=src \
        detach $STATE/body.log $MICRODUCK_RL/.venv/bin/mjpython -m mjlab_microduck.sim.body_server \
        --port $PORT --ducks 1 --keyframe SIT --scene $scene ) > $STATE/body.pid
  fi
  for i in $(seq 1 120); do
    /usr/bin/python3 -c "import socket,sys;s=socket.socket();s.settimeout(.3);sys.exit(s.connect_ex(('127.0.0.1',$PORT)))" 2>/dev/null && break
    sleep 0.5
  done
  detach $STATE/tofd.log $MICRODUCK/target/debug/tofd --sim 127.0.0.1:$PORT --socket $TOFSOCK --hz 15 > $STATE/tofd.pid
  sleep 1
  DUCK_IDENTITY=duck-m DUCK_RUNTIME_DIR=$STATE ORT_DYLIB_PATH="$ORT" RUST_LOG=info \
    detach $STATE/robotd.log $MICRODUCK/target/debug/robotd --sim 127.0.0.1:$PORT \
    --params $STATE/robotd.toml --socket $SOCK > $STATE/robotd.pid
  for i in $(seq 1 150); do [ -S $SOCK ] && break; sleep 0.2; done
  detach $STATE/navd.log $REPO/target/release/quack-navd $STATE/quack-nav.toml > $STATE/navd.pid
  for i in $(seq 1 40); do [ -S $STATE/nav.sock ] && break; sleep 0.25; done
  echo "up: body $(cat $STATE/body.pid) tofd $(cat $STATE/tofd.pid) robotd $(cat $STATE/robotd.pid) quack-navd $(cat $STATE/navd.pid)"
  echo "sockets: robotd $SOCK · navigation $STATE/nav.sock · map $STATE/map.sock"
  echo "next: $0 enable"
  ;;
enable)
  /usr/bin/python3 $HERE/call.py --robotd $SOCK robot.enable '{"on": true}'
  ;;
down)
  for n in navd robotd tofd body; do
    [ -f $STATE/$n.pid ] || continue
    p=$(cat $STATE/$n.pid)
    if ps -p $p >/dev/null 2>&1; then echo "stop $n ($p)"; kill $p; fi
    rm -f $STATE/$n.pid
  done
  ;;
*) echo "usage: $0 {up|enable|down}"; exit 2 ;;
esac

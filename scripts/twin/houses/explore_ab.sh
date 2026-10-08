#!/bin/zsh
# The exploration A/B: frozen main against a branch, each exploring every
# house from nothing (sessions of SESSION_S, at most SESSIONS), then ROUNDS
# rounds of journeys on its own books. Two twins at a time, one per arm on
# the same house, so both share the host's load.
#
#   MAIN_REPO=<built main> DEV_REPO=<built branch> TWIN_WORK=<dir> \
#     [HOUSES="apartment casa_arredata casa_libera"] [SESSION_S=1800] \
#     [SESSIONS=4] [ROUNDS=2] zsh explore_ab.sh
#
# Results in TWIN_WORK/E/{main,dev}/<house>/ (run.log: coverage per room
# per session, falls, the rounds). Per arm, MAIN_/DEV_ + MICRODUCK,
# MICRODUCK_RL, POLICY_DIR or GAIT put that arm on another stack or walk.
: "${MICRODUCK:?set MICRODUCK, MICRODUCK_RL and POLICY_DIR as for twin.sh}"
ROBOT=$MICRODUCK_RL/src/mjlab_microduck/robot/microduck
W=${TWIN_WORK:?a work dir}
MAIN=${MAIN_REPO:?a built checkout of main}
DEV=${DEV_REPO:?a built checkout of the branch}
TRUTH=$(cd "$(dirname "$0")" && pwd)
for h in ${=HOUSES:-apartment casa_arredata casa_libera}; do
  mkdir -p $W/E/main/$h $W/E/dev/$h
  ( export QN_REPO=$MAIN TWIN_WORK=$W MICRODUCK=${MAIN_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${MAIN_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${MAIN_POLICY_DIR:-$POLICY_DIR} GAIT=${MAIN_GAIT:-alpha} VIEWER=off AB_REPO=/nonexistent
    python3 $MAIN/scripts/twin/houses/final_house.py $h ${MAIN_MICRODUCK_RL:-$MICRODUCK_RL}/src/mjlab_microduck/robot/microduck/scene_$h.xml /tmp/qa 7881 $TRUTH/$h.truth.json $W/E/main/$h ${SESSION_S:-1800} ${SESSIONS:-4} ${ROUNDS:-2} > $W/E/main/$h/run.log 2>&1 ) &
  ( export QN_REPO=$DEV TWIN_WORK=$W MICRODUCK=${DEV_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${DEV_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${DEV_POLICY_DIR:-$POLICY_DIR} GAIT=${DEV_GAIT:-alpha} VIEWER=on AB_REPO=/nonexistent
    python3 $DEV/scripts/twin/houses/final_house.py $h ${DEV_MICRODUCK_RL:-$MICRODUCK_RL}/src/mjlab_microduck/robot/microduck/scene_$h.xml /tmp/qb 7882 $TRUTH/$h.truth.json $W/E/dev/$h ${SESSION_S:-1800} ${SESSIONS:-4} ${ROUNDS:-2} > $W/E/dev/$h/run.log 2>&1 ) &
  wait
  echo "$h done $(date)" >> $W/E/done.log
done
echo "all done $(date)" >> $W/E/done.log

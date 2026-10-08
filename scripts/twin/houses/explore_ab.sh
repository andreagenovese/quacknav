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
# per session, falls, the rounds).
: "${MICRODUCK:?set MICRODUCK, MICRODUCK_RL and POLICY_DIR as for twin.sh}"
ROBOT=$MICRODUCK_RL/src/mjlab_microduck/robot/microduck
W=${TWIN_WORK:?a work dir}
MAIN=${MAIN_REPO:?a built checkout of main}
DEV=${DEV_REPO:?a built checkout of the branch}
TRUTH=$(cd "$(dirname "$0")" && pwd)
for h in ${=HOUSES:-apartment casa_arredata casa_libera}; do
  mkdir -p $W/E/main/$h $W/E/dev/$h
  ( export QN_REPO=$MAIN TWIN_WORK=$W VIEWER=off AB_REPO=/nonexistent
    python3 $MAIN/scripts/twin/houses/final_house.py $h $ROBOT/scene_$h.xml /tmp/qa 7881 $TRUTH/$h.truth.json $W/E/main/$h ${SESSION_S:-1800} ${SESSIONS:-4} ${ROUNDS:-2} > $W/E/main/$h/run.log 2>&1 ) &
  ( export QN_REPO=$DEV TWIN_WORK=$W VIEWER=on AB_REPO=/nonexistent
    python3 $DEV/scripts/twin/houses/final_house.py $h $ROBOT/scene_$h.xml /tmp/qb 7882 $TRUTH/$h.truth.json $W/E/dev/$h ${SESSION_S:-1800} ${SESSIONS:-4} ${ROUNDS:-2} > $W/E/dev/$h/run.log 2>&1 ) &
  wait
  echo "$h done $(date)" >> $W/E/done.log
done
echo "all done $(date)" >> $W/E/done.log

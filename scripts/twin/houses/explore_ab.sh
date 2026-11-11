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
# One A/B at a time on these twins: both arms use /tmp/qa and /tmp/qb and
# ports 7881–7882, and a second A/B started beside a first wipes their
# state and saves its house's map into the other's run (e24, 2026-11-01:
# an outer loop left running after a stop ran casa_arredata beside a
# restarted casa_ingombra, whose map came out under the other name).
LOCK=/tmp/quack-twin-ab.lock
if ! mkdir $LOCK 2>/dev/null; then
  holder=$(cat $LOCK/pid 2>/dev/null)
  if [ -n "$holder" ] && kill -0 $holder 2>/dev/null; then
    echo "explore_ab: another A/B (pid $holder) holds the twins; not starting" >&2
    exit 1
  fi
  echo "explore_ab: a stale lock (pid ${holder:-none} gone); taking it" >&2
fi
echo $$ > $LOCK/pid
trap 'rm -rf $LOCK' EXIT INT TERM
# SLOT_OFFSET=1 starts with the arms swapped (a second run of the same houses).
k=${SLOT_OFFSET:-0}
for h in ${=HOUSES:-apartment casa_arredata casa_libera}; do
  # The arms take turns on the two twins, house by house (SLOTS=fixed:
  # main always /tmp/qa:7881): first sessions ending early 'stuck' had
  # come mostly from the second twin, 5 of 12 against 0 of 12 (FM/ex*,
  # 2026-11-10), not reproduced with the same code on both (SLOT, 0 of 4
  # each, 2026-11-11) — unexplained, so neither arm keeps one twin.
  k=$(( k + 1 ))
  if [ "${SLOTS:-alternate}" != fixed ] && [ $(( k % 2 )) -eq 0 ]; then MS=/tmp/qb MP=7882 DS=/tmp/qa DP=7881; else MS=/tmp/qa MP=7881 DS=/tmp/qb DP=7882; fi
  mkdir -p $W/E; echo "$h: main on $MS:$MP, dev on $DS:$DP" >> $W/E/slots.log
  mkdir -p $W/E/main/$h $W/E/dev/$h
  # Each house from nothing: a map left from the house before is tried at
  # boot, and a duck unsure of where it is refuses to start exploring.
  rm -rf /tmp/qa /tmp/qb
  ( export QN_REPO=$MAIN TWIN_WORK=$W MICRODUCK=${MAIN_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${MAIN_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${MAIN_POLICY_DIR:-$POLICY_DIR} GAIT=${MAIN_GAIT:-alpha} VIEWER=on AB_REPO=/nonexistent
    python3 $MAIN/scripts/twin/houses/final_house.py $h ${MAIN_MICRODUCK_RL:-$MICRODUCK_RL}/src/mjlab_microduck/robot/microduck/scene_$h.xml $MS $MP $TRUTH/$h.truth.json $W/E/main/$h ${SESSION_S:-1800} ${SESSIONS:-4} ${ROUNDS:-2} > $W/E/main/$h/run.log 2>&1 ) &
  ( export QN_REPO=$DEV TWIN_WORK=$W MICRODUCK=${DEV_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${DEV_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${DEV_POLICY_DIR:-$POLICY_DIR} GAIT=${DEV_GAIT:-alpha} VIEWER=on AB_REPO=/nonexistent
    python3 $DEV/scripts/twin/houses/final_house.py $h ${DEV_MICRODUCK_RL:-$MICRODUCK_RL}/src/mjlab_microduck/robot/microduck/scene_$h.xml $DS $DP $TRUTH/$h.truth.json $W/E/dev/$h ${SESSION_S:-1800} ${SESSIONS:-4} ${ROUNDS:-2} > $W/E/dev/$h/run.log 2>&1 ) &
  wait
  echo "$h done $(date)" >> $W/E/done.log
done
echo "all done $(date)" >> $W/E/done.log

#!/bin/zsh
# Identical tests, frozen main against overhang: the same books, the same
# rounds, two twins at a time (one per arm), so both share the host's load.
#
#   MAIN_REPO=<built main> DEV_REPO=<built branch> TWIN_WORK=<dir> \
#     [TWIN_AB=<rl-nav's scripts/rl/twin_ab.py>] zsh final_ab.sh
#
# Per arm, MAIN_/DEV_ + MICRODUCK, MICRODUCK_RL, POLICY_DIR or GAIT put that
# arm on another stack or walk (the shared ones otherwise).
# Both arms run the same way: the viewer on in both (VIEWER=off in both),
# not, as until 2026-11-11, off for main and on for the branch.
# TWIN_WORK/C/<house>/ holds the explored books (ground-after-explore.json,
# maps-after-explore/); the results land in TWIN_WORK/F/{main,dev}/.
# With TWIN_AB, casa_ingombra too (two rounds a side, main's oracle knobs).
: "${MICRODUCK:?set MICRODUCK, MICRODUCK_RL and POLICY_DIR as for twin.sh}"
ROBOT=$MICRODUCK_RL/src/mjlab_microduck/robot/microduck
W=${TWIN_WORK:?the work dir with C/<house>/ground-after-explore.json and maps-after-explore}
MAIN=${MAIN_REPO:?a built checkout of main}
DEV=${DEV_REPO:?a built checkout of the branch}
TRUTH=$(cd "$(dirname "$0")" && pwd)
# SLOT_OFFSET=1 starts with the arms swapped (a second run of the same houses).
k=${SLOT_OFFSET:-0}
for h in apartment casa_arredata; do
  # The arms take turns on the two twins, house by house (SLOTS=fixed:
  # main always /tmp/qa:7881): first sessions ending early 'stuck' had
  # come mostly from the second twin, 5 of 12 against 0 of 12 (FM/ex*,
  # 2026-11-10), not reproduced with the same code on both (SLOT, 0 of 4
  # each, 2026-11-11) — unexplained, so neither arm keeps one twin.
  k=$(( k + 1 ))
  if [ "${SLOTS:-alternate}" != fixed ] && [ $(( k % 2 )) -eq 0 ]; then MS=/tmp/qb MP=7882 DS=/tmp/qa DP=7881; else MS=/tmp/qa MP=7881 DS=/tmp/qb DP=7882; fi
  mkdir -p $W/F; echo "$h: main on $MS:$MP, dev on $DS:$DP" >> $W/F/slots.log
  for arm in main dev; do
    mkdir -p $W/F/$arm/$h; cp -R $W/C/$h/ground-after-explore.json $W/C/$h/maps-after-explore $W/F/$arm/$h/
  done
  ( export QN_REPO=$MAIN TWIN_WORK=$W MICRODUCK=${MAIN_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${MAIN_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${MAIN_POLICY_DIR:-$POLICY_DIR} GAIT=${MAIN_GAIT:-alpha} VIEWER=${VIEWER:-on} ROUNDS_ONLY=1 AB_REPO=/nonexistent
    python3 $MAIN/scripts/twin/houses/final_house.py $h ${MAIN_MICRODUCK_RL:-$MICRODUCK_RL}/src/mjlab_microduck/robot/microduck/scene_$h.xml $MS $MP $TRUTH/$h.truth.json $W/F/main/$h 1800 4 4 > $W/F/main/$h/run.log 2>&1 ) &
  ( export QN_REPO=$DEV TWIN_WORK=$W MICRODUCK=${DEV_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${DEV_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${DEV_POLICY_DIR:-$POLICY_DIR} GAIT=${DEV_GAIT:-alpha} VIEWER=${VIEWER:-on} ROUNDS_ONLY=1 AB_REPO=/nonexistent
    python3 $DEV/scripts/twin/houses/final_house.py $h ${DEV_MICRODUCK_RL:-$MICRODUCK_RL}/src/mjlab_microduck/robot/microduck/scene_$h.xml $DS $DP $TRUTH/$h.truth.json $W/F/dev/$h 1800 4 4 > $W/F/dev/$h/run.log 2>&1 ) &
  wait
  echo "$h done $(date)" >> $W/F/done.log
done
[ -n "$TWIN_AB" ] && ( export MICRODUCK=${MAIN_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${MAIN_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${MAIN_POLICY_DIR:-$POLICY_DIR} GAIT=${MAIN_GAIT:-alpha} TWIN_REPO=$MAIN STATE=/tmp/qc PORT=7883 VIEWER=${VIEWER:-on}
  python3 $TWIN_AB $W/F/main/ingombra none --arms stick --rounds 2 --main-oracle > $W/F/main/ingombra.log 2>&1 ) &
[ -n "$TWIN_AB" ] && ( export MICRODUCK=${DEV_MICRODUCK:-$MICRODUCK} MICRODUCK_RL=${DEV_MICRODUCK_RL:-$MICRODUCK_RL} POLICY_DIR=${DEV_POLICY_DIR:-$POLICY_DIR} GAIT=${DEV_GAIT:-alpha} TWIN_REPO=$DEV STATE=/tmp/qd PORT=7884 VIEWER=${VIEWER:-on}
  python3 $TWIN_AB $W/F/dev/ingombra none --arms stick --rounds 2 --main-oracle > $W/F/dev/ingombra.log 2>&1 ) &
wait
echo "all done $(date)" >> $W/F/done.log

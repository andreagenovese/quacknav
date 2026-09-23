#!/bin/zsh
# One A/B round: the same scripted stop-and-scan route on a robotd that
# hosts maploc itself (the fork) and on the released robotd + quack-navd,
# side by side, each recording scored against the apartment's true walls.
#
#   scripts/twin/ab_round.sh <n>
#
# Needs twin.sh's variables, plus FORK_TWIN: a checkout of the fork's PR 202
# branch whose try-maploc-local.sh runs the fork twin on /tmp/dsm, port 7871,
# recording into $FORK_TWIN/recordings. Results in $STATE/ab/r<n>.
# 2026-09-23, two rounds: walls 0.028/0.034 m on the fork, 0.032/0.032 here.
set -eu
HERE=${0:A:h}; REPO=${HERE:h:h}
STATE=${STATE:-/tmp/quack-twin}; PORT=${PORT:-7872}
: ${FORK_TWIN:?set FORK_TWIN (see scripts/twin/README.md)}
R=$STATE/ab/r$1; mkdir -p $R; touch $R/.start
(cd $FORK_TWIN && ./try-maploc-local.sh up >/dev/null 2>&1)
$HERE/twin.sh up >/dev/null
sleep 3
/usr/bin/python3 $HERE/call.py --robotd /tmp/dsm/a.sock robot.enable '{"on": true}' >/dev/null
$HERE/twin.sh enable >/dev/null
sleep 15
/usr/bin/python3 $HERE/scan_walk.py 300 /tmp/dsm/a.sock 7871 > $R/walk-fork.txt 2>&1 &
w1=$!
/usr/bin/python3 $HERE/scan_walk.py 300 $STATE/robotd.sock $PORT > $R/walk-navd.txt 2>&1 &
w2=$!
# The two walks only: a bare `wait` would also wait for whatever else this
# shell started, and hang.
wait $w1 $w2
(cd $FORK_TWIN && ./try-maploc-local.sh down >/dev/null 2>&1)
$HERE/twin.sh down >/dev/null
fork=$(find $FORK_TWIN/recordings -name '*.mdlg' -newer $R/.start | sort | tail -1)
navd=$(find $STATE/rec -name '*.mdlg' -newer $R/.start | sort | tail -1)
T=$FORK_TWIN/sim-maploc/apartment.toml
cd $REPO
for side in fork navd; do
  rec=${(P)side}
  cargo run -q -p maploc --release --features kinematics --example evaluate -- $rec $T $R/eval-$side > $R/eval-$side.txt 2>&1
  echo "$side: $(grep -E 'map walls vs room' $R/eval-$side.txt)"
done

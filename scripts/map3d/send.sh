#!/usr/bin/env bash
# Put a 3D house built by build.py on the duck, for quack-control's 3D tab.
#
# Usage: send.sh [--dry-run] <user@host> [house3d.bin]
#
# The file goes to /var/lib/quack-control/house3d.bin (quack-control's
# `house_file`), which quack-control reads again when it changes: the page's
# ↻ shows the new house, nothing to restart. build.py --send <user@host>
# runs this right after building. --dry-run prints the ssh and scp lines
# and connects to nothing. Extra ssh/scp options, e.g. a port:
#   SSH_OPTS="-p 2222" scripts/map3d/send.sh microduck@192.168.1.42 out/house3d.bin
# The user needs sudo on the duck.
set -euo pipefail

USAGE="usage: send.sh [--dry-run] <user@host> [house3d.bin]"
DRY_RUN=0
if [ "${1:-}" = "--dry-run" ]; then DRY_RUN=1; shift; fi
HOST="${1:?$USAGE}"
FILE="${2:-house3d.bin}"
DEST=/var/lib/quack-control/house3d.bin
STAGE=/tmp/house3d.bin.$$
# ${A[@]+"${A[@]}"} below: macOS bash 3.2 calls an empty array unbound under set -u.
read -ra OPTS <<< "${SSH_OPTS:-}"
# scp spells the port -P where ssh spells it -p.
SCP_OPTS=()
for o in ${OPTS[@]+"${OPTS[@]}"}; do
    if [ "$o" = "-p" ]; then SCP_OPTS+=("-P"); else SCP_OPTS+=("$o"); fi
done

[ -f "$FILE" ] || { echo "missing $FILE — build it with scripts/map3d/build.py" >&2; exit 1; }
[ "$(head -c 4 "$FILE")" = QH3D ] || { echo "$FILE is not a house3d.bin" >&2; exit 1; }

run() {
    if [ "$DRY_RUN" = 1 ]; then printf '%q ' "$@"; echo; else "$@"; fi
}

run scp ${SCP_OPTS[@]+"${SCP_OPTS[@]}"} -q "$FILE" "$HOST:$STAGE"
run ssh ${OPTS[@]+"${OPTS[@]}"} "$HOST" "sudo install -D -m 644 $STAGE $DEST && rm -f $STAGE"
[ "$DRY_RUN" = 1 ] || echo "$FILE -> $HOST:$DEST ($(wc -c < "$FILE" | tr -d ' ') bytes); the 3D tab's ↻ shows it"

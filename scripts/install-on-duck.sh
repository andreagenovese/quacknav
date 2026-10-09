#!/usr/bin/env bash
# Install or upgrade quack-navd on the duck, over ssh.
#
# Usage: install-on-duck.sh [--dry-run] <user@host> [binary]
#
# It runs from either of two places, and finds its files by its own:
#   - an unpacked release package (quack-nav-<version>/, see
#     scripts/package.sh): bin/quack-navd sits next to this script, and so
#     do systemd/ and quack-nav.example.toml;
#   - a checkout of the repository (scripts/install-on-duck.sh): the binary
#     defaults to target/aarch64-unknown-linux-gnu/release/quack-navd
#     (scripts/cross-build.sh), the rest is quack-nav/systemd/ and
#     quack-nav/quack-nav.example.toml.
# A [binary] given on the command line wins over both.
#
# --dry-run prints every command it would run — the ssh and scp lines and
# the script it would run on the duck — and connects to nothing.
# Extra ssh/scp options, e.g. a port:
#   SSH_OPTS="-p 2222" ./install-on-duck.sh microduck@192.168.1.42
#
# Idempotent, and it replaces only what it owns: the binary
# (/usr/local/bin/quack-navd), the unit (/etc/systemd/system/
# quack-navd.service) and the service account (/etc/sysusers.d/
# quack-nav.conf). The config (/etc/robot/quack-nav.toml) is installed
# only when there is none; the pilots go to /var/lib/quack-nav/pilots/
# <walk>/pilot.json (used only when QK_RL_POLICY names that directory);
# an old /var/lib/quacksat/places.json is copied,
# never moved, and only when /var/lib/quack-nav/ has none. Then the
# service is enabled and (re)started. The user needs sudo on the duck.
set -euo pipefail

USAGE="usage: install-on-duck.sh [--dry-run] <user@host> [binary]"
DRY_RUN=0
if [ "${1:-}" = "--dry-run" ]; then DRY_RUN=1; shift; fi
HOST="${1:?$USAGE}"

HERE="$(cd "$(dirname "$0")" && pwd)"
if [ -f "$HERE/bin/quack-navd" ]; then
    # The release package's layout.
    DEFAULT_BIN="$HERE/bin/quack-navd"
    UNIT="$HERE/systemd/quack-navd.service"
    SYSUSERS="$HERE/systemd/sysusers.d/quack-nav.conf"
    CONFIG="$HERE/quack-nav.example.toml"
    PILOTS=("alpha:$HERE/pilots/alpha/pilot.json" "velstand:$HERE/pilots/velstand/pilot.json")
    BUILD_HINT="the package is incomplete: unpack it again"
else
    # The repository's layout (this script in scripts/).
    ROOT="$(cd "$HERE/.." && pwd)"
    DEFAULT_BIN="$ROOT/target/aarch64-unknown-linux-gnu/release/quack-navd"
    UNIT="$ROOT/quack-nav/systemd/quack-navd.service"
    SYSUSERS="$ROOT/quack-nav/systemd/sysusers.d/quack-nav.conf"
    CONFIG="$ROOT/quack-nav/quack-nav.example.toml"
    PILOTS=("alpha:$ROOT/quack-rl/pilots/v3-r7-mujoco/pilot.json" "velstand:$ROOT/quack-rl/pilots/v3-r7-velstand/pilot.json")
    BUILD_HINT="run scripts/cross-build.sh first"
fi
BIN="${2:-$DEFAULT_BIN}"
# ${A[@]+"${A[@]}"} below: macOS bash 3.2 calls an empty array unbound under set -u.
read -ra OPTS <<< "${SSH_OPTS:-}"

[ -f "$BIN" ] || { echo "missing $BIN — $BUILD_HINT" >&2; exit 1; }
for f in "$UNIT" "$SYSUSERS" "$CONFIG"; do
    [ -f "$f" ] || { echo "missing $f — $BUILD_HINT" >&2; exit 1; }
done
if command -v file >/dev/null && ! file "$BIN" | grep -q 'ARM aarch64'; then
    echo "$BIN is not an aarch64 binary: $(file -b "$BIN")" >&2
    exit 1
fi

SCP_OPTS=()
for ((i = 0; i < ${#OPTS[@]}; i++)); do
    # scp spells ssh's -p (port) as -P.
    if [ "${OPTS[i]}" = "-p" ]; then SCP_OPTS+=("-P"); else SCP_OPTS+=("${OPTS[i]}"); fi
done

# Run a local command, or with --dry-run only print it.
step() {
    if [ "$DRY_RUN" = 1 ]; then
        printf '+'; printf ' %q' "$@"; printf '\n'
    else
        "$@"
    fi
}

if [ "$DRY_RUN" = 1 ]; then
    echo "dry run: nothing is copied and nothing runs on $HOST"
    STAGE=/tmp/quack-nav-install.XXXXXX
    step ssh ${OPTS[@]+"${OPTS[@]}"} "$HOST" mktemp -d /tmp/quack-nav-install.XXXXXX
else
    STAGE=$(ssh ${OPTS[@]+"${OPTS[@]}"} "$HOST" mktemp -d /tmp/quack-nav-install.XXXXXX)
fi
echo "copying to $HOST:$STAGE"
step scp ${SCP_OPTS[@]+"${SCP_OPTS[@]}"} -q "$BIN" "$HOST:$STAGE/quack-navd"
step scp ${SCP_OPTS[@]+"${SCP_OPTS[@]}"} -q "$UNIT" "$HOST:$STAGE/quack-navd.service"
step scp ${SCP_OPTS[@]+"${SCP_OPTS[@]}"} -q "$SYSUSERS" "$HOST:$STAGE/sysusers.conf"
step scp ${SCP_OPTS[@]+"${SCP_OPTS[@]}"} -q "$CONFIG" "$HOST:$STAGE/quack-nav.toml"
# The pilots, one per walk (docs/rl-pilot.md); off until QK_RL_POLICY names
# their directory, /var/lib/quack-nav/pilots.
for p in "${PILOTS[@]}"; do
    walk="${p%%:*}"; file="${p#*:}"
    [ -f "$file" ] && step scp ${SCP_OPTS[@]+"${SCP_OPTS[@]}"} -q "$file" "$HOST:$STAGE/pilot-$walk.json"
done

# shellcheck disable=SC2087 # $STAGE is expanded here on purpose
REMOTE=$(cat <<REMOTE
set -euo pipefail
cd "$STAGE"
run() { echo "+ \$*"; sudo "\$@"; }

run install -m 755 quack-navd /usr/local/bin/quack-navd
run install -m 644 quack-navd.service /etc/systemd/system/quack-navd.service
run install -D -m 644 sysusers.conf /etc/sysusers.d/quack-nav.conf
run systemd-sysusers /etc/sysusers.d/quack-nav.conf
getent group robot >/dev/null \\
    || echo "warning: no 'robot' group: robotd's socket is not provisioned here, and the unit's SupplementaryGroups=robot will fail"

if [ -f /etc/robot/quack-nav.toml ]; then
    echo "keeping /etc/robot/quack-nav.toml"
else
    run install -D -m 644 quack-nav.toml /etc/robot/quack-nav.toml
fi

if [ -f /var/lib/quacksat/places.json ] && ! sudo test -f /var/lib/quack-nav/places.json; then
    run install -d -o quacknav -g quacknav -m 755 /var/lib/quack-nav
    run install -o quacknav -g quacknav -m 644 /var/lib/quacksat/places.json /var/lib/quack-nav/places.json
    echo "copied the old places registry; /var/lib/quacksat/places.json can go once quack-navd lists its places"
fi

for walk in alpha velstand; do
    if [ -f "pilot-\$walk.json" ]; then
        run install -D -o quacknav -g quacknav -m 644 "pilot-\$walk.json" "/var/lib/quack-nav/pilots/\$walk/pilot.json"
    fi
done

run systemctl daemon-reload
run systemctl enable quack-navd
run systemctl restart quack-navd
sleep 2
sudo systemctl --no-pager --lines=5 status quack-navd || true
rm -rf "$STAGE"
REMOTE
)

if [ "$DRY_RUN" = 1 ]; then
    step ssh ${OPTS[@]+"${OPTS[@]}"} "$HOST" bash -s
    echo "  with this script on its standard input:"
    printf '%s\n' "$REMOTE" | sed 's/^/  | /'
    exit 0
fi
ssh ${OPTS[@]+"${OPTS[@]}"} "$HOST" bash -s <<< "$REMOTE"

echo "installed on $HOST — follow it with: ssh $HOST journalctl -u quack-navd -f"

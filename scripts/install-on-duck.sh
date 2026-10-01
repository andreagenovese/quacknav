#!/usr/bin/env bash
# Install or upgrade quack-navd on the duck, over ssh.
#
# Usage: scripts/install-on-duck.sh <user@host> [binary]
#   binary defaults to target/aarch64-unknown-linux-gnu/release/quack-navd
#   (scripts/cross-build.sh). Extra ssh/scp options, e.g. a port:
#   SSH_OPTS="-p 2222" scripts/install-on-duck.sh radxa@192.168.1.42
#
# Idempotent, and it replaces only what it owns: the binary
# (/usr/local/bin/quack-navd), the unit (/etc/systemd/system/
# quack-navd.service) and the service account (/etc/sysusers.d/
# quack-nav.conf). The config (/etc/robot/quack-nav.toml) is installed
# only when there is none; an old /var/lib/quacksat/places.json is copied,
# never moved, and only when /var/lib/quack-nav/ has none. Then the
# service is enabled and (re)started. The user needs sudo on the duck.
set -euo pipefail

HOST="${1:?usage: install-on-duck.sh <user@host> [binary]}"
cd "$(dirname "$0")/.."
BIN="${2:-target/aarch64-unknown-linux-gnu/release/quack-navd}"
read -ra OPTS <<< "${SSH_OPTS:-}"

[ -f "$BIN" ] || { echo "missing $BIN — run scripts/cross-build.sh first" >&2; exit 1; }
if command -v file >/dev/null && ! file "$BIN" | grep -q 'ARM aarch64'; then
    echo "$BIN is not an aarch64 binary: $(file -b "$BIN")" >&2
    exit 1
fi

SCP_OPTS=()
for ((i = 0; i < ${#OPTS[@]}; i++)); do
    # scp spells ssh's -p (port) as -P.
    if [ "${OPTS[i]}" = "-p" ]; then SCP_OPTS+=("-P"); else SCP_OPTS+=("${OPTS[i]}"); fi
done

STAGE=$(ssh "${OPTS[@]}" "$HOST" 'mktemp -d /tmp/quack-nav-install.XXXXXX')
echo "copying to $HOST:$STAGE"
scp "${SCP_OPTS[@]}" -q "$BIN" "$HOST:$STAGE/quack-navd"
scp "${SCP_OPTS[@]}" -q quack-nav/systemd/quack-navd.service "$HOST:$STAGE/quack-navd.service"
scp "${SCP_OPTS[@]}" -q quack-nav/systemd/sysusers.d/quack-nav.conf "$HOST:$STAGE/sysusers.conf"
scp "${SCP_OPTS[@]}" -q quack-nav/quack-nav.example.toml "$HOST:$STAGE/quack-nav.toml"

# shellcheck disable=SC2087 # $STAGE is expanded here on purpose
ssh "${OPTS[@]}" "$HOST" bash -s <<REMOTE
set -euo pipefail
cd "$STAGE"
run() { echo "+ \$*"; sudo "\$@"; }

run install -m 755 quack-navd /usr/local/bin/quack-navd
run install -m 644 quack-navd.service /etc/systemd/system/quack-navd.service
run install -D -m 644 sysusers.conf /etc/sysusers.d/quack-nav.conf
run systemd-sysusers /etc/sysusers.d/quack-nav.conf
getent group robot >/dev/null \
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

run systemctl daemon-reload
run systemctl enable quack-navd
run systemctl restart quack-navd
sleep 2
sudo systemctl --no-pager --lines=5 status quack-navd || true
rm -rf "$STAGE"
REMOTE

echo "installed on $HOST — follow it with: ssh $HOST journalctl -u quack-navd -f"

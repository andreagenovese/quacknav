#!/usr/bin/env bash
# Pack the duck's install package: everything install-on-duck.sh needs,
# so a release installs without a checkout of this repository.
#
# Usage: scripts/package.sh <version> <binary> <outdir>
#   version  e.g. 0.2.0-rc2 (CI: the tag without its v)
#   binary   the aarch64 quack-navd (scripts/cross-build.sh:
#            target/aarch64-unknown-linux-gnu/release/quack-navd)
#   outdir   where quack-nav-<version>-aarch64-linux.tar.gz and its
#            .sha256 are written (created if absent)
#
# The tarball holds one directory, quack-nav-<version>/:
#   bin/quack-navd
#   systemd/quack-navd.service
#   systemd/sysusers.d/quack-nav.conf
#   quack-nav.example.toml
#   install-on-duck.sh            (finds bin/ next to itself)
#   pilots/alpha/, pilots/velstand/  pilot.json + pilot.onnx, one per walk
#   README-install.md, README-install.it.md
#   LICENSE, NOTICE, CHANGELOG.md
# The .sha256 is in `sha256sum` format, checked with `shasum -a 256 -c` or
# `sha256sum -c` from the directory holding both files.
set -euo pipefail

VERSION="${1:?usage: package.sh <version> <binary> <outdir>}"
BIN="${2:?usage: package.sh <version> <binary> <outdir>}"
OUTDIR="${3:?usage: package.sh <version> <binary> <outdir>}"

case "$VERSION" in
    v*) echo "version without the v: ${VERSION#v}" >&2; exit 1 ;;
    *[!A-Za-z0-9.+-]*|'') echo "not a version: $VERSION" >&2; exit 1 ;;
esac
[ -f "$BIN" ] || { echo "missing $BIN — run scripts/cross-build.sh first" >&2; exit 1; }
if command -v file >/dev/null && ! file "$BIN" | grep -q 'ARM aarch64'; then
    echo "$BIN is not an aarch64 binary: $(file -b "$BIN")" >&2
    exit 1
fi

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CARGO_VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/Cargo.toml" | head -1)
[ "$VERSION" = "$CARGO_VERSION" ] \
    || echo "note: packaging $VERSION, Cargo.toml says $CARGO_VERSION" >&2

NAME="quack-nav-$VERSION"
TARBALL="$NAME-aarch64-linux.tar.gz"
mkdir -p "$OUTDIR"
OUTDIR="$(cd "$OUTDIR" && pwd)"
WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT

PKG="$WORK/$NAME"
install -d "$PKG/bin" "$PKG/systemd/sysusers.d"
install -m 755 "$BIN" "$PKG/bin/quack-navd"
install -m 644 "$ROOT/quack-nav/systemd/quack-navd.service" "$PKG/systemd/"
install -m 644 "$ROOT/quack-nav/systemd/sysusers.d/quack-nav.conf" "$PKG/systemd/sysusers.d/"
install -m 644 "$ROOT/quack-nav/quack-nav.example.toml" "$PKG/"
install -m 755 "$ROOT/scripts/install-on-duck.sh" "$PKG/"
# The pilots, one per walk: pilot.json (what quack-navd loads) and its ONNX twin.
for pair in alpha:v3-r7-mujoco velstand:v3-r7-velstand; do
    walk="${pair%%:*}"; run="${pair#*:}"
    install -d "$PKG/pilots/$walk"
    install -m 644 "$ROOT/quack-rl/pilots/$run/pilot.json" "$ROOT/quack-rl/pilots/$run/pilot.onnx" "$PKG/pilots/$walk/"
done
# The install README (scripts/package/), with this package's version in.
for f in README-install.md README-install.it.md; do
    sed "s/@VERSION@/$VERSION/g" "$ROOT/scripts/package/$f" > "$PKG/$f"
    chmod 644 "$PKG/$f"
done
install -m 644 "$ROOT/LICENSE" "$ROOT/CHANGELOG.md" "$PKG/"
[ -f "$ROOT/NOTICE" ] && install -m 644 "$ROOT/NOTICE" "$PKG/"

# Owned by root in the archive, whoever packed it; no macOS ._ files.
if tar --version 2>/dev/null | grep -q 'GNU tar'; then
    tar -C "$WORK" --owner=0 --group=0 --numeric-owner --sort=name -czf "$OUTDIR/$TARBALL" "$NAME"
else
    COPYFILE_DISABLE=1 tar -C "$WORK" --uid 0 --gid 0 --uname root --gname root -czf "$OUTDIR/$TARBALL" "$NAME"
fi

if command -v sha256sum >/dev/null; then
    (cd "$OUTDIR" && sha256sum "$TARBALL" > "$TARBALL.sha256")
else
    (cd "$OUTDIR" && shasum -a 256 "$TARBALL" > "$TARBALL.sha256")
fi
echo "packed $OUTDIR/$TARBALL"
cat "$OUTDIR/$TARBALL.sha256"

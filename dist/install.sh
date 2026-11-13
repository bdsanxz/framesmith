#!/bin/sh
# Framesmith one-command installer (Unix: macOS arm64/x64, Linux x64).
# Installs the packaged binary into PREFIX/bin after verifying the package
# checksum against release metadata. Runs under the maker's own review —
# no admin step required; PREFIX defaults to ~/.local.
#
# Usage: ./install.sh [--prefix DIR] [--force]
#
# The installer:
#   1. Refuses with the supported set when run on an unsupported OS/arch.
#   2. Verifies the binary's SHA-256 against the adjacent checksums file
#      (release metadata) before copying; a mismatch refuses and leaves any
#      previously installed binary untouched.
#   3. Copies the binary (plus notices) into PREFIX/bin and PREFIX/share.
#   4. Does NOT fetch FFmpeg: the installed binary bootstraps FFmpeg on
#      first render through the existing pinned-registry flow. Renders
#      work offline once the tool cache is warm.
#
# Exit 0 = installed and verified. Exit 1 = refused (named cause).

set -eu

PREFIX="${PREFIX:-$HOME/.local}"
FORCE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --prefix) PREFIX="${2:-}"; shift 2 ;;
    --prefix=*) PREFIX="${1#--prefix=}"; shift ;;
    --force) FORCE=1; shift ;;
    -h|--help)
      sed -n '1,24p' "$0"
      echo "usage: ./install.sh [--prefix DIR] [--force]"
      exit 0 ;;
    *) echo "install.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# --- platform gate: refuse naming the supported set -------------------
OS="$(uname -s)"
ARCH="$(uname -m)"

# Normalize to the shipped pairs (macos-arm64, macos-x64, linux-x64).
case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64) TARGET_SLUG="macos-arm64" ;;
      x86_64) TARGET_SLUG="macos-x64" ;;
      *) TARGET_SLUG="" ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64) TARGET_SLUG="linux-x64" ;;
      *) TARGET_SLUG="" ;;
    esac
    ;;
  *)
    TARGET_SLUG="" ;;
esac

if [ -z "$TARGET_SLUG" ]; then
  echo "install.sh: unsupported platform $OS/$ARCH." >&2
  echo "install.sh: supported platforms: macos-arm64; macos-x64; windows-x64; linux-x64" >&2
  echo "install.sh: install the matching package for your OS/arch or build from source (cargo build --bin framesmith)." >&2
  exit 1
fi

# --- checksum verification against release metadata -------------------
# The package ships checksums.txt (release metadata) with one
# "<hex>  <name>" per file. The installer verifies the binary against it
# BEFORE copying; a mismatch refuses and the previous install stays.
BIN_NAME="framesmith"
BIN_SRC="$SCRIPT_DIR/$BIN_NAME"
CHECKSUMS="$SCRIPT_DIR/checksums.txt"

if [ ! -f "$BIN_SRC" ]; then
  echo "install.sh: missing binary $BIN_SRC (corrupt package?)" >&2
  exit 1
fi

if [ ! -f "$CHECKSUMS" ]; then
  echo "install.sh: missing checksums.txt release metadata; refusing to install unverified." >&2
  exit 1
fi

# Extract expected hex for the binary.
EXPECTED=""
while IFS= read -r line; do
  # Skip blanks and comments.
  [ -z "$line" ] && continue
  case "$line" in \#*) continue ;; esac
  hex="$(printf '%s' "$line" | awk '{print $1}')"
  name="$(printf '%s' "$line" | awk '{print $2}')"
  if [ "$name" = "$BIN_NAME" ]; then
    EXPECTED="$hex"
    break
  fi
done < "$CHECKSUMS"

if [ -z "$EXPECTED" ]; then
  echo "install.sh: no checksum entry for $BIN_NAME in checksums.txt; refusing." >&2
  exit 1
fi

# Compute SHA-256 with shasum or sha256sum (both exist on macOS/Linux).
if command -v shasum >/dev/null 2>&1; then
  ACTUAL="$(shasum -a 256 "$BIN_SRC" | awk '{print $1}')"
elif command -v sha256sum >/dev/null 2>&1; then
  ACTUAL="$(sha256sum "$BIN_SRC" | awk '{print $1}')"
else
  echo "install.sh: no shasum/sha256sum found; cannot verify checksum, refusing." >&2
  exit 1
fi

if [ "$ACTUAL" != "$EXPECTED" ]; then
  echo "install.sh: checksum mismatch for $BIN_NAME: expected $EXPECTED but found $ACTUAL." >&2
  echo "install.sh: refusing; the package is corrupt or tampered. No files were copied and any existing install is untouched." >&2
  exit 1
fi

echo "install.sh: checksum verified ($BIN_NAME: $ACTUAL)"

# --- install into PREFIX ---------------------------------------------
BIN_DIR="$PREFIX/bin"
SHARE_DIR="$PREFIX/share/framesmith"

mkdir -p "$BIN_DIR" "$SHARE_DIR"

# Copy + make executable. A symlink is not used: the binary must work
# from any CWD without a dangling link, and replacement is atomic-ish
# (copy to temp then rename).
TMP_BIN="$BIN_DIR/.framesmith.tmp.$$"
cp "$BIN_SRC" "$TMP_BIN"
chmod 755 "$TMP_BIN"
# Strip quarantine/quarantine-adjacent metadata on macOS (best effort).
if [ "$OS" = "Darwin" ]; then
  xattr -cr "$TMP_BIN" 2>/dev/null || true
fi
mv -f "$TMP_BIN" "$BIN_DIR/framesmith"

echo "install.sh: installed $BIN_DIR/framesmith"

# Notices travel with the binary (third-party / GPL ffmpeg-registry note).
if [ -f "$SCRIPT_DIR/THIRD-PARTY-NOTICES.md" ]; then
  cp "$SCRIPT_DIR/THIRD-PARTY-NOTICES.md" "$SHARE_DIR/" 2>/dev/null || true
fi
if [ -f "$SCRIPT_DIR/LICENSE" ]; then
  cp "$SCRIPT_DIR/LICENSE" "$SHARE_DIR/" 2>/dev/null || true
fi
if [ -f "$SCRIPT_DIR/README.md" ]; then
  cp "$SCRIPT_DIR/README.md" "$SHARE_DIR/" 2>/dev/null || true
fi

# Verify the installed binary runs (version check).
if "$BIN_DIR/framesmith" --version >/dev/null 2>&1; then
  echo "install.sh: $("$BIN_DIR/framesmith" --version 2>/dev/null || echo "framesmith installed")"
else
  echo "install.sh: installed but version check failed; rendering may still work via bootstrap." >&2
fi

# Make the first render work without manual FFmpeg provisioning: the
# binary already bootstraps FFmpeg on first render. The installer does
# not fetch FFmpeg itself; it confirms the binary is reachable.
echo "install.sh: render a project with: $BIN_DIR/framesmith render <folder>"
echo "install.sh: first render will download FFmpeg via the pinned registry if the host has none (network required once), then render offline."
echo "install.sh: add $BIN_DIR to PATH if needed: export PATH=\"$BIN_DIR:\$PATH\""

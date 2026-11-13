#!/bin/sh
# Build one per-OS installer package for Framesmith (Sprint 27 T001 / CAP-001).
#
# Packages are built ONLY from tagged releases (REC-001): without a tag
# that matches the version, the build refuses. Each package ships the
# binary plus launchers and third-party notices (incl. the GPL
# ffmpeg-registry note), and installs with one command per OS via the
# bundled installer, which verifies the package checksum against release
# metadata before copying.
#
# Usage:
#   build-package.sh --os {macos|windows|linux} --arch {arm64|x64}
#                    --version <ver> --bin <path> [--out DIR]
#   build-package.sh --verify-set --version <ver> --out DIR
#
# --version is the bare version without a leading `v` (e.g. 0.1.0). When
# omitted, the version is derived from the git tag on HEAD; an untagged
# HEAD refuses (not a tagged release).
#
# The binary is the freshly built framesmith binary. On Windows the binary
# must be `framesmith.exe`; elsewhere `framesmith`. The script renames /
# copies it to the installed name and makes it executable.
#
# Sets:
#   build        assemble one package and its checksums.txt
#   verify-set   check that all four required assets exist for the version;
#                an incomplete set refuses naming every missing OS slug.

set -eu

OS=""
ARCH=""
VERSION=""
BIN=""
OUT_DIR=""
ACTION="build"
VERBOSE=0

while [ $# -gt 0 ]; do
  case "$1" in
    --os) OS="${2:-}"; shift 2 ;;
    --os=*) OS="${1#--os=}"; shift ;;
    --arch) ARCH="${2:-}"; shift 2 ;;
    --arch=*) ARCH="${1#--arch=}"; shift ;;
    --version) VERSION="${2:-}"; shift 2 ;;
    --version=*) VERSION="${1#--version=}"; shift ;;
    --bin) BIN="${2:-}"; shift 2 ;;
    --bin=*) BIN="${1#--bin=}"; shift ;;
    --out) OUT_DIR="${2:-}"; shift 2 ;;
    --out=*) OUT_DIR="${1#--out=}"; shift ;;
    --verify-set) ACTION="verify-set"; shift ;;
    --verbose) VERBOSE=1; shift ;;
    -h|--help) sed -n '1,30p' "$0"; echo "usage: build-package.sh --os O --arch A --version V --bin PATH [--out DIR]"; echo "       build-package.sh --verify-set --version V --out DIR"; exit 0 ;;
    *) echo "build-package.sh: unknown flag $1" >&2; exit 2 ;;
  esac
done

# --- helpers -----------------------------------------------------------

fail() { echo "build-package.sh: $1" >&2; exit 1; }
info() { if [ "$VERBOSE" -eq 1 ]; then echo "build-package.sh: $1"; fi; }

# Validate OS/arch against the shipped set; normalize rust spellings.
normalize() {
  case "$1" in
    aarch64) echo "arm64";;
    arm64) echo "arm64";;
    x86_64) echo "x64";;
    x64) echo "x64";;
    *) echo "$1";;
  esac
}

# Derive binary/installer names (Windows .exe handling).
binary_for() {
  if [ "$1" = "windows" ]; then echo "framesmith.exe"; else echo "framesmith"; fi
}
installer_for() {
  if [ "$1" = "windows" ]; then echo "install.ps1"; else echo "install.sh"; fi
}
slug_for() { printf '%s-%s' "$1" "$2"; }
archive_ext_for() { if [ "$1" = "windows" ]; then echo "zip"; else echo "tar.gz"; fi; }
asset_name() {
  # asset_name VERSION OS ARCH
  printf 'framesmith-%s-%s.%s' "$1" "$(slug_for "$2" "$3")" "$(archive_ext_for "$2")"
}

is_release_version() {
  # Bare version: N.N.N with optional -prerelease. Leading v already stripped.
  # Use a regex: ^[0-9]+\.[0-9]+\.[0-9]+(-.*)?$
  v="$1"
  case "$v" in
    ""|"v") return 1 ;;
  esac
  # One regex covers the whole thing; grep -Eq is portable.
  if printf '%s' "$v" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+(-[A-Za-z0-9._-]+)?$'; then
    return 0
  fi
  return 1
}

supported_line() {
  printf 'supported platforms: macos/arm64; macos/x64; windows/x64; linux/x64'
}

# Repo roots (this script lives in framesmith/tools/).
TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
DIST_TEMPLATES="$FRAMESMITH_DIR/dist"

# --- tagged-release gate ----------------------------------------------
# Packages build only from tagged releases. When --version is given
# explicitly (CI passes the tag version explicitly), we still require that
# HEAD is tagged with a release tag matching that version OR that the
# caller proves a tagged release via --tag. When --version is omitted we
# derive it from the tag. An untagged HEAD with an explicit version still
# needs the --allow-untagged-test escape hatch? No: explicit version WITHOUT
# a tag is refused unless GIT_COMMIT is on a tag. For local verification
# the test creates a lightweight tag; CI tags before this script runs.

resolve_version() {
  ver="$1"
  if [ -n "$ver" ]; then
    # Strip leading v.
    case "$ver" in v*) ver="${ver#v}";; esac
    if ! is_release_version "$ver"; then
      fail "version '$ver' is not a release version (expected N.N.N, e.g. 0.1.0)"
    fi
    # Require HEAD to be tagged with v<ver> or <ver>.
    tag="$(git -C "$FRAMESMITH_DIR" describe --tags --exact-match 2>/dev/null || true)"
    if [ -z "$tag" ]; then
      fail "packages build only from tagged releases; HEAD is not tagged (pass a git tag v$ver and rebuild). Use 'git tag v$ver' or run from a tagged release."
    fi
    # HEAD must be on the matching tag (exact match).
    tag_ver="$(printf '%s' "$tag" | sed 's/^v//')"
    if [ "$tag_ver" != "$ver" ]; then
      fail "HEAD is tagged $tag but --version is $ver; HEAD must be tagged v$ver for a $ver package."
    fi
    printf '%s' "$ver"
    return
  fi
  # No explicit version: derive from tag on HEAD.
  tag="$(git -C "$FRAMESMITH_DIR" describe --tags --exact-match 2>/dev/null || true)"
  if [ -z "$tag" ]; then
    fail "packages build only from tagged releases; HEAD is not tagged. Tag the release (e.g. 'git tag v0.1.0') or pass --version with a tagged HEAD."
  fi
  ver="$(printf '%s' "$tag" | sed 's/^v//')"
  if ! is_release_version "$ver"; then
    fail "tag '$tag' is not a release tag (expected vN.N.N)"
  fi
  printf '%s' "$ver"
}

# --- SHA-256 of a file -------------------------------------------------
sha256_file() {
  file="$1"
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$file" | awk '{print $1}'
  elif command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    fail "no shasum/sha256sum found; cannot checksum packages"
  fi
}

# --- actions ----------------------------------------------------------

if [ "$ACTION" = "verify-set" ]; then
  VERSION="$(resolve_version "$VERSION")"
  if [ -z "$OUT_DIR" ]; then fail "--verify-set needs --out DIR"; fi

  missing=""
  for pair in "macos:arm64" "macos:x64" "windows:x64" "linux:x64"; do
    los="$(printf '%s' "$pair" | cut -d: -f1)"
    larch="$(printf '%s' "$pair" | cut -d: -f2)"
    asset="$(asset_name "$VERSION" "$los" "$larch")"
    if [ ! -f "$OUT_DIR/$asset" ]; then
      slug="$(slug_for "$los" "$larch")"
      if [ -z "$missing" ]; then missing="$slug"; else missing="$missing, $slug"; fi
    fi
  done

  if [ -n "$missing" ]; then
    fail "incomplete OS set for release $VERSION: missing $missing"
  fi
  echo "build-package.sh: complete OS set for $VERSION (macos-arm64, macos-x64, windows-x64, linux-x64)"
  exit 0
fi

# build: one package.
if [ -z "$OS" ] || [ -z "$ARCH" ]; then
  fail "--os and --arch are required for build (got os='$OS' arch='$ARCH')"
fi
ARCH="$(normalize "$ARCH")"

# Unsupported combinations refuse naming the supported set.
case "$OS/$ARCH" in
  macos/arm64|macos/x64|windows/x64|linux/x64) ;;
  *)
    fail "unsupported platform $OS/$ARCH; $(supported_line)"
    ;;
esac

VERSION="$(resolve_version "$VERSION")"

if [ -z "$BIN" ]; then
  # Default to cargo debug/release output for the host.
  BIN="$FRAMESMITH_DIR/target/debug/framesmith"
  # On Windows the debug binary is framesmith.exe; Unix is framesmith.
  if [ "$OS" = "windows" ] && [ ! -f "$BIN" ] && [ -f "$FRAMESMITH_DIR/target/debug/framesmith.exe" ]; then
    BIN="$FRAMESMITH_DIR/target/debug/framesmith.exe"
  fi
fi
if [ ! -f "$BIN" ]; then
  fail "binary not found: $BIN (build it first: cargo build --bin framesmith)"
fi

# Default out dir is under target/ so writes are .gitignore-safe.
if [ -z "$OUT_DIR" ]; then
  OUT_DIR="$FRAMESMITH_DIR/target/dist"
fi
mkdir -p "$OUT_DIR"

SLUG="$(slug_for "$OS" "$ARCH")"
ASSET="$(asset_name "$VERSION" "$OS" "$ARCH")"
BIN_NAME="$(binary_for "$OS")"
INSTALLER="$(installer_for "$OS")"

info "building $ASSET for $OS/$ARCH version $VERSION (bin: $BIN)"

STAGE="$(mktemp -d /tmp/framesmith-dist-XXXXXX)"
trap 'rm -rf "$STAGE"' EXIT INT TERM

# Layout inside the package: binary at root, installer at root,
# notices + license at root, checksums.txt at root (generated).
cp "$BIN" "$STAGE/$BIN_NAME"
chmod 755 "$STAGE/$BIN_NAME" 2>/dev/null || true
if [ "$OS" = "macos" ]; then xattr -cr "$STAGE/$BIN_NAME" 2>/dev/null || true; fi

# Installer template.
if [ ! -f "$DIST_TEMPLATES/$INSTALLER" ]; then
  fail "installer template missing: $DIST_TEMPLATES/$INSTALLER"
fi
cp "$DIST_TEMPLATES/$INSTALLER" "$STAGE/$INSTALLER"
chmod 755 "$STAGE/$INSTALLER" 2>/dev/null || true

# Notices + license (third-party / GPL ffmpeg-registry note travels with every package).
for f in THIRD-PARTY-NOTICES.md LICENSE; do
  if [ -f "$DIST_TEMPLATES/$f" ]; then
    cp "$DIST_TEMPLATES/$f" "$STAGE/$f"
  fi
done

# README: prefer the workspace README when present.
if [ -f "$FRAMESMITH_DIR/README.md" ]; then
  cp "$FRAMESMITH_DIR/README.md" "$STAGE/README.md"
fi

# Generate checksums.txt (release metadata): sha256 of every file in the
# staged package, one "<hex>  <name>" per line, sorted.
# checksums.txt itself is excluded (it cannot check itself); the outer
# checksums file at OUT_DIR level provides tamper evidence for the archive.
( cd "$STAGE" && sh -c 'for f in *; do [ -f "$f" ] || continue; [ "$f" = "checksums.txt" ] && continue; printf "%s\n" "$f"; done' | sort > /tmp/fs-checksums-list.txt )
if [ -s /tmp/fs-checksums-list.txt ]; then
  : > "$STAGE/checksums.txt"
  while IFS= read -r f; do
    [ -n "$f" ] || continue
    hex="$(sha256_file "$STAGE/$f")"
    printf '%s  %s\n' "$hex" "$f" >> "$STAGE/checksums.txt"
  done < /tmp/fs-checksums-list.txt
fi

# Create the archive.
DEST="$OUT_DIR/$ASSET"
if [ "$OS" = "windows" ]; then
  # Zip via python3 stdlib (portable; avoids depending on zip binary).
  python3 - "$STAGE" "$DEST" <<'PY'
import sys, zipfile, pathlib
stage = pathlib.Path(sys.argv[1])
dest = pathlib.Path(sys.argv[2])
dest.parent.mkdir(parents=True, exist_ok=True)
with zipfile.ZipFile(dest, "w", compression=zipfile.ZIP_DEFLATED) as z:
    for p in sorted(stage.iterdir()):
        if p.is_file():
            z.write(p, arcname=p.name)
print(f"wrote {dest}")
PY
else
  # tar.gz via python's tarfile for reproducibility + portability.
  python3 - "$STAGE" "$DEST" <<'PY'
import sys, tarfile, pathlib
stage = pathlib.Path(sys.argv[1])
dest = pathlib.Path(sys.argv[2])
dest.parent.mkdir(parents=True, exist_ok=True)
with tarfile.open(dest, "w:gz") as t:
    for p in sorted(stage.iterdir()):
        if p.is_file():
            t.add(p, arcname=p.name, recursive=False)
print(f"wrote {dest}")
PY
fi

# Outer checksums file at OUT_DIR level: sha256 of every built asset.
# This is the release metadata the installer (and the release workflow)
# verifies the package archive against. Regenerate over the full set.
(
  # OUT_DIR may be relative; capture its absolute form before cd.
  OUT_ABS="$(cd "$OUT_DIR" && pwd)"
  cd "$OUT_ABS" &&
  for f in framesmith-*.tar.gz framesmith-*.zip; do
    [ -f "$f" ] || continue
    hex="$(sha256_file "$OUT_ABS/$f")"
    printf '%s  %s\n' "$hex" "$f"
  done | sort > checksums.txt.tmp 2>/dev/null || true
  if [ -s checksums.txt.tmp ]; then
    mv checksums.txt.tmp checksums.txt
  else
    rm -f checksums.txt.tmp
  fi
)

echo "build-package.sh: built $DEST"
echo "build-package.sh: package contains $BIN_NAME, $INSTALLER, THIRD-PARTY-NOTICES.md, checksums.txt (and LICENSE/README.md when present)"
echo "build-package.sh: checksums written to $OUT_DIR/checksums.txt"

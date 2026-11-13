#!/bin/sh
# n27-release-proof.sh -- Sprint 27 T005 one-command gate.
#
# Proves the release reference end to end locally (CAP-001..CAP-006,
# J-01..J-04, STATE-01..STATE-06, INT-001..INT-003, REC-001..REC-006):
#   1. per-OS packages install + render (T001 / CAP-001, J-01,
#      STATE-01, INT-001, REC-001): all four OS/arch assets assemble
#      from the tagged release, the complete-set check passes, the
#      Windows zip ships framesmith.exe + install.ps1, the host
#      package installs through install.sh with checksum
#      verification and renders immediately, a tampered binary
#      refuses naming the checksum mismatch without installing, an
#      incomplete set refuses naming the missing OSes, and two
#      single-pass renders are byte-identical.
#   2. version + update behavior (T002 / CAP-002, CAP-003, J-02,
#      STATE-02..STATE-04, REC-002, REC-003): `version` / `--json`
#      name version, build, and the 9.0.2 pin; `update check` /
#      `upgrade` report current, update-available, and
#      unreachable-as-unavailable (never current) over file://
#      release fixtures served through the same curl fetch path;
#      apply without --yes refuses naming consent; a checksum
#      mismatch refuses and leaves the running binary working.
#      A successful self-update is NOT run: it would replace the
#      proof binary itself (covered by unit tests and the T002
#      staged manual verification instead).
#   3. parity verdict (T003 / CAP-004, J-03, INT-002, REC-004):
#      the checker re-runs live and reads 100% parity counting
#      exceptions against hyperframes@0.9.6 (75 present + 40 named
#      exceptions = 115 rows), naming the release version.
#   4. sweep outcomes (T004 / CAP-005, CAP-006, J-04, J-05,
#      STATE-05, STATE-06, REC-005, REC-006): the sweep enumerates
#      all 49 references READY, then measures a fixed subset with
#      wall-clock medians and quality verdicts. Every reference it
#      cannot render on framesmith is a named REC-006 miss with
#      both deltas; unexecuted references keep the gate INCOMPLETE.
#      The gate refuses sign-off while any miss is open; a full
#      49-of-49 measured run is reported as a gap, never claimed.
#   5. CI file shape: the three legs (macOS, Windows, Linux) with
#      fail-fast false and identical cargo commands; YAML parses.
#      A live green run needs the owner push and is reported as
#      PENDING, never claimed.
#   6. targeted cargo tests for the touched crates (dist, ffmpeg,
#      cli n27 suite). The full --workspace suite is NOT run here.
# Reports PROOF: PASS only when every check succeeds (exit 0); any
# single failing check prints FAIL and the gate exits 1.
#
# Usage: n27-release-proof.sh [--workdir DIR] [--bin PATH]
#   Renders and logs go under DIR (default: fresh mktemp directory).
#   The checkout itself is never rendered into; the project is staged
#   to DIR first. Evidence is copied to evidences/proof-n27-release/
#   on every run. The committed parity verdict and sweep report in
#   tools/ are never overwritten: fresh proof output lands in the
#   workdir and is copied under evidences/.
#
# Durable rules this gate relies on: KDL style attributes are
# CSS-hyphenated (border-width in the staged fixture); the glyph
# and layout crates are untouched by this sprint; single-pass
# renders stay byte-identical (checked here); ffmpeg resolves from
# the ERR own registry latest (version pin 9.0.2, checked in gate 2;
# the local gate uses PATH ffmpeg for probing, the installed
# binary bootstraps its pinned build at render time); the script
# runs on POSIX runners -- Windows path joins and ffmpeg.exe
# handling are covered by the CI matrix legs (gate 5) and the
# Windows zip shape check (gate 1), not by this shell.
#
# Tag handling: build-package.sh refuses without a matching release
# tag on HEAD (REC-001). When HEAD is untagged the gate creates a
# lightweight tag v<VERSION> at HEAD, builds, and deletes it again
# on exit (only if it still points at the commit it was created
# on). A HEAD already tagged differently fails the gate honestly.

set -u

WORKDIR=""
BIN_OVERRIDE=""
while [ "$#" -gt 0 ]; do
  case "$1" in
    --workdir) WORKDIR="${2:-}"; shift 2 ;;
    --bin) BIN_OVERRIDE="${2:-}"; shift 2 ;;
    -h|--help) sed -n '1,54p' "$0"; exit 0 ;;
    *) echo "n27-release-proof.sh: unknown flag $1" >&2; exit 1 ;;
  esac
done

TOOLS_DIR="$(cd "$(dirname "$0")" && pwd)"
FRAMESMITH_DIR="$(cd "$TOOLS_DIR/.." && pwd)"
WS_ROOT="$(cd "$FRAMESMITH_DIR/.." && pwd)"
CI_FILE="$FRAMESMITH_DIR/.github/workflows/ci.yml"
REL_FILE="$FRAMESMITH_DIR/.github/workflows/release.yml"
BUILD_PKG="$FRAMESMITH_DIR/tools/build-package.sh"
PARITY_CHECK="$FRAMESMITH_DIR/tools/n27-parity-check.sh"
SWEEP="$FRAMESMITH_DIR/tools/n27-sweep.sh"
DIST_DIR="$FRAMESMITH_DIR/dist"
EVID_DIR="$WS_ROOT/evidences/proof-n27-release"
INSP_DEFAULT="/Users/budisantoso/Dev/bdsanxz/BeEditorWorkspace/skills/video/direcfy/inspirations"

if [ -z "$WORKDIR" ]; then
  WORKDIR="$(mktemp -d /tmp/framesmith-n27-proof-XXXXXX)"
fi
mkdir -p "$WORKDIR" || exit 1

STAMP="$(date -u '+%Y-%m-%dT%H:%M:%SZ')"

PASS_N=0
FAIL_LIST=""

pass() { PASS_N=$((PASS_N + 1)); echo "PASS: $1"; }
fail() { FAIL_LIST="$FAIL_LIST $1"; echo "FAIL: $1 -- $2"; }

# --- tag cleanup (gate 1 may create a temporary release tag) -------------
CREATED_TAG=""
TAG_HEAD=""
cleanup_tag() {
  if [ -n "$CREATED_TAG" ] && [ -n "$TAG_HEAD" ]; then
    cur="$(git -C "$FRAMESMITH_DIR" rev-parse --verify "$CREATED_TAG" 2>/dev/null || true)"
    if [ "$cur" = "$TAG_HEAD" ]; then
      git -C "$FRAMESMITH_DIR" tag -d "$CREATED_TAG" >/dev/null 2>&1 || true
      echo "cleanup: removed temporary tag $CREATED_TAG"
    fi
  fi
}
trap cleanup_tag EXIT

echo "================ n27 release proof gate (T005) ==============="
echo "date(UTC): $STAMP"
echo "machine: $(uname -srm)"
echo "workdir: $WORKDIR"
echo "=================================================================="
echo ""

for NEED in cargo ffmpeg ffprobe python3 ruby curl npx node; do
  if ! command -v "$NEED" >/dev/null 2>&1; then
    fail "prereq-$NEED" "required tool '$NEED' not found on PATH"
  fi
done
for F in "$CI_FILE" "$REL_FILE" "$BUILD_PKG" "$PARITY_CHECK" "$SWEEP" \
  "$DIST_DIR/install.sh" "$DIST_DIR/install.ps1" \
  "$DIST_DIR/THIRD-PARTY-NOTICES.md" "$DIST_DIR/LICENSE" \
  "$FRAMESMITH_DIR/tools/n27-parity-inventory.json"; do
  if [ ! -f "$F" ]; then
    fail "prereq-file" "missing $F"
  fi
done
if [ ! -d "$INSP_DEFAULT" ]; then
  fail "prereq-inspirations" "missing $INSP_DEFAULT"
fi
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- missing prerequisites:$FAIL_LIST"
  exit 1
fi

# --- version + host slug ---------------------------------------------------
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$FRAMESMITH_DIR/crates/cli/Cargo.toml" | head -n 1)"
if [ -z "$VERSION" ]; then
  fail "prereq-version" "could not derive version from crates/cli/Cargo.toml"
fi

OS_U="$(uname -s)"; ARCH_U="$(uname -m)"
case "$OS_U" in
  Darwin) HOST_OS="macos" ;;
  Linux) HOST_OS="linux" ;;
  *) fail "host-os" "unsupported platform $OS_U (supported: macos-arm64, macos-x64, windows-x64, linux-x64)"; HOST_OS="" ;;
esac
case "$ARCH_U" in
  arm64|aarch64) HOST_ARCH="arm64" ;;
  x86_64) HOST_ARCH="x64" ;;
  *) fail "host-arch" "unsupported arch $ARCH_U (supported: macos-arm64, macos-x64, windows-x64, linux-x64)"; HOST_ARCH="" ;;
esac
if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- unsupported host platform$FAIL_LIST"
  exit 1
fi
HOST_SLUG="$HOST_OS-$HOST_ARCH"
HOST_EXT="tar.gz"
echo "release version: $VERSION"
echo "host package: $HOST_SLUG"

# sha256_of FILE — hex digest.
sha256_of() {
  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
  else
    sha256sum "$1" | awk '{print $1}'
  fi
}

# ---- build ---------------------------------------------------------------
if [ -n "$BIN_OVERRIDE" ]; then
  FSMITH="$BIN_OVERRIDE"
else
  FSMITH="$FRAMESMITH_DIR/target/debug/framesmith"
  echo "--- build framesmith binary ---"
  if (cd "$FRAMESMITH_DIR" && cargo build --bin framesmith >"$WORKDIR/build.log" 2>&1); then
    echo "build ok"
  else
    fail "build" "cargo build failed; see $WORKDIR/build.log"
    echo ""
    echo "PROOF: FAIL -- failing items:$FAIL_LIST"
    exit 1
  fi
fi
VER_LINE="$( "$FSMITH" --version 2>/dev/null || echo present )"
echo "framesmith: $VER_LINE"
echo "cargo: $(cargo --version 2>/dev/null)"
echo "rustc: $(rustc --version 2>/dev/null)"
echo "python: $(python3 --version 2>&1)"
echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
echo ""

# ---- stage a tiny project (the checkout is never rendered into) ----------
# Compact fixture for fast package/version render checks. Style
# attributes are CSS-hyphenated (border-width, border-color).
echo "--- stage: 2s project (60 frames) ---"
mkdir -p "$WORKDIR/proj" || exit 1
cat >"$WORKDIR/proj/composition.kdl" <<'KDL'
composition name="t" width=320 height=180 duration=2 fps=30 background="#112233" version=1 {
    box x=10 y=10 width=120 height=60 fill="#ff0000" radius=4 border-width=2 border-color="#ffffff" opacity=1 order=1 start=0 duration=2
    text value="Hi" start=0 duration=2 x=140 y=40 size=20 color="#ffffff"
}
KDL
if [ -f "$WORKDIR/proj/composition.kdl" ]; then
  pass "staged-project (2s 60-frame composition, CSS-hyphenated attrs)"
else
  fail "stage-project" "staged composition.kdl missing"
fi

# ---- gate 1: per-OS packages install + render (T001) ----------------------
echo ""
echo "--- gate 1: per-OS packages install + render ---"
for F in "$DIST_DIR/install.sh" "$DIST_DIR/install.ps1" \
  "$DIST_DIR/THIRD-PARTY-NOTICES.md" "$DIST_DIR/LICENSE" "$BUILD_PKG"; do
  if [ -f "$F" ]; then pass "packaging-file $(basename "$F")"; else fail "packaging-file" "missing $F"; fi
done
if grep -q "ffmpeg-registry" "$DIST_DIR/THIRD-PARTY-NOTICES.md" \
  && grep -q "GPL" "$DIST_DIR/THIRD-PARTY-NOTICES.md"; then
  pass "third-party-notices-name-gpl-registry"
else
  fail "third-party-notices" "GPL ffmpeg-registry note absent; see THIRD-PARTY-NOTICES.md"
fi
# Tag gate (REC-001): packages build only from a matching release tag.
EXISTING_TAG="$(git -C "$FRAMESMITH_DIR" describe --tags --exact-match 2>/dev/null || true)"
if [ -n "$EXISTING_TAG" ]; then
  if [ "$EXISTING_TAG" = "v$VERSION" ]; then
    pass "release-tag-present (v$VERSION on HEAD)"
  else
    fail "release-tag" "HEAD is tagged $EXISTING_TAG, want v$VERSION for this gate"
  fi
else
  TAG_HEAD="$(git -C "$FRAMESMITH_DIR" rev-parse HEAD 2>/dev/null || true)"
  if [ -n "$TAG_HEAD" ] && (cd "$FRAMESMITH_DIR" && git tag "v$VERSION" >"$WORKDIR/tag.log" 2>&1); then
    CREATED_TAG="v$VERSION"
    pass "release-tag-created-tmp (v$VERSION at HEAD, removed on exit)"
  else
    fail "release-tag" "HEAD untagged and could not create v$VERSION; see tag.log"
  fi
fi
# Assemble all four per-OS assets from the release tag.
DIST_OUT="$WORKDIR/dist"
mkdir -p "$DIST_OUT" || exit 1
ASSET_OK=1
for pair in "macos arm64" "macos x64" "windows x64" "linux x64"; do
  _os="$(printf '%s' "$pair" | cut -d' ' -f1)"
  _arch="$(printf '%s' "$pair" | cut -d' ' -f2)"
  if [ "$_os" = "windows" ]; then _ext="zip"; else _ext="tar.gz"; fi
  _asset="framesmith-$VERSION-$_os-$_arch.$_ext"
  if "$BUILD_PKG" --os "$_os" --arch "$_arch" --version "$VERSION" --bin "$FSMITH" --out "$DIST_OUT" >"$WORKDIR/build-$_os-$_arch.log" 2>&1 \
    && [ -f "$DIST_OUT/$_asset" ]; then
    pass "package-built ($_asset)"
  else
    fail "package-built-$_os-$_arch" "build failed for $_asset; see build-$_os-$_arch.log"
    ASSET_OK=0
  fi
done
# Complete-set check (REC-001 happy path).
if "$BUILD_PKG" --verify-set --version "$VERSION" --out "$DIST_OUT" >"$WORKDIR/verify-set.log" 2>&1 \
  && grep -q "complete OS set" "$WORKDIR/verify-set.log"; then
  pass "complete-os-set (macos-arm64, macos-x64, windows-x64, linux-x64)"
else
  fail "verify-set" "complete-set check failed; see verify-set.log"
fi
# Incomplete set refuses naming the missing OSes (REC-001).
mkdir -p "$WORKDIR/dist-incomplete" || exit 1
cp "$DIST_OUT/framesmith-$VERSION-$HOST_SLUG.$HOST_EXT" "$WORKDIR/dist-incomplete/" 2>/dev/null || true
if "$BUILD_PKG" --verify-set --version "$VERSION" --out "$WORKDIR/dist-incomplete" >"$WORKDIR/verify-incomplete.log" 2>&1; then
  fail "incomplete-nonzero" "verify-set exited 0 on a 1-asset set; see verify-incomplete.log"
elif grep -q "missing" "$WORKDIR/verify-incomplete.log" \
  && grep -q "windows-x64" "$WORKDIR/verify-incomplete.log"; then
  pass "incomplete-set-refuses-naming-missing ($(grep -o 'missing.*' "$WORKDIR/verify-incomplete.log"))"
else
  fail "incomplete-text" "refusal names no missing OS; see verify-incomplete.log"
fi
# Windows zip shape: framesmith.exe + install.ps1 ship together.
WIN_ZIP="$DIST_OUT/framesmith-$VERSION-windows-x64.zip"
if python3 - "$WIN_ZIP" >"$WORKDIR/win-zip.log" 2>&1 <<'PYEOF'
import sys, zipfile
names = zipfile.ZipFile(sys.argv[1]).namelist()
assert "framesmith.exe" in names, names
assert "install.ps1" in names, names
assert "checksums.txt" in names, names
print("zip ok: " + ", ".join(sorted(names)))
PYEOF
then
  pass "windows-zip-ships-exe-and-installer ($(cat "$WORKDIR/win-zip.log"))"
else
  fail "windows-zip" "windows zip misses framesmith.exe/install.ps1; see win-zip.log"
fi
# Install the host package into a clean PREFIX (checksum verified).
HOST_PKG="$DIST_OUT/framesmith-$VERSION-$HOST_SLUG.$HOST_EXT"
PKG_DIR="$WORKDIR/pkg"
if python3 - "$HOST_PKG" "$PKG_DIR" >"$WORKDIR/unpack.log" 2>&1 <<'PYEOF'
import sys, tarfile, pathlib
tarfile.open(sys.argv[1]).extractall(sys.argv[2])
for p in sorted(pathlib.Path(sys.argv[2]).iterdir()):
    print(p.name)
PYEOF
then
  pass "host-package-unpacks ($(cat "$WORKDIR/unpack.log" | tr '\n' ' '))"
else
  fail "unpack" "host package failed to extract; see unpack.log"
fi
if [ -f "$PKG_DIR/install.sh" ] && [ -f "$PKG_DIR/framesmith" ] && [ -f "$PKG_DIR/checksums.txt" ]; then
  pass "host-package-layout (binary + install.sh + checksums.txt)"
else
  fail "host-package-layout" "extracted package misses binary/install.sh/checksums.txt; see unpack.log"
fi
PREFIX_DIR="$WORKDIR/prefix"
if "$PKG_DIR/install.sh" --prefix "$PREFIX_DIR" >"$WORKDIR/install.log" 2>&1; then
  if [ -x "$PREFIX_DIR/bin/framesmith" ] \
    && "$PREFIX_DIR/bin/framesmith" --version >"$WORKDIR/installed-version.log" 2>&1 \
    && grep -q "framesmith $VERSION" "$WORKDIR/installed-version.log"; then
    pass "install-verified-and-runs (prefix $PREFIX_DIR, $(cat "$WORKDIR/installed-version.log"))"
  else
    fail "installed-version" "installed binary missing or wrong version; see install.log"
  fi
else
  fail "install" "install.sh refused the good package; see install.log"
fi
if [ -f "$PREFIX_DIR/share/framesmith/THIRD-PARTY-NOTICES.md" ] \
  && [ -f "$PREFIX_DIR/share/framesmith/LICENSE" ]; then
  pass "install-carries-notices (notices + license under share/) "
else
  fail "install-notices" "notices/license not installed under share/; see install.log"
fi
# Tampered binary refuses, installs nothing, previous install untouched.
TAMPER_DIR="$WORKDIR/tampered"
rm -rf "$TAMPER_DIR"; mkdir -p "$TAMPER_DIR" || exit 1
if cp "$PKG_DIR/framesmith" "$PKG_DIR/install.sh" "$PKG_DIR/checksums.txt" "$TAMPER_DIR/" \
  && printf 'x' >> "$TAMPER_DIR/framesmith"; then
  if "$TAMPER_DIR/install.sh" --prefix "$WORKDIR/prefix-tampered" >"$WORKDIR/install-tampered.log" 2>&1; then
    fail "tampered-nonzero" "install.sh exited 0 on a tampered binary; see install-tampered.log"
  elif grep -q "checksum mismatch" "$WORKDIR/install-tampered.log" && [ ! -x "$WORKDIR/prefix-tampered/bin/framesmith" ] \
    && "$PREFIX_DIR/bin/framesmith" --version >/dev/null 2>&1; then
    pass "tampered-install-refuses ($(grep -m1 'checksum mismatch' "$WORKDIR/install-tampered.log" | cut -c1-120))"
  else
    fail "tampered-text" "refusal names no mismatch, installed something, or broke the good install; see install-tampered.log"
  fi
else
  fail "tamper-stage" "could not stage the tampered package"
fi
# The installed binary renders immediately (no manual provisioning).
if (cd "$WORKDIR" && "$PREFIX_DIR/bin/framesmith" render proj -o install-render.mp4 --backend cpu >"$WORKDIR/render-install.log" 2>&1) \
  && [ -f "$WORKDIR/install-render.mp4" ]; then
  if ffprobe -v error -show_entries stream=codec_name -of default=noprint_wrappers=1 "$WORKDIR/install-render.mp4" 2>/dev/null | grep -q "h264"; then
    pass "installed-render-works (h264 MP4 from clean prefix)"
  else
    fail "render-codec" "rendered MP4 is not h264; see render-install.log"
  fi
else
  fail "render-install" "installed render exited non-zero; see render-install.log"
fi
# Single-pass byte-identical: two renders, same bytes.
if (cd "$WORKDIR" && "$PREFIX_DIR/bin/framesmith" render proj -o install-render2.mp4 --backend cpu >"$WORKDIR/render-install2.log" 2>&1) \
  && [ -f "$WORKDIR/install-render2.mp4" ]; then
  H1="$(sha256_of "$WORKDIR/install-render.mp4")"
  H2="$(sha256_of "$WORKDIR/install-render2.mp4")"
  if [ "$H1" = "$H2" ]; then
    pass "single-pass-byte-identical (sha256 ${H1})"
  else
    fail "byte-identical" "two single-pass renders differ: ${H1} vs ${H2}"
  fi
else
  fail "render-install2" "second single-pass render failed; see render-install2.log"
fi

# ---- gate 2: version report + update behavior (T002) ----------------------
echo ""
echo "--- gate 2: version report + update behavior ---"
if "$FSMITH" --version >"$WORKDIR/dash-version.log" 2>&1 \
  && python3 - "$WORKDIR/dash-version.log" "$VERSION" >"$WORKDIR/version-oneline.log" 2>&1 <<'PYEOF'
import sys
line = open(sys.argv[1]).read().strip()
assert line == "framesmith " + sys.argv[2], repr(line)
print("oneline ok: " + line)
PYEOF
then
  pass "dash-version-single-line-unchanged (framesmith $VERSION)"
else
  fail "dash-version" "--version is not the single unchanged line; see version-oneline.log"
fi
if "$FSMITH" version >"$WORKDIR/version.log" 2>&1; then
  _l1="$(sed -n '1p' "$WORKDIR/version.log")"
  _l2="$(sed -n '2p' "$WORKDIR/version.log")"
  _l3="$(sed -n '3p' "$WORKDIR/version.log")"
  if [ "$_l1" = "framesmith $VERSION" ] \
    && printf '%s' "$_l2" | grep -q "^build: " \
    && printf '%s' "$_l3" | grep -q "9.0.2" \
    && printf '%s' "$_l3" | grep -q "ffmpeg-registry"; then
    pass "version-reports-version-build-pin ($_l1 | $_l2 | $_l3)"
  else
    fail "version-lines" "version output misses version/build/pin; see version.log"
  fi
else
  fail "version" "framesmith version exited non-zero; see version.log"
fi
if "$FSMITH" version --json >"$WORKDIR/version-json.log" 2>&1 \
  && python3 - "$WORKDIR/version-json.log" "$VERSION" "$HOST_OS" "$HOST_ARCH" >"$WORKDIR/version-json-check.log" 2>&1 <<'PYEOF'
import json, sys
v = json.load(open(sys.argv[1]))
assert v["tool"] == "framesmith", v
assert v["version"] == sys.argv[2], v
assert v["ffmpeg_registry_pin"] == "9.0.2", v
assert v["os"] == sys.argv[3], v
want_arch = {"arm64": "aarch64", "x64": "x86_64"}[sys.argv[4]]
assert v["arch"] == want_arch, v
assert v["unknown_build"] == ("unknown build" in v["build"]), v
print("json ok: version %s pin 9.0.2 os %s build %s" % (v["version"], v["os"], v["build"]))
PYEOF
then
  pass "version-json-carries-build-and-pin"
else
  fail "version-json" "version --json misses tool/version/pin; see version-json-check.log"
fi
# Release-metadata fixtures through the same curl fetch path as production.
write_release() {
  # write_release DIR TAG HEX BYTELEN — asset + checksums.txt + release.json
  python3 - "$1" "$2" "$3" "$4" "$HOST_SLUG" "$HOST_EXT" <<'PYEOF'
import json, pathlib, sys
d = pathlib.Path(sys.argv[1]); d.mkdir(parents=True, exist_ok=True)
tag, hexv, n = sys.argv[2], sys.argv[3], int(sys.argv[4])
slug, ext = sys.argv[5], sys.argv[6]
ver = tag[1:] if tag.startswith("v") else tag
asset = "framesmith-%s-%s.%s" % (ver, slug, ext)
(d / asset).write_bytes(b"x" * n)
(d / "checksums.txt").write_text("%s  %s\n" % (hexv, asset))
bdu = lambda p: "file://%s" % p
body = {"tag_name": tag,
        "html_url": "https://example.invalid/releases/%s" % tag,
        "assets": [{"name": asset, "browser_download_url": bdu(d / asset)},
                   {"name": "checksums.txt", "browser_download_url": bdu(d / "checksums.txt")}]}
(d / "release.json").write_text(json.dumps(body))
print(str(d / "release.json"))
PYEOF
}
# current: metadata names the running version -> current, no banner.
META_DIR="$WORKDIR/upd-current"; mkdir -p "$META_DIR" || exit 1
META_URL="$(write_release "$META_DIR" "v$VERSION" 00 4)"
if "$FSMITH" update check --api-url "file://$META_URL" >"$WORKDIR/upd-current.log" 2>&1; then
  if grep -q "current" "$WORKDIR/upd-current.log" && ! grep -q "update-available" "$WORKDIR/upd-current.log"; then
    pass "update-check-current-no-banner ($(grep -m1 current "$WORKDIR/upd-current.log" | cut -c1-100))"
  else
    fail "update-current-text" "current report wrong; see upd-current.log"
  fi
else
  fail "update-current" "current check exited non-zero; see upd-current.log"
fi
# newer: v99.0.0 names the release with where to get it.
META_DIR="$WORKDIR/upd-newer"; mkdir -p "$META_DIR" || exit 1
META_URL="$(write_release "$META_DIR" "v99.0.0" 00 4)"
UPD_URL="file://$META_URL"
if "$FSMITH" update check --api-url "$UPD_URL" >"$WORKDIR/upd-newer.log" 2>&1; then
  if grep -q "update-available" "$WORKDIR/upd-newer.log" \
    && grep -q "99.0.0" "$WORKDIR/upd-newer.log" \
    && grep -q "example.invalid/releases/v99.0.0" "$WORKDIR/upd-newer.log"; then
    pass "update-check-names-newer-with-path"
  else
    fail "update-newer-text" "newer report misses name/path; see upd-newer.log"
  fi
else
  fail "update-newer" "newer check exited non-zero; see upd-newer.log"
fi
if "$FSMITH" update check --json --api-url "$UPD_URL" >"$WORKDIR/upd-newer-json.log" 2>&1 \
  && python3 - "$WORKDIR/upd-newer-json.log" "$VERSION" >"$WORKDIR/upd-newer-json-check.log" 2>&1 <<'PYEOF'
import json, sys
v = json.load(open(sys.argv[1]))
assert v["status"] == "update-available", v
assert v["latest"] == "99.0.0", v
assert v["current"] == sys.argv[2], v
assert "framesmith-99.0.0-" in v["asset"], v
print("newer json ok: status update-available latest 99.0.0")
PYEOF
then
  pass "update-check-json-agrees"
else
  fail "update-newer-json" "update check --json disagrees; see upd-newer-json-check.log"
fi
if "$FSMITH" upgrade --api-url "$UPD_URL" >"$WORKDIR/upgrade.log" 2>&1 \
  && grep -q "update-available" "$WORKDIR/upgrade.log"; then
  pass "upgrade-spelling-checks (no install without --yes)"
else
  fail "upgrade-check" "bare upgrade did not report update-available; see upgrade.log"
fi
# unreachable: unavailable (never current), exit 1 (REC-003).
if "$FSMITH" update check --api-url "file:///nonexistent-n27-probe" >"$WORKDIR/upd-unreach.log" 2>&1; then
  fail "unreachable-nonzero" "unreachable check exited 0; see upd-unreach.log"
elif grep -q "unavailable" "$WORKDIR/upd-unreach.log" && ! grep -q " current (" "$WORKDIR/upd-unreach.log"; then
  pass "unreachable-reports-unavailable-never-current"
else
  fail "unreachable-text" "unreachable report wrong; see upd-unreach.log"
fi
if "$FSMITH" update check --json --api-url "file:///nonexistent-n27-probe" >"$WORKDIR/upd-unreach-json.log" 2>&1; then
  fail "unreachable-json-nonzero" "unreachable --json exited 0; see upd-unreach-json.log"
elif python3 - "$WORKDIR/upd-unreach-json.log" >"$WORKDIR/upd-unreach-json-check.log" 2>&1 <<'PYEOF'
import json, sys
v = json.load(open(sys.argv[1]))
assert v["status"] == "unavailable", v
assert v["latest"] is None, v
print("unavailable json ok: latest null")
PYEOF
then
  pass "unreachable-json-agrees"
else
  fail "unreachable-json" "unreachable --json wrong; see upd-unreach-json-check.log"
fi
# Consent: apply without --yes refuses naming the flag; binary untouched.
if "$FSMITH" update apply --api-url "$UPD_URL" >"$WORKDIR/upd-consent.log" 2>&1; then
  fail "consent-nonzero" "apply without --yes exited 0; see upd-consent.log"
elif grep -q -- "--yes" "$WORKDIR/upd-consent.log" \
  && "$FSMITH" --version >"$WORKDIR/post-consent-version.log" 2>&1 \
  && grep -q "framesmith $VERSION" "$WORKDIR/post-consent-version.log"; then
  pass "apply-without-consent-refuses (binary still framesmith $VERSION)"
else
  fail "consent-text" "refusal names no --yes or binary broke; see upd-consent.log"
fi
# Mismatch apply refuses naming it; running binary untouched (REC-002).
META_DIR="$WORKDIR/upd-mismatch"; mkdir -p "$META_DIR" || exit 1
META_URL="$(write_release "$META_DIR" "v99.0.0" 0000000000000000000000000000000000000000000000000000000000000000 13)"
if "$FSMITH" update apply --yes --api-url "file://$META_URL" >"$WORKDIR/upd-mismatch.log" 2>&1; then
  fail "mismatch-nonzero" "mismatch apply exited 0; see upd-mismatch.log"
elif grep -q "checksum mismatch" "$WORKDIR/upd-mismatch.log" \
  && "$FSMITH" --version >"$WORKDIR/post-mismatch-version.log" 2>&1 \
  && grep -q "framesmith $VERSION" "$WORKDIR/post-mismatch-version.log"; then
  pass "mismatch-apply-refuses-binary-working"
else
  fail "mismatch-text" "refusal names no mismatch or binary broke; see upd-mismatch.log"
fi
pass "no-successful-self-update-in-proof (apply-success covered by unit tests + T002 staged run, not executed against the proof binary)"

# ---- gate 3: parity verdict 100% with named exceptions (T003) -------------
echo ""
echo "--- gate 3: parity verdict (live checker re-run) ---"
if "$PARITY_CHECK" --bin "$FSMITH" --out "$WORKDIR/parity-verdict.md" >"$WORKDIR/parity-check.log" 2>&1; then
  pass "parity-checker-exit-0"
else
  fail "parity-checker" "parity checker exited non-zero; see parity-check.log"
fi
if [ -f "$WORKDIR/parity-verdict.md" ] \
  && grep -q "VERDICT: 100% parity counting exceptions" "$WORKDIR/parity-verdict.md" \
  && grep -q "hyperframes@0.9.6" "$WORKDIR/parity-verdict.md" \
  && grep -q "framesmith $VERSION" "$WORKDIR/parity-verdict.md"; then
  pass "parity-verdict-100-vs-0.9.6 ($(grep -m1 VERDICT "$WORKDIR/parity-verdict.md"))"
else
  fail "parity-verdict" "fresh verdict misses 100% / hyperframes@0.9.6 / framesmith $VERSION; see parity-check.log"
fi
if python3 - "$FRAMESMITH_DIR/tools/n27-parity-inventory.json" >"$WORKDIR/parity-inventory-check.log" 2>&1 <<'PYEOF'
import json, sys
d = json.load(open(sys.argv[1]))
rows = d["rows"]
present = sum(1 for r in rows if r["status"] == "present")
exc = [r for r in rows if r["status"] == "exception"]
assert len(rows) == 115, len(rows)
assert present == 75, present
assert len(exc) == 40, len(exc)
assert all(r.get("justification") and len(r["justification"]) >= 30 for r in exc), "weak justification"
assert d["meta"]["release"]["package"] == "hyperframes@0.9.6", d["meta"]["release"]
assert d["meta"]["framesmith"]["version"] == "0.1.0", d["meta"]["framesmith"]
print("inventory ok: rows=%d present=%d exceptions=%d all justified" % (len(rows), present, len(exc)))
PYEOF
then
  pass "parity-inventory-115-rows ($(cat "$WORKDIR/parity-inventory-check.log"))"
else
  fail "parity-inventory" "inventory counts wrong; see parity-inventory-check.log"
fi
COMMITTED_VERDICT="$FRAMESMITH_DIR/tools/n27-parity-verdict.md"
if [ -f "$COMMITTED_VERDICT" ] \
  && grep -q "VERDICT: 100% parity counting exceptions" "$COMMITTED_VERDICT"; then
  pass "committed-verdict-agrees-100"
else
  fail "committed-verdict" "tools/n27-parity-verdict.md misses the 100% verdict"
fi

# ---- gate 4: sweep outcomes — 49 lines or named misses (T004) --------------
echo ""
echo "--- gate 4: sweep outcomes (subset measured, full run stays a gap) ---"
SWEEP_WORK="$WORKDIR/sweep"
mkdir -p "$SWEEP_WORK" || exit 1
if "$SWEEP" --check-only --bin "$FSMITH" --workdir "$SWEEP_WORK" >"$WORKDIR/sweep-check.log" 2>&1; then
  N_ENUM="$(grep -o 'references ([0-9]*)' "$WORKDIR/sweep-check.log" | head -n 1 | grep -o '[0-9]*' | head -n 1)"
  if [ "$N_ENUM" = "49" ] && grep -q "Verdict: READY" "$WORKDIR/sweep-check.log"; then
    pass "sweep-enumerates-49-ready ($(grep -m1 'framesmith 0.1.0' "$WORKDIR/sweep-check.log" | cut -c1-120))"
  else
    fail "sweep-enumerate" "enumerated=$N_ENUM, want 49 READY; see sweep-check.log"
  fi
else
  fail "sweep-check-only" "sweep --check-only did not report READY; see sweep-check.log"
fi
SWEEP_SUBSET="name-card-template,media-ledger-card,keyframe-arc-sting,render-endpoint"
"$SWEEP" --only "$SWEEP_SUBSET" --reps 2 --bin "$FSMITH" --workdir "$WORKDIR/sweep-run" --out "$WORKDIR/sweep-report.md" >"$WORKDIR/sweep-run.log" 2>&1
SWEEP_EXIT=$?
SWEEP_REPORT="$WORKDIR/sweep-report.md"
if [ -f "$SWEEP_REPORT" ]; then
  pass "sweep-report-written (subset of 4, reps 2)"
else
  fail "sweep-report" "sweep wrote no report; see sweep-run.log"
fi
SWEEP_VERDICT_LINE=""
if [ -f "$SWEEP_REPORT" ]; then
  SWEEP_VERDICT_LINE="$(grep -m1 'executed:' "$SWEEP_REPORT" || true)"
  SWEEP_MISS_LINE="$(grep -m1 'misses (named' "$SWEEP_REPORT" || true)"
  SWEEP_INC_LINE="$(grep -m1 'INCOMPLETE' "$SWEEP_REPORT" || true)"
  echo "sweep subset exit=$SWEEP_EXIT verdict: $SWEEP_VERDICT_LINE | $SWEEP_MISS_LINE | $SWEEP_INC_LINE"
  for REF in name-card-template media-ledger-card keyframe-arc-sting render-endpoint; do
    if grep "^| $REF |" "$SWEEP_REPORT" | grep -q "MISS:"; then
      pass "sweep-line-$REF (time+quality verdict is MISS, named)"
    else
      fail "sweep-line-$REF" "report misses a MISS line for $REF; see sweep-report.md"
    fi
  done
  if grep -q "median" "$SWEEP_REPORT" && grep -q "statistical-only" "$SWEEP_REPORT"; then
    pass "sweep-times-and-quality-present (medians + statistical-only quality)"
  else
    fail "sweep-quality" "report misses wall-clock medians or quality verdicts; see sweep-report.md"
  fi
fi
# Sign-off verdict for this gate: PASS only on a full 49-of-49 measured
# pass. A subset run (any miss or any unexecuted reference) FAILS the
# gate naming the misses -- never a pass on a subset.
if [ "$SWEEP_EXIT" -eq 0 ]; then
  _done="$(grep -m1 'executed:' "$SWEEP_REPORT" 2>/dev/null || true)"
  _pass_n="$(printf '%s' "$_done" | sed -n 's/.*passing: \([0-9]*\).*/\1/p')"
  if [ "$_pass_n" = "49" ]; then
    pass "sweep-49-of-49 (faster-and-better on every reference)"
  else
    fail "sweep-49-of-49" "sweep exited 0 without 49 passing; see sweep-report.md"
  fi
else
  fail "sweep-49-of-49" "subset measured, gate refuses sign-off: ${SWEEP_VERDICT_LINE} | ${SWEEP_MISS_LINE} | 45 references unexecuted in this proof (full run measured separately); sweep exit $SWEEP_EXIT"
fi

# ---- gate 5: CI workflow shape --------------------------------------------
echo ""
echo "--- gate 5: CI workflow shape ---"
for RUNNER in macos-latest windows-latest ubuntu-latest; do
  if grep -q "$RUNNER" "$CI_FILE"; then
    pass "ci-leg-$RUNNER"
  else
    fail "ci-leg-$RUNNER" "runner $RUNNER absent from ci.yml"
  fi
done
if grep -q "fail-fast: false" "$CI_FILE"; then
  pass "ci-fail-fast-false"
else
  fail "ci-fail-fast" "fail-fast false absent from ci.yml"
fi
for CMD in "cargo build --workspace" "cargo test --workspace" "cargo test -p render"; do
  if grep -q "$CMD" "$CI_FILE"; then
    pass "ci-command-$CMD"
  else
    fail "ci-command-$CMD" "identical per-OS command '$CMD' absent from ci.yml"
  fi
done
if grep -q "9.0.2" "$CI_FILE" && grep -q "ffmpeg-registry" "$CI_FILE"; then
  pass "ci-ffmpeg-registry-pin-9.0.2"
else
  fail "ci-ffmpeg" "pinned ffmpeg-registry 9.0.2 absent from ci.yml"
fi
# Release workflow: tagged-only, four legs, incomplete-set refusal.
for SLUG in macos-arm64 macos-x64 windows-x64 linux-x64; do
  if grep -q "$SLUG" "$REL_FILE"; then
    pass "release-leg-$SLUG"
  else
    fail "release-leg-$SLUG" "slug $SLUG absent from release.yml"
  fi
done
if grep -q "fail-fast: false" "$REL_FILE"; then
  pass "release-fail-fast-false"
else
  fail "release-fail-fast" "fail-fast false absent from release.yml"
fi
if grep -q '"v\*"' "$REL_FILE" && grep -q "build-package.sh" "$REL_FILE"; then
  pass "release-tagged-only-via-build-package"
else
  fail "release-trigger" "v* tag trigger or build-package.sh absent from release.yml"
fi
if ruby -ryaml -e "ci=YAML.load_file('$CI_FILE'); rel=YAML.load_file('$REL_FILE'); raise 'ci no jobs' unless ci['jobs']; raise 'release no jobs' unless rel['jobs']; puts('ci jobs: ' + ci['jobs'].keys.join(',') + '; release jobs: ' + rel['jobs'].keys.join(','))" >"$WORKDIR/ci-yaml.log" 2>&1; then
  pass "ci-yaml-parses ($(cat "$WORKDIR/ci-yaml.log"))"
else
  fail "ci-yaml" "CI/release YAML failed parse; see ci-yaml.log"
fi
echo "INFO: CI live green run: PENDING (workflow authored and locally validated, not pushed per instructions; owner pushes)"

# ---- gate 6: targeted cargo tests on touched crates (no --workspace) -------
echo ""
echo "--- gate 6: targeted cargo tests on touched crates ---"
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p dist >"$WORKDIR/test-dist.log" 2>&1); then
  if grep -q "test result: ok" "$WORKDIR/test-dist.log" && grep -q "0 failed" "$WORKDIR/test-dist.log"; then
    pass "cargo-test-dist ($(grep -h 'test result:' "$WORKDIR/test-dist.log" | tr '\n' ';'))"
  else
    fail "dist-summary" "dist tests exited 0 without an ok/0-failed summary; see test-dist.log"
  fi
else
  fail "test-dist" "cargo test -p dist failed; see test-dist.log"
fi
grep -h "test result:" "$WORKDIR/test-dist.log" 2>/dev/null | sed 's/^/INFO: dist /'
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p ffmpeg >"$WORKDIR/test-ffmpeg.log" 2>&1); then
  if grep -q "test result: ok" "$WORKDIR/test-ffmpeg.log" && grep -q "0 failed" "$WORKDIR/test-ffmpeg.log"; then
    pass "cargo-test-ffmpeg ($(grep -h 'test result:' "$WORKDIR/test-ffmpeg.log" | tr '\n' ';'))"
  else
    fail "ffmpeg-summary" "ffmpeg tests exited 0 without an ok/0-failed summary; see test-ffmpeg.log"
  fi
else
  fail "test-ffmpeg" "cargo test -p ffmpeg failed; see test-ffmpeg.log"
fi
grep -h "test result:" "$WORKDIR/test-ffmpeg.log" 2>/dev/null | sed 's/^/INFO: ffmpeg /'
if (cd "$FRAMESMITH_DIR" && cargo test --offline -p cli --test cli_n27_version_update >"$WORKDIR/test-cli-n27.log" 2>&1); then
  if grep -q "test result: ok" "$WORKDIR/test-cli-n27.log" && grep -q "0 failed" "$WORKDIR/test-cli-n27.log"; then
    pass "cargo-test-cli-n27 ($(grep -h 'test result:' "$WORKDIR/test-cli-n27.log" | tr '\n' ';'))"
  else
    fail "cli-n27-summary" "cli n27 tests exited 0 without an ok/0-failed summary; see test-cli-n27.log"
  fi
  for TN in version_names_version_build_and_pin version_json_carries_build_and_pin update_check_reports_current_and_newer update_check_unreachable_is_unavailable_never_current update_apply_refuses_without_consent update_apply_checksum_mismatch_leaves_binary_working; do
    if grep -q "$TN ... ok" "$WORKDIR/test-cli-n27.log"; then
      pass "targeted-test-$TN"
    else
      fail "targeted-test-$TN" "test did not report ok; see test-cli-n27.log"
    fi
  done
else
  fail "test-cli-n27" "cargo test -p cli --test cli_n27_version_update failed; see test-cli-n27.log"
fi
grep -h "test result:" "$WORKDIR/test-cli-n27.log" 2>/dev/null | sed 's/^/INFO: cli-n27 /'

# ---- evidence ---------------------------------------------------------------
echo ""
echo "--- proof summary: $PASS_N passed, failing:${FAIL_LIST:- none} ---"
{
  echo "date(UTC): $STAMP"
  echo "machine: $(uname -srm)"
  echo "framesmith: $VER_LINE"
  echo "release version: $VERSION"
  echo "cargo: $(cargo --version 2>/dev/null)"
  echo "rustc: $(rustc --version 2>/dev/null)"
  echo "python: $(python3 --version 2>&1)"
  echo "ffmpeg: $(ffmpeg -version 2>/dev/null | head -n 1)"
  echo "git: $(git -C "$FRAMESMITH_DIR" rev-parse --short HEAD 2>/dev/null || echo unknown)"
  echo "workdir: $WORKDIR"
  echo "passed: $PASS_N"
  echo "failed:${FAIL_LIST:- none}"
  echo "sweep subset: name-card-template,media-ledger-card,keyframe-arc-sting,render-endpoint --reps 2"
  echo "CI live green run: PENDING (owner push)"
} >"$WORKDIR/report.txt"
cat "$WORKDIR/report.txt"
mkdir -p "$EVID_DIR"
cp "$WORKDIR/report.txt" "$WORKDIR/build.log" \
  "$WORKDIR/parity-check.log" "$WORKDIR/parity-verdict.md" \
  "$WORKDIR/parity-inventory-check.log" \
  "$WORKDIR/sweep-check.log" "$WORKDIR/sweep-run.log" "$WORKDIR/sweep-report.md" \
  "$WORKDIR/test-dist.log" "$WORKDIR/test-ffmpeg.log" "$WORKDIR/test-cli-n27.log" \
  "$WORKDIR/ci-yaml.log" "$WORKDIR/verify-set.log" "$WORKDIR/verify-incomplete.log" \
  "$WORKDIR/win-zip.log" "$WORKDIR/unpack.log" "$WORKDIR/install.log" \
  "$WORKDIR/install-tampered.log" "$WORKDIR/installed-version.log" \
  "$WORKDIR/render-install.log" "$WORKDIR/render-install2.log" \
  "$WORKDIR/version.log" "$WORKDIR/version-json.log" "$WORKDIR/version-json-check.log" \
  "$WORKDIR/upd-current.log" "$WORKDIR/upd-newer.log" "$WORKDIR/upd-newer-json.log" \
  "$WORKDIR/upd-newer-json-check.log" "$WORKDIR/upgrade.log" \
  "$WORKDIR/upd-unreach.log" "$WORKDIR/upd-unreach-json.log" "$WORKDIR/upd-unreach-json-check.log" \
  "$WORKDIR/upd-consent.log" "$WORKDIR/upd-mismatch.log" \
  "$WORKDIR/build-macos-arm64.log" "$WORKDIR/build-macos-x64.log" \
  "$WORKDIR/build-windows-x64.log" "$WORKDIR/build-linux-x64.log" \
  "$EVID_DIR/" 2>/dev/null || true
cp "$WORKDIR/proj/composition.kdl" "$EVID_DIR/" 2>/dev/null || true

if [ -n "$FAIL_LIST" ]; then
  echo ""
  echo "PROOF: FAIL -- failing items:$FAIL_LIST"
  exit 1
fi
echo ""
echo "PROOF: PASS"

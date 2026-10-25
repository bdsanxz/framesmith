#!/usr/bin/env bash
# The core suite of Framesmith: every test but the yardstick tests, run with only the system's folders
# on the search path and with the programs a test may start limited to the places of
# docs/dev/gates.md, section 10. It is the body of the `core` gate of gates.sh.
#
# Written for the bash 3.2 that macOS ships.

set -u

usage() {
    cat <<'EOT'
usage: scripts/gates/core-suite.sh [--root DIR] [--hosted] [-p CRATE]...

  Checks that no core test file names ffmpeg or ffprobe, then runs the tests of the workspace
  (by default the repository, or DIR) through scripts/gates/core-runner.sh, skipping the yardstick
  tests, and then, unless --hosted, the tests marked "reference machine only", which it counts.
  Every problem is a line that starts with "core problem:". Exit code 0 when there is none,
  1 otherwise, 2 for wrong usage.

  --root DIR   the workspace to test; the runner and the record are always this repository's
  --hosted     the hosted runner's form: tests marked "reference machine only" are not run
  -p CRATE     only this crate (may be given more than once)
EOT
}

REPO=$(cd "$(dirname "$0")/../.." && pwd -P) || exit 2
WORKSPACE="$REPO"
HOSTED=0
CRATES=""
while [ $# -gt 0 ]; do
    case "$1" in
        --root)
            [ $# -ge 2 ] || { echo "--root needs a folder" >&2; usage >&2; exit 2; }
            WORKSPACE=$(cd "$2" && pwd -P) || { echo "no folder: $2" >&2; exit 2; }
            shift
            ;;
        --hosted) HOSTED=1 ;;
        -p)
            [ $# -ge 2 ] || { echo "-p needs a crate name" >&2; usage >&2; exit 2; }
            CRATES="$CRATES $2"
            shift
            ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done
cd "$WORKSPACE" || exit 2

SCOPE=(--workspace)
if [ -n "$CRATES" ]; then
    SCOPE=()
    for crate in $CRATES; do SCOPE+=(-p "$crate"); done
fi

export CARGO_TERM_COLOR=never
export NO_COLOR=1
# A run of the control or of the suite reports to no runner (docs/dev/gates.md, section 12).
unset GITHUB_ACTIONS GITHUB_STEP_SUMMARY GITHUB_OUTPUT GITHUB_ENV GITHUB_PATH

TARGET=$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"target_directory":"\([^"]*\)".*/\1/p')
if [ -z "$TARGET" ]; then
    echo "core problem: cargo could not name the target folder of $WORKSPACE"
    exit 1
fi
LOGS="$TARGET/gates"
mkdir -p "$LOGS" || { echo "cannot make $LOGS" >&2; exit 2; }

# The places besides the target and the record that a test may start programs from.
SYSROOT=$(rustc --print sysroot 2>/dev/null)
DEVTOOLS=$(xcode-select -p 2>/dev/null)
if [ -z "$SYSROOT" ]; then
    echo "core problem: rustc could not name the folder of the toolchain"
    exit 1
fi
export FRAMESMITH_CORE_TARGET="$TARGET"
export FRAMESMITH_CORE_TOOLCHAIN="$SYSROOT"
export FRAMESMITH_CORE_DEVTOOLS="$DEVTOOLS"
# The token of this run: the runner starts each test program under a name that begins with it, and the
# reading of refused starts counts only the refusals recorded for such a name, so that a run of another
# copy, or one at the same time, is not taken for this one (docs/dev/gates.md, section 10).
FRAMESMITH_CORE_RUN_TOKEN=r$(printf '%s %s %s %s' "$RANDOM" "$$" "$(date +%s)" "$WORKSPACE" | shasum | cut -c1-8)
export FRAMESMITH_CORE_RUN_TOKEN

RUNNER="$REPO/scripts/gates/core-runner.sh"
CONFIG="target.'cfg(all())'.runner=[\"$RUNNER\"]"
# The system log is read to the second, so the window starts on the next whole second: a start
# refused just before this run (by a run of the control, say) is not this run's.
sleep 1
STARTED=$(date '+%Y-%m-%d %H:%M:%S')
FAILED=0

# A comparison with FFmpeg lives in a file or a folder of a test file whose name starts with
# `yardstick`; a core test file that names ffmpeg or ffprobe is a failure, with the test function
# around the line.
placement() {
    ( cd "$WORKSPACE/crates" 2>/dev/null && find . -path '*/tests/*' -name '*.rs' ! -name 'yardstick*' ! -path '*/yardstick*/*' | sort ) |
        while IFS= read -r file; do
            awk -v file="crates/${file#./}" '
                /^[ \t]*(pub[ \t]+)?(async[ \t]+)?fn[ \t]+[A-Za-z0-9_]+/ {
                    current = $0
                    sub(/^[ \t]*(pub[ \t]+)?(async[ \t]+)?fn[ \t]+/, "", current)
                    sub(/[^A-Za-z0-9_].*$/, "", current)
                }
                tolower($0) ~ /ffmpeg|ffprobe/ {
                    printf "core problem: the core test file %s names ffmpeg or ffprobe at line %d (in fn %s); a comparison with FFmpeg belongs in a file or a folder whose name starts with yardstick\n", file, FNR, (current == "" ? "none" : current)
                }
            ' "$WORKSPACE/crates/${file#./}"
        done
}

# The programs the sandbox refused to start since the run began, read from the system log when it
# can be read, as scripts/gates/core-refused.sh does ($1 is the log of the run). The system writes
# its record of a refusal some time after it refuses the start, so that script reads the log for a
# short, bounded time and not once (docs/dev/gates.md, section 10, "Reading the refused starts").
refused_starts() {
    bash "$REPO/scripts/gates/core-refused.sh" --since "$STARTED" --run-log "$1"
}

# Runs the tests through the runner, with the output streamed and kept in $LOGS/$1.log.
run_tests() {
    local name="$1"
    shift
    echo "\$ cargo test ${SCOPE[*]} --lib --bins --tests --no-fail-fast --config <runner: scripts/gates/core-runner.sh> -- --skip yardstick_ $*"
    ( cargo test "${SCOPE[@]}" --lib --bins --tests --no-fail-fast --config "$CONFIG" -- --skip yardstick_ "$@" 2>&1; echo $? >"$LOGS/$name.code" ) | tee "$LOGS/$name.log"
    RUN_CODE=$(cat "$LOGS/$name.code")
    # The runner's links of this run, one beside each test program it started.
    find "$TARGET" -name "$FRAMESMITH_CORE_RUN_TOKEN-*" -type f -delete 2>/dev/null
    echo "[exit $RUN_CODE]"
}

echo "core suite in $WORKSPACE; the tests get the search path /usr/bin:/bin:/usr/sbin:/sbin"

PLACED=$(placement)
if [ -n "$PLACED" ]; then
    echo "$PLACED"
    FAILED=1
fi

run_tests core-suite
if [ "$RUN_CODE" -ne 0 ]; then
    FAILED=1
    refused_starts "$LOGS/core-suite.log"
fi

if [ "$HOSTED" -eq 0 ]; then
    MARKED=$(grep -c '^test .* \.\.\. ignored, reference machine only' "$LOGS/core-suite.log")
    echo
    echo "-- reference machine only: $MARKED marked test(s) run here, as the hosted run cannot run them"
    run_tests core-reference --ignored reference_machine_only_
    if [ "$RUN_CODE" -ne 0 ]; then
        FAILED=1
        refused_starts "$LOGS/core-reference.log"
    fi
    RAN=$(grep -c '^test .* \.\.\. ok$' "$LOGS/core-reference.log")
    echo "reference machine only: $MARKED marked, $RAN ran and passed"
    if [ "$RAN" -ne "$MARKED" ]; then
        echo "core problem: $MARKED test(s) are marked reference machine only and $RAN ran passing"
        FAILED=1
    fi
fi

exit "$FAILED"

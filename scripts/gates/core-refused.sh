#!/usr/bin/env bash
# The programs the sandbox of the core suite refused to start, named from the system log.
# Called by core-suite.sh after a run that failed. The system writes its record of a refusal some
# time after it refuses the start, so the log is read again and again for a short, bounded time
# (docs/dev/gates.md, section 10, "Reading the refused starts"), not once.
#
# Written for the bash 3.2 that macOS ships.

set -u

usage() {
    cat <<'EOT'
usage: scripts/gates/core-refused.sh --since 'YYYY-MM-DD HH:MM:SS' --run-log FILE

  Prints "core problem: the system refused to start <program>" for each program the system refused
  to start since the given time to a test program of this run: one started under a name that begins
  with the run's token, as the runner starts them. A refusal recorded under another run's token, or for
  one of the test programs named in FILE, the output of the run, under its plain name, is not counted
  and is printed as "set aside, refused in another run: <program> (by <name>)". A refusal recorded
  under no token for a program built in the run's target folder (an executable file directly in
  <target>/debug or <target>/release, such as the framesmith binary a test starts) is not counted
  either, but is named, as "refused, run unknown: <program> (by <name>)", so that a failing run still
  names what was refused. The log is read
  waiting for the system's record as the document says. The problems are the lines it prints; the exit
  code is 0, or 2 for wrong usage.

  FRAMESMITH_CORE_TARGET       the target folder of the run, as core-suite.sh sets it
  FRAMESMITH_CORE_RUN_TOKEN    the token of the run (r and 8 hexadecimal digits), as core-suite.sh sets it
  FRAMESMITH_CORE_LOG_COMMAND  a program run in place of `log`, for the tests of this script only
EOT
}

SINCE=""
RUNLOG=""
while [ $# -gt 0 ]; do
    case "$1" in
        --since)
            [ $# -ge 2 ] || { echo "--since needs a time" >&2; usage >&2; exit 2; }
            SINCE="$2"
            shift
            ;;
        --run-log)
            [ $# -ge 2 ] || { echo "--run-log needs a file" >&2; usage >&2; exit 2; }
            RUNLOG="$2"
            shift
            ;;
        -h | --help) usage; exit 0 ;;
        *) echo "unknown option: $1" >&2; usage >&2; exit 2 ;;
    esac
    shift
done
[ -n "$SINCE" ] && [ -f "$RUNLOG" ] || { usage >&2; exit 2; }
TOKEN="${FRAMESMITH_CORE_RUN_TOKEN:-}"
case "$TOKEN" in
    r[0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef]) ;;
    *) echo "FRAMESMITH_CORE_RUN_TOKEN must be r and 8 hexadecimal digits, as core-suite.sh sets it: '$TOKEN'" >&2; usage >&2; exit 2 ;;
esac

LOG_COMMAND="${FRAMESMITH_CORE_LOG_COMMAND:-log}"
# The shortest wait after the tests end, the bound after which no new reading is begun, the most one
# reading may take, and the pause between two readings. The same numbers as the document's.
SETTLE_MS=3000
BOUND_MS=20000
READ_LIMIT=30
PAUSE=1

# The log belongs to the whole machine, and every copy of the repository builds test programs of the
# same names. A refusal is kept only when the process that was refused was started under this run's
# name, which begins with the token (the system keeps the first 32 characters of a name). A refusal
# recorded under another run's token, or for one of this run's test programs under its plain name, is
# set aside and said so; any other record is not this gate's and is passed over.
PROGRAMS=$(sed -n 's/^ *Running .* (.*\/deps\/\([^/)]*\))$/\1/p' "$RUNLOG" | sort -u)
# The programs built in the run's target folder besides the test programs: a test may start one (the
# framesmith binary, by CARGO_BIN_EXE_framesmith), and its refusals carry its own name, with no token.
BUILT=""
if [ -n "${FRAMESMITH_CORE_TARGET:-}" ]; then
    BUILT=$(for profile in debug release; do
        for file in "$FRAMESMITH_CORE_TARGET/$profile"/*; do
            [ -f "$file" ] && [ -x "$file" ] && basename "$file"
        done
    done | sort -u)
fi
# The output of the tests points to a refusal for each start the profile forbade.
POINTED=$(grep -c 'Operation not permitted' "$RUNLOG")

now_ms() { perl -MTime::HiRes=time -e 'printf "%d\n", time() * 1000'; }

READING=""   # the programs named by the latest reading that could be read
RUN_UNKNOWN="" # the refusals of the latest reading that could be read by a built program, under no token
SET_ASIDE="" # the refusals of the latest reading that could be read that were recorded for another run
READ_OK=0    # 1 when the latest reading could be read

read_once() {
    local raw
    raw=$(perl -e 'alarm shift; exec @ARGV' "$READ_LIMIT" "$LOG_COMMAND" show --start "$SINCE" --predicate 'eventMessage CONTAINS "deny(1) process-exec"' --style compact)
    if [ $? -ne 0 ]; then
        READ_OK=0
        return
    fi
    READ_OK=1
    local records
    records=$(printf '%s\n' "$raw" |
        sed -n 's/^.*Sandbox: \([^ (]*\)([0-9]*) deny(1) process-exec\*\{0,1\} \(.*\)$/\1 \2/p')
    READING=$(printf '%s\n' "$records" |
        while read -r process path; do
            case "$process" in
                ("$TOKEN"-*) echo "core problem: the system refused to start $path" ;;
            esac
        done | sort -u)
    SET_ASIDE=$(printf '%s\n' "$records" |
        while read -r process path; do
            case "$process" in ("$TOKEN"-* | '') continue ;; esac
            # Another run's token proves the record is another run's, whatever program it names.
            case "$process" in
                (r[0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef]-*)
                    echo "set aside, refused in another run: $path (by $process)"
                    continue ;;
            esac
            plain="$process"
            if printf '%s\n' "$PROGRAMS" | awk -v process="$plain" 'index($0, process) == 1 { found = 1 } END { exit !found }'; then
                echo "set aside, refused in another run: $path (by $process)"
            fi
        done | sort -u)
    RUN_UNKNOWN=$(printf '%s\n' "$records" |
        while read -r process path; do
            case "$process" in ('') continue ;; (r[0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef][0123456789abcdef]-*) continue ;; esac
            # A test program's own name is set aside above, not named here.
            if printf '%s\n' "$PROGRAMS" | awk -v process="$process" 'index($0, process) == 1 { found = 1 } END { exit !found }'; then
                continue
            fi
            if printf '%s\n' "$BUILT" | awk -v process="$process" 'process != "" && index($0, process) == 1 { found = 1 } END { exit !found }'; then
                echo "refused, run unknown: $path (by $process)"
            fi
        done | sort -u)
}

count_named() { printf '%s\n' "$READING" | grep -c .; }

START=$(now_ms)
PREVIOUS=""
HAVE_PREVIOUS=0
FAILURES=0
while :; do
    read_once
    ELAPSED=$(($(now_ms) - START))
    if [ "$READ_OK" -eq 1 ]; then
        FAILURES=0
        if [ "$ELAPSED" -ge "$SETTLE_MS" ] && [ "$HAVE_PREVIOUS" -eq 1 ] && [ "$READING" = "$PREVIOUS" ] &&
            [ "$(count_named)" -ge "$POINTED" ]; then
            break
        fi
        PREVIOUS="$READING"
        HAVE_PREVIOUS=1
    else
        FAILURES=$((FAILURES + 1))
        HAVE_PREVIOUS=0
        [ "$FAILURES" -ge 2 ] && break
    fi
    [ "$ELAPSED" -ge "$BOUND_MS" ] && break
    sleep "$PAUSE"
    # The bound is looked at again after the pause, so that no reading is begun after it (rule 2).
    [ "$(($(now_ms) - START))" -ge "$BOUND_MS" ] && break
done

[ -n "$READING" ] && printf '%s\n' "$READING"
[ -n "$SET_ASIDE" ] && printf '%s\n' "$SET_ASIDE"
[ -n "$RUN_UNKNOWN" ] && printf '%s\n' "$RUN_UNKNOWN"
if [ "$FAILURES" -ge 2 ]; then
    echo "core problem: the system log could not be read, so the programs the system refused to start are not named"
elif [ "$(count_named)" -lt "$POINTED" ]; then
    echo "core problem: after waiting $((BOUND_MS / 1000)) seconds the system log names $(count_named) program(s) the system refused to start, and the output of the tests shows $POINTED start(s) refused (\"Operation not permitted\"); a refused program may be missing from this list"
fi
exit 0

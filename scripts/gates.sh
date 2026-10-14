#!/usr/bin/env bash
# The quality gates of the Framesmith repository: formatting, static analysis, the tests, the core
# suite with only the system on the search path, the licence gate and the frx7 pin. The same script runs them before a push (.githooks/pre-push) and
# on the hosted runner (.github/workflows/gates.yml). The behaviour is specified in docs/dev/gates.md.
#
# Written for the bash 3.2 that macOS ships.

set -u

usage() {
    cat <<'EOF'
usage: scripts/gates.sh [--hosted] [--gate NAME]... [-p CRATE]...

  With no option it runs the six gates, then the tests marked "reference machine only" and the
  yardstick tests (which start ffmpeg and ffprobe, or the reference, from outside, to compare; where
  the program is not installed each is reported as skipped, by name, and never as passed).
  Each gate reports PASS, or FAIL with what failed. All of them run; the exit code is
  0 when every gate passed, 1 when any failed.

gates (NAME):
  format    formatting         cargo fmt --all -- --check
  lint      static analysis    cargo clippy --workspace --all-targets -- -D warnings
  test      tests              cargo test --workspace --no-fail-fast
  core      core suite         the tests again with only the system on the search path and no program
                               started but those of docs/dev/gates.md, section 10
                               (scripts/gates/core-selftest.sh, then scripts/gates/core-suite.sh)
  licence   licence gate       cargo deny check licenses
  pin       frx7 pin           scripts/gates/frx7.sh check (no path override committed, one pinned revision, the pin moved with its record)

options:
  --gate NAME   run only this gate (may be given more than once). NARROWED.
  -p CRATE      narrow format, lint and test to this crate (may be given more than once).
                The licence gate and the frx7 pin gate always look at the whole workspace. NARROWED.
  --hosted      the hosted runner's form: the tests marked "reference machine only" and the
                yardstick tests are listed as not run and never run (in the core gate, the tests
                marked "reference machine only" are not run). The hosted workflow runs
                one job for each gate, so --hosted with --gate is not narrowed.
  -h, --help    this text

On a hosted runner that has a job summary file (GITHUB_STEP_SUMMARY), the run ends by printing the
content of that file into the log, between two marker lines (docs/dev/gates.md, section 13).

A NARROWED run says so on its first and last lines, and exits 3 when its part passed: it is
never a pass of the gates. Exit codes: 0 all gates passed, 1 a gate failed, 2 wrong usage,
3 narrowed run whose part passed.
EOF
}

ROOT=$(cd "$(dirname "$0")/.." && pwd) || exit 2
cd "$ROOT" || exit 2

HOSTED=0
WANTED=""
CRATES=""
while [ $# -gt 0 ]; do
    case "$1" in
        --hosted) HOSTED=1 ;;
        --gate)
            [ $# -ge 2 ] || { echo "--gate needs a name" >&2; usage >&2; exit 2; }
            case "$2" in
                format | lint | test | core | licence | pin) WANTED="$WANTED $2" ;;
                *) echo "unknown gate: $2 (there are format, lint, test, core, licence, pin)" >&2; usage >&2; exit 2 ;;
            esac
            shift
            ;;
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

# A run is narrowed when it leaves out a gate or a crate. The hosted workflow runs each gate as a
# job of its own, so a gate named under --hosted is not a narrowing: the five jobs are the gates.
NARROWED=0
[ -n "$CRATES" ] && NARROWED=1
[ -n "$WANTED" ] && [ "$HOSTED" -eq 0 ] && NARROWED=1
[ -z "$WANTED" ] && WANTED=" format lint test core licence pin"

# The scope of the cargo commands: the whole workspace, or the crates named with -p.
if [ -z "$CRATES" ]; then
    FMT_SCOPE=(--all)
    SCOPE=(--workspace)
else
    SCOPE=()
    for crate in $CRATES; do SCOPE+=(-p "$crate"); done
    FMT_SCOPE=("${SCOPE[@]}")
fi

export CARGO_TERM_COLOR=never
export NO_COLOR=1

LOGS="${CARGO_TARGET_DIR:-$ROOT/target}/gates"
mkdir -p "$LOGS" || { echo "cannot make $LOGS" >&2; exit 2; }

ON_GITHUB=0
[ "${GITHUB_ACTIONS:-}" = "true" ] && ON_GITHUB=1

NARROWED_WHAT=""
if [ "$NARROWED" -eq 1 ]; then
    NARROWED_WHAT="gates:${WANTED}"
    [ -n "$CRATES" ] && NARROWED_WHAT="$NARROWED_WHAT; crates:$CRATES"
    echo "NARROWED RUN ($NARROWED_WHAT). This is not a pass of the gates."
    echo
fi

SUMMARY=""     # one line for each gate: "PASS name" or "FAIL name: what"
NOTES=""       # what was not run, said under the summary
FAILED=0
NOT_RUN_LIST=""

say_summary() { SUMMARY="$SUMMARY$1"$'\n'; }
add_note() { NOTES="$NOTES$1"$'\n'; }

# Annotation and job summary lines for the hosted run. Newlines are not allowed in an annotation.
github_error() {
    [ "$ON_GITHUB" -eq 1 ] || return 0
    echo "::error title=$1::$(printf '%s' "$2" | tr '\n' ' ' | cut -c1-900)"
}
github_summary() {
    [ "$ON_GITHUB" -eq 1 ] && [ -n "${GITHUB_STEP_SUMMARY:-}" ] && printf '%s\n' "$1" >>"$GITHUB_STEP_SUMMARY"
    return 0
}

# At its very end a run on a hosted runner prints the job summary file as it then is, between two marker
# lines, so that the log of the job holds what the summary page of the job shows: the page of a private
# repository can be read by nobody who is not signed in, the log can be read by the hosting service's
# command line (docs/dev/gates.md, section 3). Whatever wrote the content is shown, the script or any
# program it started. Every line is printed after "| ", so that a line of the content never begins with
# "::" or "##[" and the runner takes none for a command of its own; a carriage return, which the runner
# reads as the end of a line, and a null byte are turned into line ends first. A local run prints nothing.
JOB_SUMMARY_BEGIN="==== framesmith gates: job summary, begin ===="
print_job_summary() {
    [ "$ON_GITHUB" -eq 1 ] && [ -n "${GITHUB_STEP_SUMMARY:-}" ] || return 0
    local body="" count=0
    echo "$JOB_SUMMARY_BEGIN"
    if [ -f "$GITHUB_STEP_SUMMARY" ] && [ -r "$GITHUB_STEP_SUMMARY" ]; then
        body=$(tr '\r\000' '\n\n' <"$GITHUB_STEP_SUMMARY" | sed 's/^/| /')
        if [ -n "$body" ]; then
            count=$(printf '%s\n' "$body" | wc -l | tr -d ' ')
            printf '%s\n' "$body"
        fi
        echo "==== framesmith gates: job summary, end ($count line(s)) ===="
    else
        echo "==== framesmith gates: job summary, end (the file cannot be read) ===="
    fi
}

# Ends the run with the exit code $1, after the job summary was printed.
finish() {
    print_job_summary
    exit "$1"
}

# Prints the command, runs it with its output streamed and kept in $LOGS/$1.log; the exit code is
# left in RUN_CODE.
RUN_CODE=0
run() {
    local name="$1"
    shift
    echo "\$ $*"
    [ "$ON_GITHUB" -eq 1 ] && echo "::group::output of: $*"
    ( "$@" 2>&1; echo $? >"$LOGS/$name.code" ) | tee "$LOGS/$name.log"
    [ "$ON_GITHUB" -eq 1 ] && echo "::endgroup::"
    RUN_CODE=$(cat "$LOGS/$name.code")
    echo "[exit $RUN_CODE]"
}

# The errors and warnings of rustc and clippy with their places: "message (file:line:col)".
compiler_problems() {
    awk '
        /^(error|warning)(\[[^]]*\])?: / {
            head = $0
            if (head ~ /could not compile|build failed|aborting due to|warnings? emitted|test failed, to rerun|targets? failed|previous errors?|previous warnings?/) { head = ""; next }
            next
        }
        /^ *--> / { if (head != "") { sub(/^ *--> /, ""); print head " (" $0 ")"; head = "" } }
    ' "$1" | sort -u
}

# --- the gates --------------------------------------------------------------------------------

gate_format() {
    NAME="formatting"
    run format cargo fmt "${FMT_SCOPE[@]}" -- --check
}

gate_lint() {
    NAME="static analysis"
    run lint cargo clippy "${SCOPE[@]}" --all-targets -- -D warnings
}

gate_test() {
    NAME="tests"
    run test cargo test "${SCOPE[@]}" --no-fail-fast
}

# The core suite with only the system on the search path. First the control: a workspace of tests
# that misbehave on purpose must be failed by name, or the trap is not closed. Then the real suite.
# The control's deliberate failures are printed here and reach no runner (docs/dev/gates.md, section 12).
gate_core() {
    NAME="core suite without FFmpeg"
    local suite=(bash "$ROOT/scripts/gates/core-suite.sh")
    [ "$HOSTED" -eq 1 ] && suite+=(--hosted)
    for crate in $CRATES; do suite+=(-p "$crate"); done
    run core-selftest bash "$ROOT/scripts/gates/core-selftest.sh"
    local control=$RUN_CODE
    run core "${suite[@]}"
    [ "$control" -ne 0 ] && RUN_CODE=1
    return 0
}

gate_licence() {
    NAME="licence gate"
    if ! cargo deny --version >/dev/null 2>&1; then
        echo "cargo-deny is not installed. Install it with: cargo install --locked cargo-deny --version 0.20.2"
        RUN_CODE=127
        echo "cargo-deny is not installed" >"$LOGS/licence.log"
        return
    fi
    run licence cargo deny check licenses
}

gate_pin() {
    NAME="frx7 pin"
    run pin bash "$ROOT/scripts/gates/frx7.sh" check
}

# --- what a failure names ---------------------------------------------------------------------

named_format() {
    grep '^Diff in ' "$LOGS/format.log" | sed -e 's/^Diff in //' -e 's/:[0-9]*:*$//' -e "s|^$ROOT/||" | sort -u |
        sed 's/^/formatting differs in /'
}

named_lint() { compiler_problems "$LOGS/lint.log"; }

named_test() {
    # A failing test is named with the place of its panic; where the panic names none, with the test file.
    # The log is $1, or the log of the tests.
    awk '
        /^ *Running / { context = $0; sub(/^ *Running /, "", context); sub(/ \(.*$/, "", context) }
        /^ *Doc-tests / { context = $0; sub(/^ */, "", context) }
        /^thread \047.*\047.* panicked at / {
            who = $0; sub(/^thread \047/, "", who); sub(/\047.*$/, "", who)
            where = $0; sub(/^.* panicked at /, "", where); sub(/:$/, "", where)
            place[who] = where
        }
        /^test .* \.\.\. FAILED$/ { count++; failed[count] = $2; seen[$2] = context }
        END {
            for (i = 1; i <= count; i++) {
                name = failed[i]
                print "test " name " failed (" ((name in place) ? place[name] : seen[name]) ")"
            }
        }
    ' "${1:-$LOGS/test.log}"
    compiler_problems "${1:-$LOGS/test.log}"
}

named_core() {
    grep -h '^core self-test problem: ' "$LOGS/core-selftest.log" | sed 's/^core self-test problem: /the control of this gate failed: /'
    grep -h '^core problem: ' "$LOGS/core.log" | sed 's/^core problem: //'
    named_test "$LOGS/core.log"
}

named_pin() { grep '^refused: ' "$LOGS/pin.log" | sed 's/^refused: //'; }

# A fetch of frx7 that cargo could not make, from its "unable to update <remote>?rev=<revision>" line
# (docs/dev/frx7.md, section 7). Any gate that builds can print it.
named_fetch() {
    grep -h 'unable to update [^ ]*bdsanxz/frx7?rev=' "$LOGS/$1.log" 2>/dev/null |
        sed -e 's|^.*unable to update \([^ ]*bdsanxz/frx7\)?rev=\([0-9A-Za-z]*\).*$|frx7 revision \2 could not be fetched from \1; see docs/dev/frx7.md, section 7|' | sort -u
}

named_licence() {
    awk '
        /^error\[rejected\]/ { inrej = 1; lic = ""; next }
        /^error\[unlicensed\]: / { inrej = 0; line = $0; sub(/^error\[unlicensed\]: /, "", line); print line " (no licence is stated)"; next }
        /^(error|warning)/ { inrej = 0 }
        inrej && /^ *├ [A-Za-z0-9.+-]+ - / { l = $0; sub(/^ *├ /, "", l); sub(/ - .*$/, "", l); lic = (lic == "" ? l : lic ", " l); next }
        inrej && /^ *├ [A-Za-z0-9_.-]+ v[0-9]/ { d = $0; sub(/^ *├ /, "", d); print d " is under " (lic == "" ? "a licence that is not on the list" : lic) " (not allowed)"; inrej = 0 }
    ' "$LOGS/licence.log"
    if ! grep -q 'licenses \(ok\|FAILED\)' "$LOGS/licence.log"; then
        sed -n '1p' "$LOGS/licence.log"
    fi
}

# --- the tests marked "reference machine only", and the yardstick tests ---------------------------

# Looks at the output of the plain test run. A marked test is ignored with its reason. A test named
# as marked that is not ignored with that reason would run on the hosted runner, which is a failure
# of the marking. The same holds for the yardstick tests.
check_marks() {
    MARKED=$(grep -c '^test .* \.\.\. ignored, reference machine only' "$LOGS/test.log")
    UNMARKED=$(grep -E '^test (.*::)?reference_machine_only_[A-Za-z0-9_]* \.\.\. ' "$LOGS/test.log" | grep -v ' ignored, reference machine only' | sed -e 's/^test //' -e 's/ \.\.\. .*$//')
    NOT_RUN_LIST=$(grep '^test .* \.\.\. ignored, reference machine only' "$LOGS/test.log" | sed -e 's/^test //' -e 's/ \.\.\. ignored, / : /')
    Y_MARKED=$(grep -c '^test .* \.\.\. ignored, yardstick' "$LOGS/test.log")
    Y_UNMARKED=$(grep -E '^test (.*::)?yardstick_[A-Za-z0-9_]* \.\.\. ' "$LOGS/test.log" | grep -v ' ignored, yardstick' | sed -e 's/^test //' -e 's/ \.\.\. .*$//')
    Y_NOT_RUN_LIST=$(grep '^test .* \.\.\. ignored, yardstick' "$LOGS/test.log" | sed -e 's/^test //' -e 's/ \.\.\. ignored, / : /')
}

reference_machine_run() {
    # Runs every marked test, here, on the reference machine, and counts them.
    echo
    echo "-- reference machine only: $MARKED marked test(s) run here, as the hosted run cannot run them"
    run reference cargo test "${SCOPE[@]}" --no-fail-fast -- --ignored reference_machine_only_
    REF_CODE=$RUN_CODE
    RAN=$(grep -c '^test .* \.\.\. ok$' "$LOGS/reference.log")
    REF_FAILED=$(grep '^test .* \.\.\. FAILED$' "$LOGS/reference.log" | sed -e 's/^test //' -e 's/ \.\.\. FAILED$//')
}

yardstick_run() {
    # Asks every yardstick test to run (docs/dev/gates.md, section 5). Where the program a test
    # compares with is installed it runs; where it is not, the runner reports the test as skipped,
    # by name. Counts both.
    echo
    echo "-- yardstick: $Y_MARKED marked test(s), asked to run here; each is run if its program is installed, and reported as skipped, by name, if it is not"
    run yardstick cargo test "${SCOPE[@]}" --no-fail-fast -- --ignored yardstick_
    Y_CODE=$RUN_CODE
    Y_RAN=$(grep -c '^test .* \.\.\. ok$' "$LOGS/yardstick.log")
    Y_SKIPPED=$(grep -c '^test .* \.\.\. skipped, ' "$LOGS/yardstick.log")
    Y_SKIPPED_LIST=$(grep '^test .* \.\.\. skipped, ' "$LOGS/yardstick.log" | sed -e 's/^test //' -e 's/ \.\.\. skipped, / : /')
    Y_FAILED=$(grep '^test .* \.\.\. FAILED$' "$LOGS/yardstick.log" | sed -e 's/^test //' -e 's/ \.\.\. FAILED$//')
}

# --- run ----------------------------------------------------------------------------------------

echo "Framesmith gates in $ROOT"
echo "$(cargo --version 2>&1); $(rustc --version 2>&1); $(cargo deny --version 2>&1 | head -n 1)"
echo

for gate in $WANTED; do
    SECONDS=0
    echo "== $gate"
    case "$gate" in
        format) gate_format ;;
        lint) gate_lint ;;
        test) gate_test ;;
        core) gate_core ;;
        licence) gate_licence ;;
        pin) gate_pin ;;
    esac
    problems=""
    if [ "$RUN_CODE" -ne 0 ]; then
        case "$gate" in
            format) problems=$(named_format) ;;
            lint) problems=$(named_lint) ;;
            test) problems=$(named_test) ;;
            core) problems=$(named_core) ;;
            licence) problems=$(named_licence) ;;
            pin) problems=$(named_pin) ;;
        esac
        fetched=$(named_fetch "$gate")
        if [ -n "$fetched" ]; then
            problems="$fetched"$'\n'"$problems"
        fi
    fi

    if [ "$gate" = "test" ]; then
        MARKED=0
        UNMARKED=""
        NOT_RUN_LIST=""
        Y_MARKED=0
        Y_UNMARKED=""
        Y_NOT_RUN_LIST=""
        [ -f "$LOGS/test.log" ] && check_marks
        if [ -n "$UNMARKED" ]; then
            RUN_CODE=1
            problems="$problems"$'\n'"named reference_machine_only_ but not marked with #[ignore = \"reference machine only: ...\"]: $(echo "$UNMARKED" | tr '\n' ' ' | sed 's/ *$//')"
        fi
        if [ -n "$Y_UNMARKED" ]; then
            RUN_CODE=1
            problems="$problems"$'\n'"named yardstick_ but not marked with #[ignore = \"yardstick: ...\"]: $(echo "$Y_UNMARKED" | tr '\n' ' ' | sed 's/ *$//')"
        fi
        TEST_CODE=$RUN_CODE
        if [ "$HOSTED" -eq 1 ]; then
            echo
            echo "-- not run (reference machine only): ${MARKED} test(s)"
            if [ -n "$NOT_RUN_LIST" ]; then
                echo "$NOT_RUN_LIST" | sed 's/^/   not run: /'
            fi
            echo "-- not run (yardstick, needs ffmpeg and ffprobe, or the reference): ${Y_MARKED} test(s)"
            if [ -n "$Y_NOT_RUN_LIST" ]; then
                echo "$Y_NOT_RUN_LIST" | sed 's/^/   not run: /'
            fi
            github_summary "### Not run on the hosted runner (reference machine only): ${MARKED}"
            [ -n "$NOT_RUN_LIST" ] && github_summary "$(echo "$NOT_RUN_LIST" | sed 's/^/- not run: /')"
            github_summary "### Not run on the hosted runner (yardstick): ${Y_MARKED}"
            [ -n "$Y_NOT_RUN_LIST" ] && github_summary "$(echo "$Y_NOT_RUN_LIST" | sed 's/^/- not run: /')"
            add_note "hosted run: $MARKED reference machine only test(s) and $Y_MARKED yardstick test(s) were not run"
        else
            reference_machine_run
            RUN_CODE=$TEST_CODE
            if [ -n "$REF_FAILED" ]; then
                RUN_CODE=1
                problems="$problems"$'\n'"reference machine only test failed: $(echo "$REF_FAILED" | tr '\n' ' ' | sed 's/ *$//')"
            elif [ "$REF_CODE" -ne 0 ]; then
                RUN_CODE=1
                problems="$problems"$'\n'"the reference machine only run ended with exit code $REF_CODE"
            fi
            if [ "$RAN" -ne "$MARKED" ]; then
                RUN_CODE=1
                problems="$problems"$'\n'"$MARKED test(s) are marked reference machine only and $RAN ran passing"
            fi
            echo "reference machine only: $MARKED marked, $RAN ran and passed"
            if [ "$Y_MARKED" -eq 0 ]; then
                echo "yardstick: 0 marked"
            else
                # The run sets RUN_CODE to its own exit code. What the plain run and the run of the
                # reference machine only tests decided stays the verdict of the gate: put it back, or
                # a passing yardstick run would wipe out a failure of the earlier runs.
                VERDICT_SO_FAR=$RUN_CODE
                yardstick_run
                RUN_CODE=$VERDICT_SO_FAR
                if [ -n "$Y_FAILED" ]; then
                    RUN_CODE=1
                    problems="$problems"$'\n'"yardstick test failed: $(echo "$Y_FAILED" | tr '\n' ' ' | sed 's/ *$//')"
                elif [ "$Y_CODE" -ne 0 ]; then
                    RUN_CODE=1
                    problems="$problems"$'\n'"the yardstick run ended with exit code $Y_CODE"
                fi
                if [ $((Y_RAN + Y_SKIPPED)) -ne "$Y_MARKED" ]; then
                    RUN_CODE=1
                    problems="$problems"$'\n'"$Y_MARKED test(s) are marked yardstick and $Y_RAN ran passing and $Y_SKIPPED were skipped"
                fi
                echo "yardstick: $Y_MARKED marked, $Y_RAN ran and passed, $Y_SKIPPED skipped"
                if [ "$Y_SKIPPED" -gt 0 ]; then
                    echo
                    echo "-- skipped (yardstick): $Y_SKIPPED test(s); the program they compare with was not found, so nothing was compared and none of them passed"
                    echo "$Y_SKIPPED_LIST" | sed 's/^/   skipped: /'
                    add_note "$Y_SKIPPED yardstick test(s) were skipped: the program they compare with was not found, so nothing was compared (they are not passed, and no line of ROADMAP.md may rest on them)"
                fi
            fi
        fi
    fi

    problems=$(printf '%s\n' "$problems" | sed '/^$/d')
    echo
    if [ "$RUN_CODE" -eq 0 ]; then
        echo "PASS $NAME (${SECONDS} s)"
        say_summary "PASS $NAME"
        github_summary "- PASS $NAME"
    else
        FAILED=1
        if [ -z "$problems" ]; then
            problems="the command ended with exit code $RUN_CODE and printed no cause the script can name; read its output above"
        fi
        echo "FAIL $NAME (${SECONDS} s): what failed:"
        echo "$problems" | sed 's/^/    /'
        say_summary "FAIL $NAME: $(echo "$problems" | head -n 1)$([ "$(echo "$problems" | wc -l)" -gt 1 ] && echo " (and $(( $(echo "$problems" | wc -l) - 1 )) more)")"
        github_error "$NAME failed" "$problems"
        github_summary "- FAIL $NAME"
        github_summary "$(echo "$problems" | sed 's/^/    /')"
    fi
    echo
done

echo "== summary"
printf '%s' "$SUMMARY"
if [ -n "$NOTES" ]; then
    printf 'note: %s' "$NOTES"
fi
if [ "$FAILED" -ne 0 ]; then
    if [ "$NARROWED" -eq 1 ]; then
        echo "NARROWED RUN ($NARROWED_WHAT): a part failed. This is not a pass of the gates."
    else
        echo "GATES FAILED"
    fi
    finish 1
fi
if [ "$NARROWED" -eq 1 ]; then
    echo "NARROWED RUN ($NARROWED_WHAT): its part passed. This is NOT a pass of the gates; run scripts/gates.sh with no option."
    finish 3
fi
if [ "$HOSTED" -eq 1 ]; then
    echo "HOSTED GATE PASSED"
else
    echo "GATES PASSED"
fi
finish 0

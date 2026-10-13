#!/usr/bin/env bash
# The quality gates of the Framesmith repository: formatting, static analysis, the tests and the
# licence gate. The same script runs them before a push (.githooks/pre-push) and on the hosted
# runner (.github/workflows/gates.yml). The behaviour is specified in docs/dev/gates.md.
#
# Written for the bash 3.2 that macOS ships.

set -u

usage() {
    cat <<'EOF'
usage: scripts/gates.sh [--hosted] [--gate NAME]... [-p CRATE]...

  With no option it runs the four gates, then the tests marked "reference machine only" and the
  yardstick tests (which start ffmpeg and ffprobe from outside, to compare, where they are installed).
  Each gate reports PASS, or FAIL with what failed. All of them run; the exit code is
  0 when every gate passed, 1 when any failed.

gates (NAME):
  format    formatting         cargo fmt --all -- --check
  lint      static analysis    cargo clippy --workspace --all-targets -- -D warnings
  test      tests              cargo test --workspace --no-fail-fast
  licence   licence gate       cargo deny check licenses

options:
  --gate NAME   run only this gate (may be given more than once). NARROWED.
  -p CRATE      narrow format, lint and test to this crate (may be given more than once).
                The licence gate always looks at the whole workspace. NARROWED.
  --hosted      the hosted runner's form: the tests marked "reference machine only" and the
                yardstick tests are listed as not run and never run. The hosted workflow runs
                one job for each gate, so --hosted with --gate is not narrowed.
  -h, --help    this text

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
                format | lint | test | licence) WANTED="$WANTED $2" ;;
                *) echo "unknown gate: $2 (there are format, lint, test, licence)" >&2; usage >&2; exit 2 ;;
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
# job of its own, so a gate named under --hosted is not a narrowing: the four jobs are the gates.
NARROWED=0
[ -n "$CRATES" ] && NARROWED=1
[ -n "$WANTED" ] && [ "$HOSTED" -eq 0 ] && NARROWED=1
[ -z "$WANTED" ] && WANTED=" format lint test licence"

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

# --- what a failure names ---------------------------------------------------------------------

named_format() {
    grep '^Diff in ' "$LOGS/format.log" | sed -e 's/^Diff in //' -e 's/:[0-9]*:*$//' -e "s|^$ROOT/||" | sort -u |
        sed 's/^/formatting differs in /'
}

named_lint() { compiler_problems "$LOGS/lint.log"; }

named_test() {
    # A failing test is named with the place of its panic; where the panic names none, with the test file.
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
    ' "$LOGS/test.log"
    compiler_problems "$LOGS/test.log"
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

# The programs a yardstick test starts from outside: found by looking in the folders of the search
# path, which starts nothing.
yardstick_programs_present() {
    command -v ffmpeg >/dev/null 2>&1 && command -v ffprobe >/dev/null 2>&1
}

yardstick_run() {
    # Runs every yardstick test where ffmpeg and ffprobe are installed, and counts them.
    echo
    echo "-- yardstick: $Y_MARKED marked test(s), run here because ffmpeg and ffprobe are installed here"
    run yardstick cargo test "${SCOPE[@]}" --no-fail-fast -- --ignored yardstick_
    Y_CODE=$RUN_CODE
    Y_RAN=$(grep -c '^test .* \.\.\. ok$' "$LOGS/yardstick.log")
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
        licence) gate_licence ;;
    esac
    problems=""
    if [ "$RUN_CODE" -ne 0 ]; then
        case "$gate" in
            format) problems=$(named_format) ;;
            lint) problems=$(named_lint) ;;
            test) problems=$(named_test) ;;
            licence) problems=$(named_licence) ;;
        esac
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
            echo "-- not run (yardstick, needs ffmpeg and ffprobe): ${Y_MARKED} test(s)"
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
            if yardstick_programs_present; then
                yardstick_run
                if [ -n "$Y_FAILED" ]; then
                    RUN_CODE=1
                    problems="$problems"$'\n'"yardstick test failed: $(echo "$Y_FAILED" | tr '\n' ' ' | sed 's/ *$//')"
                elif [ "$Y_CODE" -ne 0 ]; then
                    RUN_CODE=1
                    problems="$problems"$'\n'"the yardstick run ended with exit code $Y_CODE"
                fi
                if [ "$Y_RAN" -ne "$Y_MARKED" ]; then
                    RUN_CODE=1
                    problems="$problems"$'\n'"$Y_MARKED test(s) are marked yardstick and $Y_RAN ran passing"
                fi
                echo "yardstick: $Y_MARKED marked, $Y_RAN ran and passed"
            elif [ "$Y_MARKED" -eq 0 ]; then
                echo "yardstick: 0 marked"
            else
                echo
                echo "-- not run (yardstick): $Y_MARKED test(s); ffmpeg and ffprobe were not found in any folder of the search path, so nothing was compared"
                if [ -n "$Y_NOT_RUN_LIST" ]; then
                    echo "$Y_NOT_RUN_LIST" | sed 's/^/   not run: /'
                fi
                add_note "$Y_MARKED yardstick test(s) were not run: ffmpeg and ffprobe were not found on the search path, so nothing was compared (they are not passed)"
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
    exit 1
fi
if [ "$NARROWED" -eq 1 ]; then
    echo "NARROWED RUN ($NARROWED_WHAT): its part passed. This is NOT a pass of the gates; run scripts/gates.sh with no option."
    exit 3
fi
if [ "$HOSTED" -eq 1 ]; then
    echo "HOSTED GATE PASSED"
else
    echo "GATES PASSED"
fi
exit 0

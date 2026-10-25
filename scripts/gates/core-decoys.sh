#!/usr/bin/env bash
# The decoy programs of the control of the core gate, and their marker files. A decoy is a program
# that a test of the control's workspace starts by its path, and that the sandbox profile of the core
# suite must refuse. A decoy writes its marker file if it ever runs, so the control proves each refusal
# by the marker, and not by the system's log: `prove` shows, outside the profile, that each decoy writes
# its marker when it runs; `judge` requires, after the suite, that each decoy's test failed by its name
# with the refusal in its output, and that the marker is absent. See docs/dev/gates.md, section 10.
#
# Every problem is a line that starts with "core self-test problem:"; a note starts with
# "core self-test note:". Exit code 0 when there is none, 1 when there is one, 2 for wrong usage.
#
# Written for the bash 3.2 that macOS ships.

set -u

usage() {
    cat <<'EOT'
usage: scripts/gates/core-decoys.sh make FOLDER
       scripts/gates/core-decoys.sh prove FOLDER
       scripts/gates/core-decoys.sh judge FOLDER OUTPUT

  make   writes the decoy programs under FOLDER/decoy, each with its marker file named in FOLDER/markers
  prove  runs each decoy once outside the profile, requires its marker file to appear, then removes it
  judge  reads OUTPUT, the output of the suite run in the workspace of FOLDER: each decoy's test must have
         failed by its name with "Operation not permitted" in its output, and its marker file must be absent;
         a note says how many decoy programs the system log named as refused
EOT
}

# Each decoy: its path under the decoy folder, and the test of the control's workspace that starts it
# (the test names are those of scripts/gates/core-selftest.sh).
DECOYS="ffmpeg:starts_ffmpeg_by_its_path outside/bin/outside-command:starts_a_command_installed_outside_the_target_and_the_system"

STATUS=0
problem() {
    echo "core self-test problem: $1"
    STATUS=1
}

# The marker file of a decoy: a name in the markers folder, made of its path under the decoy folder
# with each slash turned into an underscore.
marker_of() {
    printf '%s/markers/%s\n' "$1" "${2//\//_}"
}

# The output of one test in the failures section of a suite run: the lines from its header to the next section.
block_of() {
    awk -v name="$1" '
        $0 == "---- " name " stdout ----" { on = 1; next }
        on && (/^---- / || /^failures:/ || /^test result:/) { on = 0 }
        on { print }
    ' "$2"
}

make_decoys() {
    local folder="$1" pair rel
    case "$folder" in
        *"'"*)
            problem "the folder of the decoys has a single quote in its name, which a decoy cannot hold: $folder"
            return 1
            ;;
    esac
    rm -rf "$folder/decoy" "$folder/markers"
    mkdir -p "$folder/decoy/outside/bin" "$folder/markers" || {
        problem "the decoy folders could not be made in $folder"
        return 1
    }
    for pair in $DECOYS; do
        rel=${pair%%:*}
        printf '#!/bin/sh\n: > '\''%s'\''\nexit 0\n' "$(marker_of "$folder" "$rel")" >"$folder/decoy/$rel" || {
            problem "the decoy decoy/$rel could not be written in $folder"
            return 1
        }
        chmod +x "$folder/decoy/$rel" || {
            problem "the decoy decoy/$rel could not be made runnable in $folder"
            return 1
        }
    done
    return 0
}

prove_decoys() {
    local folder="$1" pair rel marker
    for pair in $DECOYS; do
        rel=${pair%%:*}
        marker=$(marker_of "$folder" "$rel")
        if [ ! -x "$folder/decoy/$rel" ]; then
            problem "the decoy decoy/$rel is not in $folder, so its refusal cannot be proven"
            continue
        fi
        rm -f "$marker"
        # Run once outside the sandbox profile, for at most 30 seconds.
        perl -e 'alarm shift; exec @ARGV' 30 "$folder/decoy/$rel" </dev/null >/dev/null 2>&1
        if [ -f "$marker" ]; then
            echo "proved, outside the profile, that decoy/$rel writes its marker file when it runs"
            rm -f "$marker"
        else
            problem "decoy/$rel wrote no marker file when it ran outside the profile, so the absence of its marker would prove nothing"
        fi
    done
}

judge_decoys() {
    local folder="$1" output="$2" pair rel name marker refused named=0 block
    for pair in $DECOYS; do
        rel=${pair%%:*}
        name=${pair#*:}
        marker=$(marker_of "$folder" "$rel")
        refused=1
        if ! grep -q "^test $name \.\.\. FAILED$" "$output"; then
            problem "the test $name starts decoy/$rel and was not failed by its name"
            refused=0
        else
            block=$(block_of "$name" "$output")
            if ! printf '%s\n' "$block" | grep -q 'Operation not permitted'; then
                problem "the test $name failed, but its output does not show Operation not permitted for its start of decoy/$rel"
                refused=0
            fi
        fi
        if [ -e "$marker" ]; then
            problem "decoy/$rel ran: its marker file is present, so the system did not refuse its start"
            refused=0
        fi
        if [ "$refused" -eq 1 ]; then
            echo "refused, as it must: decoy/$rel (test $name failed by its name, its output shows Operation not permitted, its marker file is absent)"
        fi
    done
    # The record of a decoy names its path in full; only this control's own decoys count, never those of
    # a control of another copy running at the same time.
    local own
    own=$(cd "$folder" && pwd -P)
    for pair in $DECOYS; do
        rel=${pair%%:*}
        if grep -qFx "core problem: the system refused to start $own/decoy/$rel" "$output"; then
            named=$((named + 1))
        fi
    done
    if [ "$named" -eq 0 ]; then
        echo "core self-test note: the system log named no decoy program as refused in this run; each refusal is proven by its marker file, and the log is read as a note only"
    else
        echo "core self-test note: the system log named $named of the 2 decoy programs as refused; each refusal is proven by its marker file, and the log is read as a note only"
    fi
}

case "${1:-}" in
    make)
        [ $# -eq 2 ] || { usage >&2; exit 2; }
        make_decoys "$2" || exit 1
        ;;
    prove)
        [ $# -eq 2 ] || { usage >&2; exit 2; }
        prove_decoys "$2"
        ;;
    judge)
        [ $# -eq 3 ] || { usage >&2; exit 2; }
        [ -f "$3" ] || { echo "no file: $3" >&2; exit 2; }
        judge_decoys "$2" "$3"
        ;;
    -h | --help)
        usage
        exit 0
        ;;
    *)
        usage >&2
        exit 2
        ;;
esac
exit "$STATUS"

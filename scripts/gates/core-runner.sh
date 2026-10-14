#!/bin/bash
# The target runner of the core suite: cargo starts every test program through it, as
#   core-runner.sh <test program> [arguments]
# It starts the program with only the system's folders on the search path and under a system
# sandbox profile (sandbox-exec) that forbids starting any program except those of the places
# docs/dev/gates.md, section 10, lists: the target folder, the folders of scripts/gates/core-allowed.txt,
# the developer tools folder (FRAMESMITH_CORE_DEVTOOLS) and the Rust toolchain (FRAMESMITH_CORE_TOOLCHAIN).
# A start the profile forbids fails in the test that made it (Operation not permitted).
#
# FRAMESMITH_CORE_TARGET names the target folder; the default is CARGO_TARGET_DIR, or target/ of the
# repository.
#
# Written for the bash 3.2 that macOS ships.

set -u

SYSTEM_PATH=/usr/bin:/bin:/usr/sbin:/sbin
# The folders of the system's tools: the record may name nothing outside them.
SYSTEM_FOLDERS="/usr/bin /bin /usr/sbin /sbin /usr/libexec /System"

[ $# -ge 1 ] || { echo "usage: core-runner.sh <test program> [arguments]" >&2; exit 2; }

ROOT=$(cd "$(dirname "$0")/../.." && pwd -P) || exit 2
RECORD="$ROOT/scripts/gates/core-allowed.txt"
TARGET="${FRAMESMITH_CORE_TARGET:-${CARGO_TARGET_DIR:-$ROOT/target}}"
TARGET=$(cd "$TARGET" && pwd -P) || { echo "core-runner: no target folder: $TARGET" >&2; exit 2; }

# Nothing that could close a string of the profile may be in a path put into it.
safe() {
    case "$1" in
        *'"'* | *'\'* | *'('* | *')'* | *';'*) echo "core-runner: a path the profile cannot hold: $1" >&2; exit 2 ;;
    esac
}
safe "$TARGET"

PROFILE="(version 1)
(allow default)
(deny process-exec*)
(allow process-exec* (subpath \"$TARGET\"))"

[ -r "$RECORD" ] || { echo "core-runner: the record is missing: $RECORD" >&2; exit 2; }
while IFS= read -r line || [ -n "$line" ]; do
    case "$line" in '' | '#'*) continue ;; esac
    in_system=0
    for folder in $SYSTEM_FOLDERS; do
        case "$line" in "$folder" | "$folder"/*) in_system=1 ;; esac
    done
    if [ "$in_system" -eq 0 ]; then
        echo "core-runner: the record names $line, which is not in a folder of the system's tools" >&2
        exit 2
    fi
    safe "$line"
    PROFILE="$PROFILE
(allow process-exec* (subpath \"$line\"))"
done <"$RECORD"

# The developer tools (git's stub starts the real git there) and the toolchain (two tests start cargo).
for extra in "${FRAMESMITH_CORE_DEVTOOLS:-}" "${FRAMESMITH_CORE_TOOLCHAIN:-}"; do
    [ -n "$extra" ] || continue
    extra=$(cd "$extra" && pwd -P) || { echo "core-runner: no folder: $extra" >&2; exit 2; }
    safe "$extra"
    PROFILE="$PROFILE
(allow process-exec* (subpath \"$extra\"))"
done

exec /usr/bin/env PATH="$SYSTEM_PATH" /usr/bin/sandbox-exec -p "$PROFILE" "$@"

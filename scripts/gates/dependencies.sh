#!/usr/bin/env bash
# The outside crates Framesmith depends on, and the record that must name them (CLAUDE.md rule 11).
# Behaviour: docs/dev/gates.md, section 6.
#
#   scripts/gates/dependencies.sh list           name<TAB>version<TAB>licence of every outside crate of Cargo.lock
#   scripts/gates/dependencies.sh compare FILE   each outside crate whose name is not in FILE in backticks
#
# `compare` exits 0 when every outside crate is named, 1 when one is not, 2 on wrong usage.
set -u

root=$(cd "$(dirname "$0")/../.." && pwd)
cd "$root" || exit 2

# Every outside crate of the lock file, for every system and every feature, build and test
# dependencies included, as the manifest of each states its licence. `cargo metadata` prints one
# line of JSON in which every package starts with its name, version, id and licence, in that
# order; the packages of this workspace have an id that begins with `path+`.
list() {
    cargo metadata --format-version 1 --offline --locked |
        grep -o '{"name":"[^"]*","version":"[^"]*","id":"[^"]*","license":\(null\|"[^"]*"\)' |
        grep -v '"id":"path+' |
        sed -e 's/^{"name":"\([^"]*\)","version":"\([^"]*\)","id":"[^"]*","license":/\1\t\2\t/' -e 's/\t"\(.*\)"$/\t\1/' -e 's/\tnull$/\t(no licence stated)/' |
        sort -u
}

case "${1:-}" in
    list)
        out=$(list) || exit 1
        printf '%s\n' "$out"
        found=$(printf '%s\n' "$out" | wc -l | tr -d ' ')
        locked=$(grep -c '^source = ' Cargo.lock)
        echo "outside crates listed: $found; packages with a source in Cargo.lock: $locked" >&2
        [ "$found" -eq "$locked" ] || { echo "the two counts differ: the list is not the lock file's" >&2; exit 1; }
        ;;
    compare)
        [ $# -eq 2 ] && [ -f "$2" ] || { echo "usage: scripts/gates/dependencies.sh compare FILE" >&2; exit 2; }
        missing=0
        total=0
        while IFS="$(printf '\t')" read -r name version licence; do
            total=$((total + 1))
            if ! grep -qF "\`$name\`" "$2"; then
                printf 'not named in %s: %s %s (%s)\n' "$2" "$name" "$version" "$licence"
                missing=$((missing + 1))
            fi
        done < <(list)
        echo "$((total - missing)) of $total outside crates are named in $2 (as \`name\` in backticks); $missing are not"
        [ "$missing" -eq 0 ]
        ;;
    *)
        echo "usage: scripts/gates/dependencies.sh list | compare FILE" >&2
        exit 2
        ;;
esac

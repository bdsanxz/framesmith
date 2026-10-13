#!/usr/bin/env bash
# How Framesmith depends on frx7: the gate that refuses a committed path override and a pin moved
# without its record, and the commands for the local override and the hosted runner's deploy key.
# Behaviour: docs/dev/frx7.md.
#
#   scripts/gates/frx7.sh check               the gate "frx7 pin": looks at what is committed (HEAD)
#   scripts/gates/frx7.sh pin                 prints the pinned revision and the remote, from Cargo.toml
#   scripts/gates/frx7.sh override [FOLDER]   writes .cargo/config.toml: build against a local frx7
#   scripts/gates/frx7.sh drop-override       removes that file again
#   scripts/gates/frx7.sh deploy-key          hosted runner: makes the private remote reachable (env FRX7_DEPLOY_KEY)
#
# `check` exits 0 when nothing is refused, 1 when anything is, 2 on wrong usage. Written for the bash
# 3.2 that macOS ships.
set -u

ROOT=$(cd "$(dirname "$0")/../.." && pwd) || exit 2
cd "$ROOT" || exit 2

REMOTE="https://github.com/bdsanxz/frx7"
RECORD="docs/dev/frx7.md"
MARK="# frx7 local override, written by scripts/gates/frx7.sh override. Not committed (docs/dev/frx7.md, section 4)."
# The SSH host key GitHub publishes for github.com (the ssh_keys of https://api.github.com/meta, read on 2026-10-14).
GITHUB_HOST_KEY="github.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIOMqqnkVzrm0SdG6UOoqKLsabgH5C9okWi0dh2l9GKJl"

# --- reading what is committed --------------------------------------------------------------------

# The Cargo.toml files of a commit.
manifests_at() { git ls-tree -r --name-only "$1" | grep -E '(^|/)Cargo\.toml$'; }

# The frx7 entries of a manifest on standard input: line<TAB>remote<TAB>rev<TAB>branch-or-tag
ENTRY_AWK='
/^[[:space:]]*#/ { next }
/git[[:space:]]*=[[:space:]]*"[^"]*bdsanxz\/frx7[^"]*"/ {
    url = $0
    sub(/^.*git[[:space:]]*=[[:space:]]*"/, "", url)
    sub(/".*$/, "", url)
    rev = ""
    if (match($0, /rev[[:space:]]*=[[:space:]]*"[^"]*"/)) {
        rev = substr($0, RSTART, RLENGTH)
        sub(/^rev[[:space:]]*=[[:space:]]*"/, "", rev)
        sub(/"$/, "", rev)
    }
    other = ($0 ~ /(^|[ ,{])(branch|tag)[[:space:]]*=/) ? "yes" : "no"
    printf "%d\t%s\t%s\t%s\n", NR, url, rev, other
}'

# Every frx7 entry of the manifests of a commit: manifest<TAB>line<TAB>remote<TAB>rev<TAB>branch-or-tag
entries_at() {
    local manifest
    manifests_at "$1" | while IFS= read -r manifest; do
        git show "$1:$manifest" 2>/dev/null | awk "$ENTRY_AWK" | sed "s|^|$manifest	|"
    done
}

# The revisions the root manifest of a commit pins, one line each, or "none".
pin_at() {
    local revs
    revs=$(git show "$1:Cargo.toml" 2>/dev/null | awk "$ENTRY_AWK" | cut -f3 | sed '/^$/d' | sort -u)
    if [ -z "$revs" ]; then echo none; else echo "$revs"; fi
}

# Overrides in a manifest on standard input: line<TAB>what
OVERRIDE_AWK='
/^[[:space:]]*#/ { next }
/^\[/ {
    header = $0
    inpatch = (header ~ /^\[(patch|replace)/)
    if (header ~ /^\[replace/) print NR "\t[replace] table"
    if (header ~ /^\[patch/ && header ~ /bdsanxz\/frx7/) print NR "\tpatch of frx7: " header
    next
}
inpatch && /path[[:space:]]*=/ { print NR "\tpath in " header ": " $0; next }
/path[[:space:]]*=[[:space:]]*"(\/|\.\.|[A-Za-z]:)/ { print NR "\tpath that leaves the repository: " $0 }'

# Overrides in a cargo configuration file on standard input: line<TAB>what
CONFIG_AWK='
/^[[:space:]]*#/ { next }
/^\[patch/ { print NR "\t" $0; next }
/^[[:space:]]*paths[[:space:]]*=/ { print NR "\t" $0; next }
/^[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*=.*path[[:space:]]*=/ { print NR "\t" $0; next }
/^[[:space:]]*path[[:space:]]*=/ { print NR "\t" $0 }'

# The packages of a Cargo.lock on standard input: name<TAB>source ("" for none)
LOCK_AWK='
function flush() { if (name != "") print name "\t" source; name = ""; source = "" }
/^\[\[package\]\]/ { flush(); next }
/^name = / { name = $3; gsub(/"/, "", name) }
/^source = / { source = $3; gsub(/"/, "", source) }
END { flush() }'

# The rows of the history in docs/dev/frx7.md on standard input: old<TAB>new<TAB>reason
RECORD_AWK='
function trim(s) { gsub(/^[ \t]+|[ \t]+$/, "", s); gsub(/`/, "", s); return s }
/^## / { inside = 0 }
/^### The pin.s history/ { inside = 1; next }
inside && /^\|/ {
    n = split($0, f, "|")
    old = trim(f[3]); new = trim(f[4]); reason = trim(f[5])
    if (old == "Old revision" || old ~ /^-+$/) next
    print old "\t" new "\t" reason
}'

record_at() { git show "$1:$RECORD" 2>/dev/null | awk "$RECORD_AWK"; }

is_revision() { printf '%s' "$1" | grep -Eq '^[0-9a-f]{40}$'; }

# --- the gate ---------------------------------------------------------------------------------------

FOUND=0
refuse() {
    echo "refused: $1"
    FOUND=$((FOUND + 1))
}

short() { printf '%s' "$1" | cut -c1-12; }

cmd_check() {
    if ! git rev-parse --git-dir >/dev/null 2>&1; then
        refuse "this is not a git repository, so what is committed cannot be read"
        return 1
    fi
    if ! git rev-parse --verify -q HEAD >/dev/null; then
        refuse "there is no commit yet, so what is committed cannot be read"
        return 1
    fi

    local manifests members manifest line url rev other what
    manifests=$(manifests_at HEAD)
    members=$(for manifest in $manifests; do
        git show "HEAD:$manifest" | awk '/^\[package\]/ { p = 1; next } /^\[/ { p = 0 } p && /^name[[:space:]]*=/ { n = $0; sub(/^[^"]*"/, "", n); sub(/".*$/, "", n); print n }'
    done | sort -u)

    # The pin: the frx7 entries of the manifests.
    local entries revs
    entries=$(entries_at HEAD)
    if [ -z "$entries" ]; then
        refuse "no Cargo.toml pins frx7: the root Cargo.toml names no crate of $REMOTE at a revision (docs/dev/frx7.md, section 1)"
    fi
    # An entry with no rev has an empty field, which read would lose (a tab is blank space to it and
    # the fields after it would move up one place); a dash stands in for it.
    local tab
    tab=$(printf '\t')
    entries=$(printf '%s\n' "$entries" | sed "s/$tab$tab/$tab-$tab/")
    while IFS="$tab" read -r manifest line url rev other; do
        [ -n "$manifest" ] || continue
        [ "$manifest" = "Cargo.toml" ] || refuse "$manifest line $line names frx7 itself; only the root Cargo.toml does, a crate takes it as { workspace = true }"
        [ "$url" = "$REMOTE" ] || refuse "$manifest line $line names the remote \`$url\`; frx7 is spelled \`$REMOTE\`"
        if [ "$rev" = "-" ]; then
            refuse "$manifest line $line pins frx7 without a full revision (rev = \"<40 hex digits>\"): it has no rev"
        else
            is_revision "$rev" || refuse "$manifest line $line pins frx7 without a full revision (rev = \"<40 hex digits>\"): \`$rev\`"
        fi
        [ "$other" = "no" ] || refuse "$manifest line $line names a branch or a tag of frx7; only a full revision does not move"
    done <<EOF
$entries
EOF
    revs=$(printf '%s\n' "$entries" | cut -f4 | grep -v '^-$' | sort -u | sed '/^$/d')
    local pin=""
    if [ "$(printf '%s\n' "$revs" | sed '/^$/d' | wc -l | tr -d ' ')" -gt 1 ]; then
        refuse "frx7 is pinned at more than one revision: $(printf '%s' "$revs" | tr '\n' ' ')"
    else
        pin=$revs
    fi

    # A committed override.
    local overrides=0
    for manifest in $manifests; do
        while IFS="$(printf '\t')" read -r line what; do
            [ -n "$line" ] || continue
            overrides=$((overrides + 1))
            refuse "$manifest line $line commits an override of a dependency: $what"
        done <<EOF
$(git show "HEAD:$manifest" | awk "$OVERRIDE_AWK")
EOF
    done
    local configs
    configs=$(git ls-tree -r --name-only HEAD | grep -E '(^|/)\.cargo/config(\.toml)?$')
    local config
    for config in $configs; do
        while IFS="$(printf '\t')" read -r line what; do
            [ -n "$line" ] || continue
            overrides=$((overrides + 1))
            refuse "$config line $line commits a path override: $what"
        done <<EOF
$(git show "HEAD:$config" | awk "$CONFIG_AWK")
EOF
    done

    # The lock file.
    local frx7_in_lock=0 name source
    if git cat-file -e HEAD:Cargo.lock 2>/dev/null; then
        while IFS="$(printf '\t')" read -r name source; do
            [ -n "$name" ] || continue
            if [ -z "$source" ]; then
                printf '%s\n' "$members" | grep -qx "$name" ||
                    { overrides=$((overrides + 1)); refuse "Cargo.lock holds the package \`$name\` with no source, and it is no crate of this workspace: a path override stood in place when the lock was written"; }
            else
                case "$source" in
                    *bdsanxz/frx7*)
                        frx7_in_lock=$((frx7_in_lock + 1))
                        if [ -n "$pin" ] && [ "$source" != "git+$REMOTE?rev=$pin#$pin" ]; then
                            refuse "Cargo.lock takes frx7's \`$name\` from \`$source\`; the pin is \`$pin\` of \`$REMOTE\`"
                        fi
                        ;;
                esac
            fi
        done <<EOF
$(git show HEAD:Cargo.lock | awk "$LOCK_AWK")
EOF
        if [ -n "$entries" ] && [ "$frx7_in_lock" -eq 0 ]; then
            refuse "Cargo.lock holds no package of frx7 at the pin although the manifests ask for some; build once and commit the lock"
        fi
    else
        refuse "Cargo.lock is not committed"
    fi

    # The record.
    local rows count=0 previous="none" row_old row_new row_reason last_new=""
    rows=$(record_at HEAD)
    if [ -z "$rows" ]; then
        refuse "$RECORD holds no history of the pin (the table under \"The pin's history\")"
    fi
    while IFS="$(printf '\t')" read -r row_old row_new row_reason; do
        [ -n "$row_new" ] || continue
        count=$((count + 1))
        [ "$row_old" = "none" ] || is_revision "$row_old" || refuse "$RECORD, row $count: the old revision \`$row_old\` is neither \`none\` nor a full revision"
        is_revision "$row_new" || refuse "$RECORD, row $count: the new revision \`$row_new\` is not a full revision"
        [ "$row_old" = "$previous" ] || refuse "$RECORD, row $count: the old revision \`$row_old\` is not the new revision of the row before (\`$previous\`)"
        [ -n "$row_reason" ] || refuse "$RECORD, row $count: the move to \`$(short "$row_new")\` has no reason"
        previous=$row_new
        last_new=$row_new
    done <<EOF
$rows
EOF
    if [ -n "$pin" ] && [ -n "$last_new" ] && [ "$last_new" != "$pin" ]; then
        refuse "$RECORD ends with the revision \`$last_new\`; the pin is \`$pin\`"
    fi

    # The history: every commit that moved the pin carried its record and nothing else.
    local moves=0 commit parent old new changed subject
    if [ "$(git rev-parse --is-shallow-repository 2>/dev/null)" = "true" ]; then
        refuse "this clone is shallow, so the commits that moved the pin cannot be read; fetch the whole history"
    else
        for commit in $(git log --no-merges --reverse --format=%H -G'bdsanxz/frx7' -- Cargo.toml); do
            parent=$(git rev-parse -q --verify "$commit^" 2>/dev/null || true)
            if [ -n "$parent" ]; then old=$(pin_at "$parent" | tr '\n' ' ' | sed 's/ *$//'); else old=none; fi
            new=$(pin_at "$commit" | tr '\n' ' ' | sed 's/ *$//')
            [ "$old" != "$new" ] || continue
            subject=$(git log -1 --format=%s "$commit")
            moves=$((moves + 1))
            if [ "$old" = "none" ]; then
                if ! record_at "$commit" | awk -F'\t' -v n="$new" '$1 == "none" && $2 == n && $3 != "" { found = 1 } END { exit !found }'; then
                    refuse "commit $(short "$commit") ($subject) sets the pin to \`$new\` without the row \`none\` to that revision in $RECORD"
                fi
                continue
            fi
            changed=$(git diff-tree --no-commit-id --name-only -r "$commit" | grep -vE '^(Cargo\.toml|Cargo\.lock|docs/dev/frx7\.md)$' | tr '\n' ' ' | sed 's/ *$//')
            if [ -n "$changed" ]; then
                refuse "commit $(short "$commit") ($subject) moves the pin from \`$old\` to \`$new\` and also changes $changed; moving the pin is a change of its own"
            fi
            local before after
            before=$(record_at "$parent" | awk -F'\t' -v o="$old" -v n="$new" '$1 == o && $2 == n { c++ } END { print c + 0 }')
            after=$(record_at "$commit" | awk -F'\t' -v o="$old" -v n="$new" '$1 == o && $2 == n && $3 != "" { c++ } END { print c + 0 }')
            if [ "$after" -le "$before" ]; then
                refuse "commit $(short "$commit") ($subject) moves the pin from \`$old\` to \`$new\` without adding that row, with its reason, to $RECORD"
            fi
        done
    fi

    # What was looked at, and the working tree.
    local n_manifests n_configs
    n_manifests=$(printf '%s\n' "$manifests" | sed '/^$/d' | wc -l | tr -d ' ')
    n_configs=$(printf '%s\n' "$configs" | sed '/^$/d' | wc -l | tr -d ' ')
    if [ "$FOUND" -eq 0 ]; then
        echo "ok: frx7 is pinned at $pin of $REMOTE, in $(printf '%s\n' "$entries" | sed '/^$/d' | wc -l | tr -d ' ') entries of the root Cargo.toml"
        echo "ok: no override is committed (looked at $n_manifests Cargo.toml files, $n_configs tracked .cargo configuration files and Cargo.lock of HEAD $(short "$(git rev-parse HEAD)"))"
        echo "ok: $RECORD ends with the pin, in $count rows; $moves commit(s) changed the pin and each carries its row"
    fi
    if [ -f .cargo/config.toml ] && grep -q 'bdsanxz/frx7' .cargo/config.toml; then
        echo "note: a local override of frx7 stands in .cargo/config.toml (not committed); this gate looks at the commit"
    fi
    [ "$FOUND" -eq 0 ]
}

# --- the pin, as the working tree says ----------------------------------------------------------------

cmd_pin() {
    local line
    line=$(awk "$ENTRY_AWK" Cargo.toml | head -n 1)
    [ -n "$line" ] || { echo "Cargo.toml pins no crate of frx7" >&2; return 1; }
    printf 'revision\t%s\nremote\t%s\n' "$(printf '%s' "$line" | cut -f3)" "$(printf '%s' "$line" | cut -f2)"
}

# --- the local override ---------------------------------------------------------------------------------

# The crates of frx7 a local folder must provide: those the root Cargo.toml names and, from the
# manifests of that folder, those they need.
needed_crates() {
    local folder="$1" queue seen next name
    queue=$(grep -E 'git[[:space:]]*=[[:space:]]*"[^"]*bdsanxz/frx7' Cargo.toml | sed -e 's/^[[:space:]]*//' -e 's/[[:space:]]*=.*$//')
    seen=""
    while [ -n "$queue" ]; do
        next=""
        for name in $queue; do
            case " $seen " in *" $name "*) continue ;; esac
            [ -f "$folder/crates/$name/Cargo.toml" ] || continue
            seen="$seen $name"
            next="$next $(awk '/^\[/ { d = ($0 ~ /dependencies\]$/ && $0 !~ /dev-dependencies/); next } d && /^[A-Za-z0-9_-]+[[:space:]]*=/ { print $1 }' "$folder/crates/$name/Cargo.toml" | tr '\n' ' ')"
        done
        queue=$next
    done
    printf '%s\n' $seen
}

cmd_override() {
    local folder="${1:-$ROOT/../frx7}" crates name
    if [ ! -f "$folder/Cargo.toml" ] || ! grep -q '^\[workspace\]' "$folder/Cargo.toml" || [ ! -d "$folder/crates" ]; then
        echo "$folder is not a folder of frx7 (no Cargo.toml with a [workspace] and no crates/); give the folder: scripts/gates/frx7.sh override /path/to/frx7" >&2
        return 2
    fi
    folder=$(cd "$folder" && pwd)
    if [ -e .cargo/config.toml ] && [ "$(head -n 1 .cargo/config.toml)" != "$MARK" ]; then
        echo ".cargo/config.toml exists and was not written by this command; it is left alone" >&2
        return 1
    fi
    crates=$(needed_crates "$folder")
    if [ -z "$crates" ]; then
        echo "none of the frx7 crates the root Cargo.toml names is in $folder/crates" >&2
        return 1
    fi
    mkdir -p .cargo || return 1
    {
        echo "$MARK"
        echo "[patch.\"$REMOTE\"]"
        for name in $crates; do
            echo "$name = { path = \"$folder/crates/$name\" }"
        done
    } >.cargo/config.toml
    echo "wrote .cargo/config.toml: frx7 is built from $folder ($(printf '%s' "$crates" | tr '\n' ' ' | sed 's/ *$//'))"
    echo "Cargo.lock will differ from the committed one while this stands; do not commit it (docs/dev/frx7.md, section 4)"
}

cmd_drop_override() {
    if [ ! -e .cargo/config.toml ]; then
        echo "no local override stands (.cargo/config.toml does not exist)"
        return 0
    fi
    if [ "$(head -n 1 .cargo/config.toml)" != "$MARK" ]; then
        echo ".cargo/config.toml was not written by this command; it is left alone" >&2
        return 1
    fi
    rm .cargo/config.toml || return 1
    echo "removed .cargo/config.toml: frx7 is taken from its pin again; the next build writes the pinned sources back to Cargo.lock"
}

# --- the hosted runner's deploy key ---------------------------------------------------------------------

cmd_deploy_key() {
    if [ -z "${FRX7_DEPLOY_KEY:-}" ]; then
        echo "the variable FRX7_DEPLOY_KEY is empty or not set: the Actions secret of that name (the private half of the read-only deploy key of bdsanxz/frx7) is missing, so $REMOTE cannot be fetched" >&2
        return 1
    fi
    if [ -z "${HOME:-}" ]; then
        echo "HOME is not set" >&2
        return 1
    fi
    local ssh_dir="$HOME/.ssh" key
    key="$ssh_dir/frx7_deploy_key"
    umask 077
    mkdir -p "$ssh_dir" || return 1
    printf '%s\n' "$FRX7_DEPLOY_KEY" >"$key" || return 1
    chmod 600 "$key"
    if ! ssh-keygen -y -f "$key" >/dev/null 2>&1; then
        rm -f "$key"
        echo "the content of FRX7_DEPLOY_KEY is not a private SSH key ssh can read; it was removed and is not shown" >&2
        return 1
    fi
    printf '%s\n' "$GITHUB_HOST_KEY" >"$ssh_dir/frx7_known_hosts"
    {
        echo
        echo "Host frx7-github"
        echo "    HostName github.com"
        echo "    User git"
        echo "    IdentityFile $key"
        echo "    IdentitiesOnly yes"
        echo "    UserKnownHostsFile $ssh_dir/frx7_known_hosts"
        echo "    StrictHostKeyChecking yes"
    } >>"$ssh_dir/config"
    chmod 600 "$ssh_dir/config"
    git config --global "url.git@frx7-github:bdsanxz/frx7.insteadOf" "$REMOTE" || return 1
    echo "frx7: $REMOTE is read through the read-only deploy key (git@frx7-github); build with CARGO_NET_GIT_FETCH_WITH_CLI=true"
}

case "${1:-}" in
    check) cmd_check ;;
    pin) cmd_pin ;;
    override) shift; cmd_override "$@" ;;
    drop-override) cmd_drop_override ;;
    deploy-key) cmd_deploy_key ;;
    *)
        echo "usage: scripts/gates/frx7.sh check | pin | override [FOLDER] | drop-override | deploy-key" >&2
        exit 2
        ;;
esac

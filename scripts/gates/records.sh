#!/usr/bin/env bash
# The records of passed gates (docs/dev/gates.md, section 14). Sourced by scripts/gates.sh, which sets ROOT, HOSTED,
# CRATES and TOOLCHAIN_LINE before it sources this file, and calls record_setup once, then record_decide and
# record_pass for each gate. Written for the bash 3.2 that macOS ships.

RECORDS="${CARGO_TARGET_DIR:-$ROOT/target}/gate-records"
RECORD_KEY=""
RECORD_REASON=""

# Sets RECORD_KEY to the key of the tree now checked out, or sets RECORD_KEY empty and RECORD_REASON to why no
# record is read or written for this run.
record_setup() {
    RECORD_KEY=""
    if [ "$HOSTED" -eq 1 ]; then
        RECORD_REASON="a hosted run takes nothing from its records"
        return 0
    fi
    if [ -n "$CRATES" ]; then
        RECORD_REASON="a run narrowed to crates with -p is not a pass of the workspace"
        return 0
    fi
    local tree pin
    tree=$(git -C "$ROOT" rev-parse --verify -q "HEAD^{tree}" 2>/dev/null) || {
        RECORD_REASON="there is no commit to key a record by"
        return 0
    }
    if [ -n "$(git -C "$ROOT" status --porcelain 2>/dev/null)" ]; then
        RECORD_REASON="the working tree has uncommitted changes"
        return 0
    fi
    pin=$(bash "$ROOT/scripts/gates/frx7.sh" pin 2>/dev/null) || {
        RECORD_REASON="the frx7 pin cannot be read"
        return 0
    }
    RECORD_KEY=$(printf 'tree %s\ntoolchain %s\npin %s\n' "$tree" "$TOOLCHAIN_LINE" "$pin")
}

# Prints whether the gate $1 is taken from its record ("taken from its record") or runs ("ran", with the reason),
# and returns 0 when it is taken.
record_decide() {
    local file="$RECORDS/$1"
    if [ -z "$RECORD_KEY" ]; then
        echo "ran: $RECORD_REASON"
        return 1
    fi
    if [ -f "$file" ] && [ "$(cat "$file")" = "$RECORD_KEY" ]; then
        echo "taken from its record: a pass of this gate is recorded for this tree"
        return 0
    fi
    echo "ran: no pass of this gate is recorded for this tree"
    return 1
}

# Records the pass of the gate $1, when the tree checked out is still the tree the run started with: a gate that
# changed a tracked file passed for another tree, and is not recorded.
record_pass() {
    [ -n "$RECORD_KEY" ] || return 0
    local start="$RECORD_KEY"
    record_setup
    if [ "$RECORD_KEY" != "$start" ]; then
        RECORD_KEY="$start"
        echo "not recorded: the tree changed during the run"
        return 0
    fi
    RECORD_KEY="$start"
    mkdir -p "$RECORDS" || return 0
    printf '%s\n' "$RECORD_KEY" >"$RECORDS/$1"
    echo "recorded: this pass of the gate is kept for this tree"
}

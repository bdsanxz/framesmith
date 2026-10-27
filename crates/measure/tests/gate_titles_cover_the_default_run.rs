//! Every gate that the default run of `scripts/gates.sh` names has its title in the one table of titles of that script,
//! so that its summary line names it, whether the gate runs or is taken from its record (`docs/dev/gates.md`).

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

fn text(relative: &str) -> String {
    std::fs::read_to_string(root().join(relative)).unwrap_or_else(|why| panic!("{relative}: {why}"))
}

/// The gates of the default run, as the script's line that sets them names them.
fn default_gates(script: &str) -> Vec<String> {
    let line = script
        .lines()
        .find(|line| line.trim_start().starts_with("[ -z \"$WANTED\" ] && WANTED="))
        .expect("the script has no default list of gates");
    line.split("WANTED=\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the default list is not quoted")
        .split_whitespace()
        .map(str::to_string)
        .collect()
}

#[test]
fn every_gate_of_the_default_run_has_a_title_in_the_table() {
    let script = text("scripts/gates.sh");
    let gates = default_gates(&script);
    assert!(gates.len() >= 2, "the default run names {gates:?}");
    let missing: Vec<&String> = gates
        .iter()
        .filter(|gate| !script.lines().any(|line| line.starts_with(&format!("GATE_TITLE_{gate}=\""))))
        .collect();
    assert!(missing.is_empty(), "no title in the table of scripts/gates.sh for {missing:?}");
}

#[test]
fn the_system_free_gate_is_titled_as_its_summary_names_it() {
    let script = text("scripts/gates.sh");
    assert!(
        script.lines().any(|line| line == "GATE_TITLE_free=\"system-free crates\""),
        "the table of scripts/gates.sh does not title the system-free gate"
    );
}

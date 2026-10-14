//! The gate "frx7 pin": a committed path override is refused and named, frx7 is pinned at one
//! revision that the lock file and the record agree with, and a pin is moved in a change of its
//! own that carries its record. See `docs/dev/frx7.md`, section 6.

mod common;

use common::{lock, manifest, record, Repo, PIN_A, PIN_B, REMOTE};

#[test]
fn a_repository_pinned_at_one_revision_with_its_record_passes() {
    let repo = Repo::pinned("passes");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text.contains(&format!("ok: frx7 is pinned at {PIN_A} of {REMOTE}")),
        "{}",
        run.text
    );
    assert!(run.text.contains("ok: no override is committed"), "{}", run.text);
    assert!(!run.text.contains("refused"), "{}", run.text);
}

#[test]
fn a_committed_patch_by_a_path_is_refused_and_names_the_override() {
    let repo = Repo::pinned("patch-in-manifest");
    let mut text = manifest(PIN_A);
    text.push_str(&format!(
        "\n[patch.\"{REMOTE}\"]\nbase = {{ path = \"/home/me/frx7/crates/base\" }}\n"
    ));
    repo.write("Cargo.toml", &text);
    repo.commit("Use the local frx7");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains(&format!("refused: Cargo.toml line 11 commits an override of a dependency: path in [patch.\"{REMOTE}\"]: base = {{ path = \"/home/me/frx7/crates/base\" }}")),
        "{}",
        run.text
    );
    assert!(run.text.contains("patch of frx7"), "{}", run.text);
    assert!(!run.text.contains("ok:"), "a refusal prints no ok line: {}", run.text);
}

#[test]
fn a_dependency_by_a_path_that_leaves_the_repository_is_refused() {
    for (name, path) in [("path-up", "../frx7/crates/base"), ("path-absolute", "/Users/me/frx7/crates/base")] {
        let repo = Repo::pinned(name);
        repo.write(
            "crates/a/Cargo.toml",
            &format!("[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n[dependencies]\nbase = {{ path = \"{path}\" }}\n"),
        );
        repo.commit("Take base from a folder");
        let run = repo.script(&["check"]);
        assert_eq!(run.code, 1, "{}", run.text);
        assert!(
            run.text.contains(&format!("refused: crates/a/Cargo.toml line 6 commits an override of a dependency: path that leaves the repository: base = {{ path = \"{path}\" }}")),
            "{}",
            run.text
        );
    }
}

#[test]
fn a_committed_cargo_configuration_with_a_patch_is_refused() {
    let repo = Repo::pinned("patch-in-config");
    repo.write(
        ".cargo/config.toml",
        &format!("[patch.\"{REMOTE}\"]\nbase = {{ path = \"/home/me/frx7/crates/base\" }}\n"),
    );
    repo.commit("Commit the override");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains(&format!(
            "refused: .cargo/config.toml line 1 commits a path override: [patch.\"{REMOTE}\"]"
        )),
        "{}",
        run.text
    );
    assert!(
        run.text
            .contains(".cargo/config.toml line 2 commits a path override: base = { path = "),
        "{}",
        run.text
    );
}

#[test]
fn a_lock_file_written_while_an_override_stood_is_refused() {
    let repo = Repo::pinned("lock-with-override");
    // What Cargo writes when a patch by a path replaces the pinned crates: no source.
    let sourceless = lock(PIN_A).replace(&format!("source = \"git+{REMOTE}?rev={PIN_A}#{PIN_A}\"\n"), "");
    repo.write("Cargo.lock", &sourceless);
    repo.commit("Update the lock");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains("refused: Cargo.lock holds the package `base` with no source, and it is no crate of this workspace: a path override stood in place when the lock was written"),
        "{}",
        run.text
    );
    assert!(run.text.contains("the package `mp4` with no source"), "{}", run.text);
    assert!(
        run.text.contains("Cargo.lock holds no package of frx7 at the pin"),
        "a lock with no frx7 package at all is refused as well: {}",
        run.text
    );
}

#[test]
fn a_lock_file_that_takes_frx7_from_another_revision_is_refused() {
    let repo = Repo::pinned("lock-other-revision");
    repo.write("Cargo.lock", &lock(PIN_B));
    repo.commit("Update the lock");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains(&format!(
            "refused: Cargo.lock takes frx7's `base` from `git+{REMOTE}?rev={PIN_B}#{PIN_B}`; the pin is `{PIN_A}` of `{REMOTE}`"
        )),
        "{}",
        run.text
    );
}

#[test]
fn an_entry_with_a_tag_and_no_rev_is_refused_saying_that_it_has_no_rev() {
    let repo = Repo::pinned("pins-tag-without-rev");
    let wrong = format!(
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/a\"]\n\n[workspace.dependencies]\na = {{ path = \"crates/a\" }}\n\
         base = {{ git = \"{REMOTE}\", rev = \"{PIN_A}\" }}\n\
         format = {{ git = \"{REMOTE}\", tag = \"v1\" }}\n"
    );
    repo.write("Cargo.toml", &wrong);
    repo.commit("Pin by a tag");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text
            .contains("refused: Cargo.toml line 8 pins frx7 without a full revision (rev = \"<40 hex digits>\"): it has no rev"),
        "{}",
        run.text
    );
    assert!(
        run.text.contains("Cargo.toml line 8 names a branch or a tag of frx7"),
        "{}",
        run.text
    );
    // The value in the refusal is the rev that is not there, and never the word that came after it.
    assert!(!run.text.contains("`yes`") && !run.text.contains("`no`"), "{}", run.text);
    // An entry whose rev is there but short is still named by its value.
    let short = wrong.replace("tag = \"v1\"", "rev = \"23b07bc\"");
    repo.write("Cargo.toml", &short);
    repo.commit("Pin by a short name");
    let run = repo.script(&["check"]);
    assert!(
        run.text
            .contains("Cargo.toml line 8 pins frx7 without a full revision (rev = \"<40 hex digits>\"): `23b07bc`"),
        "{}",
        run.text
    );
}

#[test]
fn two_revisions_a_branch_a_short_name_and_another_remote_are_refused() {
    let repo = Repo::pinned("pins-wrong");
    let wrong = format!(
        "[workspace]\nresolver = \"2\"\nmembers = [\"crates/a\"]\n\n[workspace.dependencies]\na = {{ path = \"crates/a\" }}\n\
         base = {{ git = \"{REMOTE}\", rev = \"{PIN_A}\" }}\n\
         mp4 = {{ git = \"{REMOTE}\", rev = \"{PIN_B}\" }}\n\
         format = {{ git = \"{REMOTE}\", branch = \"main\" }}\n\
         probe = {{ git = \"{REMOTE}\", rev = \"23b07bc\" }}\n\
         audio = {{ git = \"git@github.com:bdsanxz/frx7.git\", rev = \"{PIN_A}\" }}\n"
    );
    repo.write("Cargo.toml", &wrong);
    repo.commit("Pin carelessly");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text
            .contains(&format!("refused: frx7 is pinned at more than one revision: {PIN_A} {PIN_B}")),
        "{}",
        run.text
    );
    assert!(
        run.text.contains("Cargo.toml line 9 pins frx7 without a full revision"),
        "{}",
        run.text
    );
    assert!(
        run.text.contains("Cargo.toml line 9 names a branch or a tag of frx7"),
        "{}",
        run.text
    );
    assert!(
        run.text
            .contains("Cargo.toml line 10 pins frx7 without a full revision (rev = \"<40 hex digits>\"): `23b07bc`"),
        "{}",
        run.text
    );
    assert!(
        run.text
            .contains("Cargo.toml line 11 names the remote `git@github.com:bdsanxz/frx7.git`"),
        "{}",
        run.text
    );
}

#[test]
fn a_member_crate_that_names_frx7_itself_is_refused() {
    let repo = Repo::pinned("member-names-frx7");
    repo.write(
        "crates/a/Cargo.toml",
        &format!("[package]\nname = \"a\"\nversion = \"0.1.0\"\n\n[dependencies]\nbase = {{ git = \"{REMOTE}\", rev = \"{PIN_A}\" }}\n"),
    );
    repo.commit("Name frx7 in a member");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text
            .contains("refused: crates/a/Cargo.toml line 6 names frx7 itself; only the root Cargo.toml does"),
        "{}",
        run.text
    );
}

#[test]
fn a_repository_that_pins_nothing_is_refused() {
    let repo = Repo::pinned("pins-nothing");
    repo.write("Cargo.toml", "[workspace]\nmembers = [\"crates/a\"]\n");
    repo.commit("Drop the pin");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(run.text.contains("refused: no Cargo.toml pins frx7"), "{}", run.text);
}

#[test]
fn a_record_that_does_not_end_with_the_pin_is_refused() {
    let repo = Repo::pinned("record-mismatch");
    repo.write("docs/dev/frx7.md", &record(&[("none", PIN_B, "First pin.")]));
    repo.commit("Edit the record");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains(&format!(
            "refused: docs/dev/frx7.md ends with the revision `{PIN_B}`; the pin is `{PIN_A}`"
        )),
        "{}",
        run.text
    );
}

#[test]
fn a_record_whose_rows_do_not_chain_or_have_no_reason_is_refused() {
    let repo = Repo::pinned("record-broken");
    repo.write(
        "docs/dev/frx7.md",
        &record(&[("none", PIN_B, "First pin."), (PIN_B, "short", "x"), (PIN_B, PIN_A, "")]),
    );
    repo.commit("Edit the record");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains("row 2: the new revision `short` is not a full revision"),
        "{}",
        run.text
    );
    assert!(
        run.text.contains(&format!(
            "row 3: the old revision `{PIN_B}` is not the new revision of the row before (`short`)"
        )),
        "{}",
        run.text
    );
    assert!(run.text.contains("row 3: the move to `111111111111` has no reason"), "{}", run.text);
}

#[test]
fn an_override_in_the_working_tree_only_does_not_stop_the_gate() {
    let repo = Repo::pinned("override-not-committed");
    // What `scripts/gates/frx7.sh override` writes, and the lock Cargo then rewrites.
    repo.write(".cargo/.gitignore", "config.toml\n");
    repo.commit("Ignore the override");
    repo.write(
        ".cargo/config.toml",
        &format!("[patch.\"{REMOTE}\"]\nbase = {{ path = \"/home/me/frx7/crates/base\" }}\n"),
    );
    let sourceless = lock(PIN_A).replace(&format!("source = \"git+{REMOTE}?rev={PIN_A}#{PIN_A}\"\n"), "");
    repo.write("Cargo.lock", &sourceless);
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(
        run.text
            .contains("note: a local override of frx7 stands in .cargo/config.toml (not committed)"),
        "{}",
        run.text
    );
    assert!(
        repo.git(&["status", "--porcelain"]).contains("Cargo.lock"),
        "the lock differs from the commit"
    );
    assert!(
        !repo.git(&["status", "--porcelain"]).contains("config.toml"),
        "the override is ignored by git"
    );
}

/// Moves the pin from `PIN_A` to `PIN_B` in the manifest and the lock, as one commit does.
fn move_pin(repo: &Repo) {
    repo.write("Cargo.toml", &manifest(PIN_B));
    repo.write("Cargo.lock", &lock(PIN_B));
}

#[test]
fn a_pin_moved_in_a_change_of_its_own_with_its_row_passes() {
    let repo = Repo::pinned("move-passes");
    move_pin(&repo);
    repo.write(
        "docs/dev/frx7.md",
        &record(&[
            ("none", PIN_A, "First pin."),
            (PIN_A, PIN_B, "frx7 gained a reader for sound files."),
        ]),
    );
    repo.commit("Move the pin of frx7");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert!(run.text.contains(&format!("ok: frx7 is pinned at {PIN_B}")), "{}", run.text);
    assert!(
        run.text
            .contains("ok: docs/dev/frx7.md ends with the pin, in 2 rows; 2 commit(s) changed the pin and each carries its row"),
        "{}",
        run.text
    );
}

#[test]
fn a_pin_moved_without_its_row_in_the_record_is_refused() {
    let repo = Repo::pinned("move-without-row");
    move_pin(&repo);
    repo.commit("Move the pin of frx7");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains(&format!(
            "moves the pin from `{PIN_A}` to `{PIN_B}` without adding that row, with its reason, to docs/dev/frx7.md"
        )),
        "{}",
        run.text
    );
    assert!(
        run.text.contains("(Move the pin of frx7)"),
        "the commit is named by its subject: {}",
        run.text
    );
    assert!(
        run.text
            .contains(&format!("docs/dev/frx7.md ends with the revision `{PIN_A}`; the pin is `{PIN_B}`")),
        "{}",
        run.text
    );
}

#[test]
fn a_pin_moved_in_a_commit_that_also_changes_other_files_is_refused() {
    let repo = Repo::pinned("move-with-other-files");
    move_pin(&repo);
    repo.write("crates/a/src/lib.rs", "pub fn changed() {}\n");
    repo.write(
        "docs/dev/frx7.md",
        &record(&[
            ("none", PIN_A, "First pin."),
            (PIN_A, PIN_B, "frx7 gained a reader for sound files."),
        ]),
    );
    repo.commit("Move the pin and change the code");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text.contains(&format!(
            "moves the pin from `{PIN_A}` to `{PIN_B}` and also changes crates/a/src/lib.rs; moving the pin is a change of its own"
        )),
        "{}",
        run.text
    );
}

#[test]
fn a_pin_first_set_without_its_row_is_refused() {
    let repo = Repo::empty("first-pin-without-row");
    repo.write("Cargo.toml", &manifest(PIN_A));
    repo.write("Cargo.lock", &lock(PIN_A));
    repo.write("crates/a/Cargo.toml", "[package]\nname = \"a\"\nversion = \"0.1.0\"\n");
    repo.write("docs/dev/frx7.md", &record(&[]));
    repo.commit("Pin frx7");
    let run = repo.script(&["check"]);
    assert_eq!(run.code, 1, "{}", run.text);
    assert!(
        run.text
            .contains(&format!("sets the pin to `{PIN_A}` without the row `none` to that revision")),
        "{}",
        run.text
    );
}

#[test]
fn a_clone_without_the_whole_history_is_refused_rather_than_passed() {
    let repo = Repo::pinned("shallow");
    repo.write("README.md", "a second commit\n");
    repo.commit("Another change");
    let clone = repo.dir.parent().expect("a parent").join("shallow-clone");
    let _ = std::fs::remove_dir_all(&clone);
    repo.git(&[
        "clone",
        "-q",
        "--depth",
        "1",
        &format!("file://{}", repo.dir.display()),
        clone.to_str().expect("a path"),
    ]);
    let output = common::isolate_from_runner(&mut std::process::Command::new("bash"))
        .arg("scripts/gates/frx7.sh")
        .arg("check")
        .current_dir(&clone)
        .output()
        .expect("the script is run");
    let text = String::from_utf8_lossy(&output.stdout);
    assert_eq!(output.status.code(), Some(1), "{text}");
    assert!(
        text.contains("refused: this clone is shallow, so the commits that moved the pin cannot be read"),
        "{text}"
    );
}

#[test]
fn the_pin_is_printed_from_the_manifest() {
    let repo = Repo::pinned("pin-printed");
    let run = repo.script(&["pin"]);
    assert_eq!(run.code, 0, "{}", run.text);
    assert_eq!(run.text, format!("revision\t{PIN_A}\nremote\t{REMOTE}\n"));
}

#[test]
fn wrong_usage_exits_with_two() {
    let repo = Repo::pinned("usage");
    assert_eq!(repo.script(&["frobnicate"]).code, 2);
    assert_eq!(repo.script(&[]).code, 2);
}

//! The gates run with git's repository variables cleared (`docs/dev/gates.md`, section 3): git runs the
//! pre-push hook with `GIT_DIR` set, an absolute path when the push is made from a linked worktree, and a test
//! that runs git in a folder of its own must still work on that folder and never on the repository pushed from.
//!
//! The repository pushed from is a scratch repository with a linked worktree. The gates' copy is run, narrowed
//! to the tests gate, with `GIT_DIR` and `GIT_WORK_TREE` set to that worktree as a hook would set them; the
//! scratch workspace's one test makes a repository in a folder of its own and commits to it. The scratch
//! repository's config, its `HEAD`s and its refs must be as they were before.

mod common;

use common::isolate_from_runner;
use common::scratch::Scratch;
use std::path::{Path, PathBuf};
use std::process::Command;

/// git in `dir`, with none of git's repository variables of the test's own run.
fn git(dir: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    isolate_from_runner(&mut command);
    for name in git_variables() {
        command.env_remove(name);
    }
    let output = command
        .current_dir(dir)
        .args(["-c", "user.name=Scratch", "-c", "user.email=scratch@example.invalid"])
        .args(args)
        .output()
        .expect("git is run");
    assert!(output.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&output.stderr));
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// The names of git's repository variables, from git's own list.
fn git_variables() -> Vec<String> {
    let output = Command::new("git")
        .args(["rev-parse", "--local-env-vars"])
        .output()
        .expect("git is run");
    String::from_utf8_lossy(&output.stdout).lines().map(str::to_string).collect()
}

/// What a test run against the wrong repository would change: its config, the `HEAD` of the main folder and of
/// the worktree, and every ref.
fn state(repository: &Path, worktree: &Path) -> (String, String, String, String) {
    (
        std::fs::read_to_string(repository.join(".git/config")).expect("the config is read"),
        git(repository, &["rev-parse", "HEAD"]),
        git(worktree, &["rev-parse", "HEAD"]),
        git(repository, &["for-each-ref"]),
    )
}

#[test]
fn a_test_that_runs_git_in_its_own_folder_leaves_the_repository_of_a_hook_from_a_worktree_unchanged() {
    let scratch = Scratch::new("git-environment");
    scratch.write(
        "crates/user/tests/git_in_a_folder.rs",
        "use std::process::Command;\n\n\
         fn git(dir: &std::path::Path, args: &[&str]) {\n    \
             let status = Command::new(\"git\").current_dir(dir)\n        \
             .args([\"-c\", \"user.name=Inner\", \"-c\", \"user.email=inner@example.invalid\"]).args(args).status().unwrap();\n    \
             assert!(status.success(), \"git {args:?}\");\n}\n\n\
         #[test]\nfn a_repository_is_made_and_committed_to_in_a_folder_of_its_own() {\n    \
             let dir = std::path::PathBuf::from(env!(\"CARGO_TARGET_TMPDIR\")).join(\"own-repository\");\n    \
             let _ = std::fs::remove_dir_all(&dir);\n    \
             std::fs::create_dir_all(&dir).unwrap();\n    \
             git(&dir, &[\"init\", \"-q\"]);\n    \
             git(&dir, &[\"config\", \"scratch.mark\", \"inner\"]);\n    \
             std::fs::write(dir.join(\"a.txt\"), \"a\").unwrap();\n    \
             git(&dir, &[\"add\", \"a.txt\"]);\n    \
             git(&dir, &[\"commit\", \"-q\", \"-m\", \"Inner commit\"]);\n}\n",
    );

    // The repository pushed from, with a linked worktree, outside the scratch workspace.
    let base: PathBuf = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("gate-scratch")
        .join("git-environment-pushed");
    let _ = std::fs::remove_dir_all(&base);
    let repository = base.join("repository");
    let worktree = base.join("worktree");
    std::fs::create_dir_all(&repository).expect("the folder is made");
    git(&repository, &["init", "-q", "-b", "main"]);
    std::fs::write(repository.join("README"), "pushed from\n").expect("the file is written");
    git(&repository, &["add", "README"]);
    git(&repository, &["commit", "-q", "-m", "First"]);
    git(&repository, &["worktree", "add", "-q", "-b", "work", worktree.to_str().unwrap()]);
    let git_dir = git(&worktree, &["rev-parse", "--absolute-git-dir"]).trim().to_string();
    let before = state(&repository, &worktree);

    let run = scratch.gates(
        &["--gate", "test"],
        &[("GIT_DIR", git_dir.as_str()), ("GIT_WORK_TREE", worktree.to_str().unwrap())],
    );

    let after = state(&repository, &worktree);
    assert_eq!(
        after.0, before.0,
        "the config of the repository pushed from was changed:\n{}",
        run.text
    );
    assert_eq!(after.1, before.1, "the HEAD of the repository pushed from moved:\n{}", run.text);
    assert_eq!(after.2, before.2, "the HEAD of its worktree moved:\n{}", run.text);
    assert_eq!(after.3, before.3, "its refs were changed:\n{}", run.text);
    assert_eq!(run.code, 3, "the narrowed run's part passed: {}", run.text);
    assert!(
        run.text
            .contains("test a_repository_is_made_and_committed_to_in_a_folder_of_its_own ... ok"),
        "{}",
        run.text
    );
}

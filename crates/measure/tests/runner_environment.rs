//! The helpers of the tests that start a script or a program of the repository take the hosted
//! runner's variables out of the child's environment, so that a failure made on purpose reaches no
//! file of the runner (`docs/dev/gates.md`, section 12).

mod common;

use common::{isolate_from_runner, Repo, RUNNER_VARIABLES};
use std::process::Command;

fn removed(command: &Command, name: &str) -> bool {
    command.get_envs().any(|(key, value)| key == name && value.is_none())
}

#[test]
fn the_runner_variables_are_the_marker_and_the_four_files_a_step_may_write_to() {
    assert_eq!(
        RUNNER_VARIABLES,
        [
            "GITHUB_ACTIONS",
            "GITHUB_STEP_SUMMARY",
            "GITHUB_OUTPUT",
            "GITHUB_ENV",
            "GITHUB_PATH"
        ]
    );
}

#[test]
fn a_command_made_isolated_has_each_runner_variable_removed() {
    let mut command = Command::new("bash");
    isolate_from_runner(&mut command);
    for name in RUNNER_VARIABLES {
        assert!(removed(&command, name), "{name} is still passed on");
    }
}

#[test]
fn the_repository_helper_that_starts_scripts_and_git_removes_each_runner_variable() {
    let repo = Repo::empty("runner-environment");
    for program in ["bash", "git"] {
        let command = repo.command(program);
        for name in RUNNER_VARIABLES {
            assert!(removed(&command, name), "{name} is passed on to {program}");
        }
    }
}

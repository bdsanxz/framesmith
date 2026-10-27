//! `framesmith init` opens no network connection (`docs/spec/init-command.md`, section 6). It runs to its end under a macOS
//! sandbox profile that denies every network operation, and a control shows that the same profile stops a program that
//! connects to a listener of this machine, which succeeds without the profile.
//!
//! A second sandbox profile cannot be applied inside the profile of the core suite (`docs/dev/gates.md`, section 10:
//! `sandbox_apply: Operation not permitted`), so there, where `scripts/gates/core-suite.sh` sets `FRAMESMITH_CORE_TARGET`,
//! the test is not run and prints why; the tests gate runs it in full on every run of the gates.

#![cfg(target_os = "macos")]

mod common;

use std::fs;
use std::net::TcpListener;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use common::{err, out, scratch};

const DENY_NETWORK: &str = "(version 1)\n(allow default)\n(deny network*)\n";
/// The same, but a network operation ends the process that makes it, with SIGKILL, before it can exit.
const KILL_NETWORK: &str = "(version 1)\n(allow default)\n(deny network* (with send-signal SIGKILL))\n";

/// Whether the test runs inside the profile of the core suite, where no second profile can be applied.
fn inside_the_core_profile() -> bool {
    if std::env::var_os("FRAMESMITH_CORE_TARGET").is_some() {
        println!("not run inside the core suite: a second sandbox profile cannot be applied there; the tests gate runs it");
        return true;
    }
    false
}

fn profile(folder: &Path) -> PathBuf {
    let path = folder.join("deny-network.sb");
    fs::write(&path, DENY_NETWORK).unwrap();
    path
}

/// Runs `program` with `args` in `dir`, under the sandbox profile at `profile` when one is given.
fn run(profile: Option<&Path>, dir: &Path, program: &str, args: &[&str]) -> Output {
    let mut command = match profile {
        Some(profile) => {
            let mut command = Command::new("/usr/bin/sandbox-exec");
            command.arg("-f").arg(profile).arg(program);
            command
        }
        None => Command::new(program),
    };
    command.args(args).current_dir(dir).output().unwrap()
}

#[test]
fn init_runs_to_its_end_under_a_profile_that_denies_every_network_operation() {
    if inside_the_core_profile() {
        return;
    }
    let cwd = scratch("init", "no-network");
    let profile = profile(&cwd);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let connect = ["-z", "127.0.0.1", port.as_str()];

    // The control: the connection succeeds without the profile, and the profile stops it.
    let open = run(None, &cwd, "/usr/bin/nc", &connect);
    assert_eq!(
        open.status.code(),
        Some(0),
        "the listener accepts a connection without the profile: {}",
        out(&open)
    );
    let denied = run(Some(&profile), &cwd, "/usr/bin/nc", &connect);
    assert_ne!(
        denied.status.code(),
        Some(0),
        "the profile stops a connection: {}{}",
        out(&denied),
        err(&denied)
    );

    // The command, under the same profile: it writes the starter and exits with 0.
    let init = run(Some(&profile), &cwd, env!("CARGO_BIN_EXE_framesmith"), &["init", "demo"]);
    assert_eq!(init.status.code(), Some(0), "init under the profile: {}{}", out(&init), err(&init));
    assert!(cwd.join("demo/composition.xml").is_file());
}

#[test]
fn init_completes_under_a_profile_that_ends_a_process_at_any_network_operation() {
    if inside_the_core_profile() {
        return;
    }
    let cwd = scratch("init", "network-killed");
    let profile = cwd.join("kill-network.sb");
    fs::write(&profile, KILL_NETWORK).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let connect = ["-z", "127.0.0.1", port.as_str()];

    // The control: a program that makes a connection is ended by the profile at that operation, so it never exits; without
    // the profile the same connection succeeds. A program that connected and went on after a failure would not survive it.
    let open = run(None, &cwd, "/usr/bin/nc", &connect);
    assert_eq!(
        open.status.code(),
        Some(0),
        "the listener accepts a connection without the profile: {}",
        out(&open)
    );
    let killed = run(Some(&profile), &cwd, "/usr/bin/nc", &connect);
    assert_eq!(
        killed.status.signal(),
        Some(9),
        "the profile ends a connecting process: {}{}",
        out(&killed),
        err(&killed)
    );

    // The command, under the profile that kills at a network operation: it still runs to its end and writes the starter.
    let init = run(Some(&profile), &cwd, env!("CARGO_BIN_EXE_framesmith"), &["init", "demo"]);
    assert_eq!(init.status.code(), Some(0), "init under the profile: {}{}", out(&init), err(&init));
    assert!(cwd.join("demo/composition.xml").is_file());
}

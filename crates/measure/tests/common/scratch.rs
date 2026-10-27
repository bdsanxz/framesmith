//! A small cargo workspace of its own, with a copy of `scripts/gates.sh` and of `scripts/gates/records.sh`
//! in it, and a way to run that copy cut off from the real runner: the tests of the gate script that need a
//! gate to pass or to fail on purpose use it (`docs/dev/gates.md`, sections 2 and 12).

use super::{isolate_from_runner, Run};
use std::path::{Path, PathBuf};
use std::process::Command;

pub struct Scratch {
    pub dir: PathBuf,
}

impl Scratch {
    /// A workspace made new in a folder named `name` under the tests' scratch folder. Its one crate,
    /// `user`, has no dependency, so that building it fetches nothing and needs no network.
    pub fn new(name: &str) -> Scratch {
        let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gate-scratch").join(name);
        let _ = std::fs::remove_dir_all(&dir);
        let scratch = Scratch { dir };
        scratch.write("Cargo.toml", "[workspace]\nresolver = \"2\"\nmembers = [\"crates/user\"]\n");
        scratch.write(
            "crates/user/Cargo.toml",
            "[package]\nname = \"user\"\nversion = \"0.1.0\"\nedition = \"2021\"\npublish = false\n",
        );
        scratch.write("crates/user/src/lib.rs", "//! A scratch crate that holds nothing.\n");
        let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates.sh");
        scratch.write("scripts/gates.sh", &std::fs::read_to_string(script).expect("the script is read"));
        let records = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/gates/records.sh");
        scratch.write(
            "scripts/gates/records.sh",
            &std::fs::read_to_string(records).expect("the record script is read"),
        );
        scratch
    }

    pub fn write(&self, relative: &str, content: &str) {
        let path = self.dir.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("the folder is made");
        std::fs::write(&path, content).expect("the file is written");
    }

    /// The command `program` in the workspace, cut off from the real runner (its variables are taken
    /// out) with cargo's home and target folders inside the workspace. The search path of the child
    /// is the folder of the toolchain that runs these tests (the folder of the cargo in `CARGO`)
    /// before the search path of the test, because the core suite runs with only the system's
    /// folders on its own (`docs/dev/gates.md`, section 10).
    pub fn command(&self, program: &str) -> Command {
        let mut command = Command::new(program);
        isolate_from_runner(&mut command);
        command
            .current_dir(&self.dir)
            .env("PATH", search_path_with_toolchain())
            .env("CARGO_HOME", self.dir.join("home"))
            .env("CARGO_TARGET_DIR", self.dir.join("target"))
            .env("CARGO_NET_OFFLINE", "true");
        command
    }

    /// Runs the workspace's copy of `scripts/gates.sh` with `args`. `env` is set after the runner's
    /// variables were taken out, so a test can put back the ones it wants a runner to have.
    pub fn gates(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = self.command("bash");
        command.arg("scripts/gates.sh").args(args);
        for (name, value) in env {
            command.env(name, value);
        }
        let output = command.output().expect("the script is run");
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        Run {
            code: output.status.code().unwrap_or(-1),
            text,
        }
    }
}

fn search_path_with_toolchain() -> std::ffi::OsString {
    let mut folders = Vec::new();
    if let Some(cargo) = std::env::var_os("CARGO") {
        if let Some(folder) = Path::new(&cargo).parent() {
            folders.push(folder.to_path_buf());
        }
    }
    folders.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
    std::env::join_paths(folders).expect("the search path is joined")
}

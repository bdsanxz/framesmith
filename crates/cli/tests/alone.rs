//! T053: with nothing to be found but a Chromium-based browser and `ffmpeg`, the binary runs `init`,
//! `check` and `render` in order, uses no network and installs nothing.
//!
//! Sprint 2, ticket T060: `preview` listens on the loopback address, so the binary now holds network code and
//! the Sprint 1 test that it holds none (`t053_c4`) is gone. What replaces it is a check of behaviour,
//! `s2_t060_c6_...`: with every use of the network denied by the operating system, `init`, `check` and
//! `render` give the same results as with it allowed.

mod common;

use common::*;
use measure::Ran;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// One test's machine: two tools folders that each hold a browser and `ffmpeg` and nothing else (in the
/// second the browser is started without its own process sandbox), an empty home folder, an empty
/// temporary folder and an empty applications folder.
struct Machine {
    root: PathBuf,
    tools: PathBuf,
    tools_unsandboxed: PathBuf,
    home: PathBuf,
    tmp: PathBuf,
    applications: PathBuf,
}

fn machine(test: &str) -> Machine {
    let root = scratch(test);
    let [tools, tools_unsandboxed, home, tmp, applications] = ["tools", "tools-unsandboxed", "home", "tmp", "applications"].map(|name| {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    });
    for (dir, flag) in [(&tools, ""), (&tools_unsandboxed, " --no-sandbox")] {
        std::os::unix::fs::symlink(ffmpeg(), dir.join("ffmpeg")).unwrap();
        live::script(&dir.join("chromium"), &format!("exec '{}'{flag} \"$@\"", browser().display()));
    }
    Machine {
        root,
        tools,
        tools_unsandboxed,
        home,
        tmp,
        applications,
    }
}

impl Machine {
    /// Runs `program` in the folder `dir` of the machine with an emptied environment: the search path is
    /// `tools` alone, and the home and temporary folders are the machine's own. So is the applications
    /// folder: a browser installed on the computer the test runs on is not part of the machine, and
    /// would otherwise be found before the one in `tools`.
    fn run(&self, tools: &Path, dir: &str, program: &Path, args: &[&str]) -> Ran {
        let dir = self.root.join(dir);
        std::fs::create_dir_all(&dir).unwrap();
        let output = Command::new(program)
            .args(args)
            .current_dir(dir)
            .env_clear()
            .env("PATH", tools)
            .env("HOME", &self.home)
            .env("TMPDIR", &self.tmp)
            .env("FRAMESMITH_DEBUG_APPLICATIONS", &self.applications)
            .stdin(Stdio::null())
            .output()
            .unwrap_or_else(|e| panic!("{} could not be run: {e}", program.display()));
        Ran {
            code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }
    }

    /// The three commands in order in `dir`; each must exit 0.
    fn three_commands(&self, tools: &Path, dir: &str, before: &[&str]) {
        for command in [&["init", "demo"][..], &["check", "demo"], &["render", "demo"]] {
            let (program, args) = match before.split_first() {
                Some((first, rest)) => (Path::new(first), [rest, &[BIN][..], command].concat()),
                None => (Path::new(BIN), command.to_vec()),
            };
            let ran = self.run(tools, dir, program, &args);
            assert_eq!(ran.code, 0, "{command:?}: {}", ran.all());
        }
    }
}

fn names(dir: &Path) -> BTreeSet<String> {
    std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

fn files(dir: &Path) -> BTreeSet<String> {
    tree(dir).keys().map(|path| path.to_string_lossy().into_owned()).collect()
}

/// One line per frame of the video: a hash of its decoded picture.
fn frame_hashes(video: &Path) -> Vec<String> {
    let ran = measure::run(
        &ffmpeg(),
        &["-v", "error", "-i", text(video), "-map", "0:v", "-f", "framemd5", "-"],
        Path::new("/"),
        &[],
    );
    assert_eq!(ran.code, 0, "{}", ran.all());
    ran.stdout
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| line.rsplit(',').next().unwrap().trim().to_string())
        .collect()
}

#[test]
fn t053_c1_c5_init_check_and_render_run_in_order_with_only_a_browser_and_ffmpeg_to_be_found() {
    let m = machine("t053_alone");
    m.three_commands(&m.tools, "work", &[]);

    let found = facts(&m.root.join("work/demo/out/demo.mp4"));
    assert_eq!((found.frames, found.width, found.height), (150, 1280, 720), "{found:?}");
    assert_eq!(found.codec, "h264");

    // Nothing but the project, its video and the log was written: no program was added to the tools,
    // nothing was left in the temporary folder, and the home folder holds only the log.
    assert_eq!(
        files(&m.root.join("work")),
        ["demo/index.html", "demo/out/demo.mp4", "demo/style.css"].map(String::from).into()
    );
    assert_eq!(names(&m.tools), ["chromium", "ffmpeg"].map(String::from).into());
    assert_eq!(names(&m.tmp), BTreeSet::new());
    assert_eq!(names(&m.applications), BTreeSet::new());
    let in_home = files(&m.home);
    assert!(
        !in_home.is_empty() && in_home.iter().all(|path| path.starts_with("Library/Logs/framesmith/")),
        "{in_home:?}"
    );
}

/// The program that runs another inside a profile of the operating system's sandbox.
#[cfg(target_os = "macos")]
const SANDBOX: &str = "/usr/bin/sandbox-exec";

/// The sandbox profile that denies every use of the network. One thing is let through: sockets that are
/// files in the system's per-user temporary folders. A full browser build makes one there to find another
/// copy of itself on the same profile, and stops when it cannot. Such a socket reaches nothing outside
/// this machine, and the system's own services (name lookup among them) do not listen there.
#[cfg(target_os = "macos")]
fn deny_network() -> String {
    let files = "(subpath \"/private/var/folders\")";
    format!(
        "(version 1)(allow default)(deny network*)\
         (allow network-bind network-inbound (local unix-socket {files}))\
         (allow network-outbound (remote unix-socket {files}))"
    )
}

/// The sandbox does cut the network off, this machine's own loopback address included.
#[cfg(target_os = "macos")]
fn assert_sandbox_cuts_loopback(m: &Machine, deny: &str) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let reach = ["/usr/bin/nc", "-z", "-G", "2", "127.0.0.1", &port];
    let outside = m.run(Path::new("/usr/bin"), "control", Path::new(reach[0]), &reach[1..]);
    assert_eq!(outside.code, 0, "{}", outside.all());
    let inside = m.run(
        Path::new("/usr/bin"),
        "control",
        Path::new(SANDBOX),
        &[&["-p", deny][..], &reach[..]].concat(),
    );
    assert_ne!(inside.code, 0, "{}", inside.all());
}

#[cfg(target_os = "macos")]
#[test]
fn t053_c3_with_every_use_of_the_network_denied_the_three_commands_give_the_same_video() {
    let m = machine("t053_offline");
    let deny = deny_network();

    assert_sandbox_cuts_loopback(&m, &deny);

    // A browser cannot set up its own process sandbox inside another sandbox, so in the sandboxed run
    // the machine's browser is started without it.
    m.three_commands(&m.tools, "online", &[]);
    m.three_commands(&m.tools_unsandboxed, "offline", &[SANDBOX, "-p", &deny]);

    let starter = |dir: &str| -> Vec<_> {
        tree(&m.root.join(dir).join("demo"))
            .into_iter()
            .filter(|(path, _)| !path.starts_with("out"))
            .collect()
    };
    assert_eq!(starter("online"), starter("offline"));
    let online = frame_hashes(&m.root.join("online/demo/out/demo.mp4"));
    let offline = frame_hashes(&m.root.join("offline/demo/out/demo.mp4"));
    assert_eq!(online.len(), 150);
    assert_eq!(online, offline);
}

/// What a run gave that does not change from one run to the next: the exit code and the words on both
/// streams, without the lines of progress that depend on timing and without the time a render took.
fn results(ran: &Ran) -> (i32, String, String) {
    let kept = |text: &str| -> String {
        text.lines()
            .filter(|line| !line.starts_with("capture "))
            .map(
                |line| match line.strip_prefix("Rendered ").and_then(|_| line.rsplit_once(", in ")) {
                    Some((head, _)) => head,
                    None => line,
                },
            )
            .collect::<Vec<_>>()
            .join("\n")
    };
    (ran.code, kept(&ran.stdout), kept(&ran.stderr))
}

/// Writes the projects the commands are run on into `dir` of the machine: a small one to render, one with
/// an error, and one that uses something Framesmith refuses.
#[cfg(target_os = "macos")]
fn projects(m: &Machine, dir: &str) {
    let root = m.root.join(dir);
    std::fs::create_dir_all(&root).unwrap();
    let ground = "<div style=\"position:absolute;inset:0;background:#c33\"></div>";
    write(
        &root,
        "tiny/index.html",
        page("data-fs-size=\"64x36\" data-fs-duration=\"0.2\"", "", ground, ""),
    );
    write(
        &root,
        "broken/index.html",
        page("data-fs-size=\"65x36\" data-fs-duration=\"0.2\"", "", "", ""),
    );
    copy_tree(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/unsupported/fs-u004-ratio-rate"),
        &root.join("refused"),
    );
}

#[cfg(target_os = "macos")]
#[test]
fn s2_t060_c6_with_every_use_of_the_network_denied_init_check_and_render_give_the_same_results_as_with_it_allowed() {
    // `preview` listens on the loopback address, so the binary holds network code. The three commands do
    // not use it, and this is the test of that: the same commands, run with the operating system denying
    // every use of the network and with it allowed, give the same exit codes, the same words, the same
    // files and the same frames.
    let m = machine("s2_t060_c6");
    let deny = deny_network();
    assert_sandbox_cuts_loopback(&m, &deny);
    projects(&m, "online");
    projects(&m, "offline");

    // The commands, in order; the codes they give are fixed here so that the two runs cannot agree by both failing.
    let commands: [(&[&str], i32); 8] = [
        (&["init", "demo"], 0),
        (&["check", "demo"], 0),
        (&["check", "tiny"], 0),
        (&["render", "tiny"], 0),
        (&["check", "broken"], 1),
        (&["render", "broken"], 1),
        (&["check", "refused"], 2),
        (&["render", "refused"], 2),
    ];
    let run_all = |tools: &Path, dir: &str, before: &[&str]| -> Vec<Ran> {
        commands
            .iter()
            .map(|(command, code)| {
                let (program, args) = match before.split_first() {
                    Some((first, rest)) => (Path::new(first), [rest, &[BIN][..], command].concat()),
                    None => (Path::new(BIN), command.to_vec()),
                };
                let ran = m.run(tools, dir, program, &args);
                assert_eq!(ran.code, *code, "{command:?} in {dir}: {}", ran.all());
                ran
            })
            .collect()
    };
    // A browser cannot set up its own process sandbox inside another sandbox, so in the sandboxed run
    // the machine's browser is started without it.
    let online = run_all(&m.tools, "online", &[]);
    let offline = run_all(&m.tools_unsandboxed, "offline", &[SANDBOX, "-p", &deny]);
    for (((command, _), allowed), denied) in commands.iter().zip(&online).zip(&offline) {
        assert_eq!(results(allowed), results(denied), "{command:?}");
    }
    // The words themselves are not empty: the comparison compared something.
    assert!(online[3].stdout.contains("Rendered 6 frames, 64x36, 30 fps"), "{}", online[3].all());
    assert!(online[6].stdout.contains("FS-U004"), "{}", online[6].all());

    // The same files: the starter, the projects, and the frames of the video.
    let written = |dir: &str| -> Vec<_> {
        tree(&m.root.join(dir))
            .into_iter()
            .filter(|(path, _)| path.extension().is_none_or(|extension| extension != "mp4"))
            .collect()
    };
    assert_eq!(written("online"), written("offline"));
    let (allowed, denied) = (
        frame_hashes(&m.root.join("online/tiny/out/tiny.mp4")),
        frame_hashes(&m.root.join("offline/tiny/out/tiny.mp4")),
    );
    assert_eq!(allowed.len(), 6);
    assert_eq!(allowed, denied);

    // The control: the one command that needs the network of its machine does not get it. With the
    // network denied `preview` cannot listen, and says so.
    let preview = m.run(
        &m.tools_unsandboxed,
        "offline",
        Path::new(SANDBOX),
        &["-p", &deny, BIN, "preview", "demo"],
    );
    assert_eq!(preview.code, 1, "{}", preview.all());
    assert!(preview.stderr.contains("FS-P001"), "{}", preview.all());
    assert_eq!(preview.stdout, "");
}

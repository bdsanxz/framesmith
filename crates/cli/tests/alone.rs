//! T053: with nothing to be found but a Chromium-based browser and `ffmpeg`, the binary runs `init`,
//! `check` and `render` in order, uses no network and installs nothing.

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

#[cfg(target_os = "macos")]
#[test]
fn t053_c3_with_every_use_of_the_network_denied_the_three_commands_give_the_same_video() {
    const SANDBOX: &str = "/usr/bin/sandbox-exec";
    let m = machine("t053_offline");
    // Every use of the network is denied. One thing is let through: sockets that are files in the
    // system's per-user temporary folders. A full browser build makes one there to find another copy
    // of itself on the same profile, and stops when it cannot. Such a socket reaches nothing outside
    // this machine, and the system's own services (name lookup among them) do not listen there.
    let files = "(subpath \"/private/var/folders\")";
    let deny = format!(
        "(version 1)(allow default)(deny network*)\
         (allow network-bind network-inbound (local unix-socket {files}))\
         (allow network-outbound (remote unix-socket {files}))"
    );

    // The sandbox does cut the network off, this machine's own loopback address included.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let reach = ["/usr/bin/nc", "-z", "-G", "2", "127.0.0.1", &port];
    let outside = m.run(Path::new("/usr/bin"), "control", Path::new(reach[0]), &reach[1..]);
    assert_eq!(outside.code, 0, "{}", outside.all());
    let inside = m.run(
        Path::new("/usr/bin"),
        "control",
        Path::new(SANDBOX),
        &[&["-p", &deny][..], &reach[..]].concat(),
    );
    assert_ne!(inside.code, 0, "{}", inside.all());

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

#[test]
fn t053_c4_the_binary_calls_none_of_the_system_s_network_functions() {
    let nm = measure::tool("nm").expect("nm is needed for this test");
    let ran = measure::run(&nm, &["-u", BIN], Path::new("/"), &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    // An imported name as the linker writes it: `_socket` on macOS, `socket@GLIBC_2.2.5` elsewhere.
    let imported: BTreeSet<&str> = ran
        .stdout
        .lines()
        .filter_map(|line| line.split_whitespace().last())
        .map(|name| name.split('@').next().unwrap().trim_start_matches('_'))
        .collect();
    assert!(
        imported.len() > 50 && imported.contains("write"),
        "the list of imported names looks wrong: {imported:?}"
    );
    let network = [
        "socket",
        "connect",
        "bind",
        "listen",
        "accept",
        "getaddrinfo",
        "gethostbyname",
        "sendto",
        "sendmsg",
        "recvfrom",
        "recvmsg",
    ];
    let used: Vec<&&str> = network.iter().filter(|name| imported.contains(**name)).collect();
    assert!(used.is_empty(), "the binary imports {used:?}");
}

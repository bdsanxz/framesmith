//! T060: `framesmith preview <project>` starts a preview on the loopback address, prints its address with the
//! composition's facts, keeps running, and a browser opened at the address shows frame 0 of the composition,
//! at its declared proportions and scaled to fit the window. The sixth criterion (the network denied) is
//! tested in `alone.rs`, where the Sprint 1 test it replaces was.

mod common;
mod preview_common;

use common::*;
use measure::Rgb;
use preview_common::*;
use serde_json::Value;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

const GROUND: Rgb = [0x10, 0x18, 0x20];
const TOP_LEFT: Rgb = [0xe6, 0x39, 0x46];
const TOP_RIGHT: Rgb = [0x2e, 0xcc, 0x71];
const BOTTOM_LEFT: Rgb = [0x34, 0x98, 0xdb];
const BOTTOM_RIGHT: Rgb = [0xf1, 0xc4, 0x0f];
const SLIDE: Rgb = [0xff, 0x7f, 0x11];
const LATE: Rgb = [0xff, 0x00, 0xff];
const WHITE: Rgb = [0xff, 0xff, 0xff];

/// The composition of `fixtures/preview/frame-zero`.
const WIDTH: f64 = 640.0;
const HEIGHT: f64 = 360.0;

fn frame_zero(test: &str) -> PathBuf {
    fixture(test, "preview/frame-zero")
}

fn start(project: &Path) -> Preview {
    Preview::start(project.parent().unwrap(), &["preview", text(project)], &[])
}

// ---------------------------------------------------------------------------------------------------
// Criterion 1: one line with the address and the facts, and the command keeps running.

#[test]
fn s2_t060_c1_preview_prints_one_line_with_the_address_and_the_compositions_facts_and_keeps_running() {
    let project = frame_zero("s2_t060_c1_line");
    let mut preview = start(&project);
    assert_eq!(
        preview.line,
        format!("Preview at http://127.0.0.1:{}/  frame-zero  640x360  30 fps  3 s", preview.port)
    );
    assert_eq!(preview.address, format!("http://127.0.0.1:{}/", preview.port));
    // It keeps running, and still answers, after a while.
    std::thread::sleep(Duration::from_millis(1500));
    assert!(preview.running(), "the command ended: {}", preview.errors());
    assert_eq!(preview.get("/").status, 200);
    let pid = preview.pid();
    let ran = preview.stop();
    // Nothing else was printed on standard output but the line of the stop (ticket T061), and nothing on
    // standard error.
    assert_eq!(ran.stdout, "Preview stopped\n", "{}", ran.all());
    assert_eq!(ran.stderr, "", "{}", ran.all());
    assert!(!live::alive(pid), "the command is still running after it was stopped");
}

#[test]
fn s2_t060_c1_the_line_names_the_size_the_rate_and_the_length_of_another_composition() {
    // The rate and the length of this project are worked out, not written; the line states the values.
    let project = fixture("s2_t060_c1_other", "title-card-unwritten");
    let facts = framesmith(&["info", text(&project), "--json"]);
    let info = document(&facts);
    let (width, height, fps) = (
        info["width"].as_u64().unwrap(),
        info["height"].as_u64().unwrap(),
        info["fps"].as_u64().unwrap(),
    );
    let name = info["composition"].as_str().unwrap().to_string();
    let length = info["duration"].to_string();
    let preview = start(&project);
    assert_eq!(
        preview.line,
        format!("Preview at {}  {name}  {width}x{height}  {fps} fps  {length} s", preview.address)
    );
}

#[test]
fn s2_t060_c1_the_run_log_tells_the_start_in_order_under_one_run_id() {
    let project = frame_zero("s2_t060_c1_log");
    let logs = scratch("s2_t060_c1_log_folder");
    let env = [("FRAMESMITH_LOG_DIR", logs.as_os_str()), ("FRAMESMITH_LOG", OsStr::new("info"))];
    let preview = Preview::start(project.parent().unwrap(), &["preview", text(&project)], &env);
    let port = preview.port;
    let _ = preview.stop();
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    let position = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line with {needle:?} in\n{log}"))
    };
    let started = position("command started");
    let listening = position("preview listening");
    let running = position("preview running");
    assert!(started < listening && listening < running, "{log}");
    assert!(
        lines[started].contains("command=\"preview\"") && lines[started].contains(&format!("project={}", project.display())),
        "{log}"
    );
    assert!(lines[listening].contains(&format!("port={port}")), "{log}");
    assert!(lines[running].contains(&format!("address=http://127.0.0.1:{port}/")), "{log}");
    let runs: std::collections::BTreeSet<&str> = lines.iter().filter_map(|line| line.split_whitespace().nth(3)).collect();
    assert_eq!(runs.len(), 1, "{runs:?} in\n{log}");
    assert!(
        lines.iter().all(|line| !line.contains(" ERROR ") && !line.contains(" WARN ")),
        "{log}"
    );
}

#[test]
fn s2_t060_c1_the_help_lists_the_command() {
    let ran = framesmith(&["--help"]);
    assert_eq!(ran.code, 0);
    assert!(ran.stdout.contains("framesmith preview <project>"), "{}", ran.stdout);
    // The Sprint 1 and 2 commands are all still listed.
    for command in [
        "init <folder>",
        "check <project>",
        "info <project>",
        "timeline <project>",
        "render <project>",
    ] {
        assert!(ran.stdout.contains(&format!("framesmith {command}")), "{}", ran.stdout);
    }
}

#[test]
fn s2_t060_c1_a_word_or_option_the_command_does_not_have_is_refused_and_nothing_starts() {
    let project = frame_zero("s2_t060_c1_words");
    for (words, said) in [
        (vec!["preview"], "`framesmith preview` needs the path of a project folder."),
        (vec!["preview", "--port", "5000"], "`framesmith preview` has no option --port."),
        (
            vec!["preview", text(&project), "extra"],
            "`framesmith preview` takes one project folder; extra is one word too many.",
        ),
    ] {
        let ran = run_in(project.parent().unwrap(), &words, &[]);
        assert_eq!(ran.code, 1, "{words:?}: {}", ran.all());
        assert_eq!(ran.stdout, "", "{words:?}");
        assert!(ran.stderr.starts_with("error FS-R004\n"), "{words:?}: {}", ran.stderr);
        assert!(ran.stderr.contains(said), "{words:?}: {}", ran.stderr);
    }
}

#[test]
fn s2_t060_x_a_project_that_does_not_pass_the_check_is_refused_as_render_refuses_it_and_nothing_listens() {
    // Until ticket T074 a project with a problem is not previewed: the words, the codes and the exit code
    // are those of `render`, which starts nothing either.
    let refused = fixture("s2_t060_x_refused", "unsupported/fs-u004-ratio-rate");
    let broken = project(
        "s2_t060_x_broken",
        "broken",
        &page("data-fs-size=\"641x360\" data-fs-duration=\"1\"", "", "", ""),
    );
    for (dir, code) in [(&refused, 2), (&broken, 1)] {
        let preview = run_in(dir.parent().unwrap(), &["preview", text(dir)], &[]);
        let render = run_in(dir.parent().unwrap(), &["render", text(dir)], &[]);
        assert_eq!(preview.code, code, "{}", preview.all());
        assert_eq!(preview.code, render.code);
        assert_eq!(preview.stderr, render.stderr);
        assert_eq!(preview.stdout, "", "{}", preview.all());
    }
}

// ---------------------------------------------------------------------------------------------------
// Criterion 2: a browser opened at the address shows frame 0, at the declared proportions, scaled to fit.

/// What a window of `width` by `height` must show of `frame-zero`, checked in the layout the page reports
/// and in the pixels of the screenshot.
fn assert_frame_zero_fits(window: &mut Window, folder: &Path, width: u32, height: u32) {
    let scale = (f64::from(width) / WIDTH).min(f64::from(height) / HEIGHT);
    window.resize(width, height);
    window.wait_scale(scale);
    let layout = window.layout();
    let number = |value: &Value, key: &str| value[key].as_f64().unwrap_or_else(|| panic!("{key} in {value}"));
    let about = |found: f64, wanted: f64, what: &str| {
        // A painted edge is blurred over a pixel on each side when the picture is scaled.
        assert!(
            (found - wanted).abs() <= 2.5,
            "{what}: {found} is not {wanted} in a window of {width}x{height}; layout {layout}"
        );
    };

    // The page says frame 0 is shown, in a window of the size asked for.
    assert_eq!(layout["state"], "shown", "{layout}");
    assert_eq!(layout["frame"], "0", "{layout}");
    assert_eq!(
        (number(&layout["window"], "width"), number(&layout["window"], "height")),
        (f64::from(width), f64::from(height))
    );

    // Declared proportions: the picture is the composition's width by its height, times one factor.
    let picture = &layout["picture"];
    let (pw, ph) = (number(picture, "width"), number(picture, "height"));
    about(pw, WIDTH * scale, "picture width");
    about(ph, HEIGHT * scale, "picture height");
    assert!((pw / ph - WIDTH / HEIGHT).abs() < 0.01, "proportions {} in {layout}", pw / ph);
    // The frame is the composition at its declared size, scaled by that factor and nothing else.
    let composition = &layout["composition"];
    about(number(composition, "width"), WIDTH * scale, "frame width");
    about(number(composition, "height"), HEIGHT * scale, "frame height");
    // Not cut off: the picture lies inside the room, and is centred in it.
    let stage = &layout["stage"];
    let (left, top) = (
        number(picture, "left") - number(stage, "left"),
        number(picture, "top") - number(stage, "top"),
    );
    assert!(left >= -0.01 && top >= -0.01, "{layout}");
    assert!(
        left + pw <= number(stage, "width") + 0.01 && top + ph <= number(stage, "height") + 0.01,
        "{layout}"
    );
    about(left, (number(stage, "width") - pw) / 2.0, "left margin");
    about(top, (number(stage, "height") - ph) / 2.0, "top margin");
    // And it is as large as it can be: it touches the room on one axis.
    assert!(
        (number(stage, "width") - pw).abs() <= 1.5 || (number(stage, "height") - ph).abs() <= 1.5,
        "the picture does not fill one side of the room: {layout}"
    );

    // The pixels agree with the layout.
    let shot = window.screenshot();
    let picture_frame = pixels(&shot, &folder.join(format!("shot-{width}x{height}")));
    assert_eq!((picture_frame.width, picture_frame.height), (width, height));
    let (left, top, right, bottom) = (left, top, left + pw - 1.0, top + ph - 1.0);
    let ground = picture_frame
        .bounds(GROUND, 6)
        .expect("the composition's background is not on screen");
    about(f64::from(ground.left), left, "left edge of the composition");
    about(f64::from(ground.top), top, "top edge of the composition");
    about(f64::from(ground.right), right, "right edge of the composition");
    about(f64::from(ground.bottom), bottom, "bottom edge of the composition");
    // All four corners of the composition are on screen: a marker sits in each, 24 pixels square.
    let marker = 24.0 * scale;
    for (colour, at_right, at_bottom) in [
        (TOP_LEFT, false, false),
        (TOP_RIGHT, true, false),
        (BOTTOM_LEFT, false, true),
        (BOTTOM_RIGHT, true, true),
    ] {
        let found = picture_frame
            .bounds(colour, 6)
            .unwrap_or_else(|| panic!("the marker {colour:?} is not on screen at {width}x{height}"));
        let (edge_x, edge_y) = (if at_right { right } else { left }, if at_bottom { bottom } else { top });
        about(
            f64::from(if at_right { found.right } else { found.left }),
            edge_x,
            "marker, horizontal edge",
        );
        about(
            f64::from(if at_bottom { found.bottom } else { found.top }),
            edge_y,
            "marker, vertical edge",
        );
        about(f64::from(found.width()), marker, "marker width");
        about(f64::from(found.height()), marker, "marker height");
    }
    // Frame 0 itself: the slide starts 100 pixels to the right of where it rests; the clip that starts at
    // 1 s is not on screen; the title, a clip that starts at 0, is.
    let slide = picture_frame.bounds(SLIDE, 6).expect("the slide is not on screen");
    about(f64::from(slide.left), left + (200.0 + 100.0) * scale, "slide, left edge at frame 0");
    about(f64::from(slide.top), top + 120.0 * scale, "slide, top edge");
    about(f64::from(slide.width()), 60.0 * scale, "slide width");
    assert_eq!(
        picture_frame.count(LATE, 30),
        0,
        "the clip that starts at 1 s is on screen at frame 0"
    );
    assert!(picture_frame.count(WHITE, 30) > 20, "the title is not on screen");
}

#[test]
fn s2_t060_c2_the_page_shows_frame_0_at_the_declared_proportions_scaled_to_fit_a_larger_and_a_smaller_window() {
    let project = frame_zero("s2_t060_c2_fit");
    let preview = start(&project);
    let folder = project.parent().unwrap().to_path_buf();
    let mut window = Window::open("s2_t060_c2_fit_browser", &preview.address, 1280, 720);
    window.wait_state("shown");
    for (width, height) in [
        (1280, 720), // larger, same proportions: the factor is 2
        (640, 360),  // the declared size: the factor is 1
        (320, 180),  // smaller, same proportions: the factor is a half
        (1000, 300), // wide: the height decides
        (300, 700),  // tall: the width decides
        (480, 270),  // the narrowest window the page is promised in
        (2560, 1000),
    ] {
        assert_frame_zero_fits(&mut window, &folder, width, height);
    }
}

#[test]
fn s2_t060_c2_at_scale_1_the_page_shows_the_picture_a_render_gives_for_frame_0() {
    let project = frame_zero("s2_t060_c2_render");
    let (ran, frames) = render_frames(&project, &[], &[]);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let rendered = &frames[0];
    assert_eq!((rendered.width, rendered.height), (640, 360));

    let preview = start(&project);
    let mut window = Window::open("s2_t060_c2_render_browser", &preview.address, 640, 360);
    window.wait_state("shown");
    window.wait_scale(1.0);
    let shot = pixels(&window.screenshot(), &project.parent().unwrap().join("shot"));
    assert_eq!((shot.width, shot.height), (640, 360));
    let similarity = measure::similarity(&shot, rendered).unwrap();
    assert!(
        similarity >= 0.99,
        "the preview's frame 0 and the render's frame 0 have a similarity of {similarity}"
    );
}

#[test]
fn s2_t060_c2_the_page_shows_no_picture_when_the_running_page_reports_a_problem() {
    // The project passes the check; the page finds a use of the browser's own animation when it runs.
    let project = fixture("s2_t060_c2_problem", "preview/runtime-animate");
    assert_eq!(run_in(project.parent().unwrap(), &["check", text(&project)], &[]).code, 0);
    let preview = start(&project);
    let mut window = Window::open("s2_t060_c2_problem_browser", &preview.address, 640, 360);
    window.wait_state("refused");
    let visible = window.eval("getComputedStyle(document.getElementById('composition')).visibility");
    let says = window
        .eval("document.getElementById('notice').innerText")
        .as_str()
        .unwrap()
        .to_string();
    let stage_shown = window.eval("getComputedStyle(document.getElementById('stage')).display");
    assert_eq!(visible, "hidden");
    assert_eq!(stage_shown, "none");
    assert!(
        says.contains("The preview cannot show this project.") && says.contains("FS-U016"),
        "{says}"
    );
    // The picture is not on screen: the composition's own colours are nowhere.
    let shot = pixels(&window.screenshot(), &project.parent().unwrap().join("shot"));
    assert_eq!(shot.count([0xe6, 0x39, 0x46], 6), 0);
    assert_eq!(shot.count([0x20, 0x28, 0x30], 6), 0);
}

// ---------------------------------------------------------------------------------------------------
// Criterion 3: the address answers on the loopback address and on no other address of the machine.

#[test]
fn s2_t060_c3_the_address_answers_on_the_loopback_address_and_on_no_other_address_of_the_machine() {
    let project = frame_zero("s2_t060_c3");
    let preview = start(&project);
    let port = preview.port;

    // It answers on the loopback address.
    let answer = reach(SocketAddr::from((Ipv4Addr::LOCALHOST, port))).unwrap();
    assert!(answer.starts_with("HTTP/1.1 200"), "{answer:?}");

    // The control: a listener on every address answers on the loopback address and on each address of
    // this machine's interfaces that is an IPv4 one, so the probe below does see an answer where there is one.
    let others = other_addresses();
    let wide = TcpListener::bind("0.0.0.0:0").unwrap();
    let wide_port = wide.local_addr().unwrap().port();
    let v4_count = others.iter().filter(|(_, address)| address.ip.is_ipv4()).count();
    let serving = std::thread::spawn(move || {
        for _ in 0..=v4_count {
            let Ok((mut stream, _)) = wide.accept() else { return };
            let _ = std::io::Write::write_all(&mut stream, b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
        }
    });
    let mut controls = vec![SocketAddr::from((Ipv4Addr::LOCALHOST, wide_port))];
    controls.extend(
        others
            .iter()
            .filter(|(_, address)| address.ip.is_ipv4())
            .map(|(_, address)| address.with_port(wide_port)),
    );
    for address in &controls {
        let answer = reach(*address).unwrap_or_else(|why| panic!("the control listener does not answer on {address}: {why}"));
        assert!(answer.starts_with("HTTP/1.1 200"), "{address}: {answer:?}");
    }
    serving.join().unwrap();

    // The preview's port answers on none of the other addresses: those of the interfaces, another
    // address of the loopback range, and the IPv6 loopback address.
    let mut tried: Vec<SocketAddr> = others.iter().map(|(_, address)| address.with_port(port)).collect();
    tried.push(SocketAddr::from(([127, 0, 0, 2], port)));
    tried.push(SocketAddr::from((std::net::Ipv6Addr::LOCALHOST, port)));
    for address in &tried {
        let result = reach(*address);
        assert!(result.is_err(), "the preview answers on {address}: {result:?}");
    }
    // After all that, it still answers on the loopback address.
    assert_eq!(preview.get("/").status, 200);
}

#[cfg(target_os = "macos")]
#[test]
fn s2_t060_c3_the_system_lists_the_preview_as_listening_on_127_0_0_1_only() {
    let lsof = Path::new("/usr/sbin/lsof");
    if !lsof.exists() {
        return;
    }
    let project = frame_zero("s2_t060_c3_lsof");
    let preview = start(&project);
    let ran = measure::run(
        lsof,
        &["-nP", "-a", "-p", &preview.pid().to_string(), "-iTCP", "-sTCP:LISTEN"],
        Path::new("/"),
        &[],
    );
    assert!(ran.stdout.contains(&format!("127.0.0.1:{}", preview.port)), "{}", ran.all());
    assert!(
        !ran.stdout.contains(&format!("*:{}", preview.port)) && !ran.stdout.contains("[::"),
        "{}",
        ran.all()
    );
}

// ---------------------------------------------------------------------------------------------------
// Criterion 4: the preview starts on a machine where no `ffmpeg` and no browser can be found.

#[test]
fn s2_t060_c4_preview_starts_on_a_machine_where_no_ffmpeg_and_no_browser_can_be_found() {
    let root = scratch("s2_t060_c4");
    let [tools, home, tmp, applications, logs] = ["tools", "home", "tmp", "applications", "logs"].map(|name| {
        let dir = root.join(name);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    });
    let project = fixture("s2_t060_c4_project", "preview/frame-zero");
    let env: Vec<(&str, &OsStr)> = vec![
        ("PATH", tools.as_os_str()),
        ("HOME", home.as_os_str()),
        ("TMPDIR", tmp.as_os_str()),
        ("FRAMESMITH_DEBUG_APPLICATIONS", applications.as_os_str()),
        ("FRAMESMITH_LOG_DIR", logs.as_os_str()),
    ];

    // The control: on this machine `render` finds neither, and says so.
    let render = std::process::Command::new(BIN)
        .args(["render", text(&project)])
        .current_dir(&root)
        .env_clear()
        .envs(env.iter().copied())
        .output()
        .unwrap();
    let said = String::from_utf8_lossy(&render.stderr);
    assert_eq!(render.status.code(), Some(1), "{said}");
    assert!(said.contains("FS-R001") && said.contains("FS-R003"), "{said}");

    // `preview` starts, prints its address and answers.
    let mut preview = Preview::start_bare(Path::new(BIN), &root, &["preview", text(&project)], &env);
    assert_eq!(
        preview.line,
        format!("Preview at http://127.0.0.1:{}/  frame-zero  640x360  30 fps  3 s", preview.port)
    );
    assert_eq!(preview.get("/").status, 200);
    let entry = preview.get("/index.html");
    assert_eq!(entry.status, 200);
    assert!(entry.text().contains("/__framesmith__/loader.js"));
    assert_eq!(&preview.get("/__framesmith__/runtime.wasm").body[..4], b"\0asm");
    assert!(preview.running());
    // It looked for nothing and started nothing: no program was started by it.
    assert_eq!(live::descendants(preview.pid()), Vec::<u32>::new());
    let ran = preview.stop();
    assert_eq!(ran.stderr, "", "{}", ran.all());
}

// ---------------------------------------------------------------------------------------------------
// Criterion 5: a project path that does not exist is an error, exit code 1, and nothing listens.

#[test]
fn s2_t060_c5_a_project_path_that_does_not_exist_is_an_error_with_exit_code_1() {
    let dir = scratch("s2_t060_c5");
    let missing = dir.join("no-such-folder");
    let ran = run_in(&dir, &["preview", text(&missing)], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert_eq!(ran.stdout, "", "{}", ran.all());
    assert_eq!(
        ran.stderr,
        format!(
            "error FS-E001 {shown}\n  The project folder {shown} does not exist.\n  fix: Give the path of a project folder. `framesmith init <folder>` creates one.\n\ncheck failed: 1 error, 0 unsupported, 0 unknown, 0 warnings\n",
            shown = missing.display()
        )
    );
    assert!(!missing.exists(), "the command made the folder");
    // A file is not a project folder either.
    let file = dir.join("a-file");
    std::fs::write(&file, "x").unwrap();
    let ran = run_in(&dir, &["preview", text(&file)], &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stderr.contains("is not a folder"), "{}", ran.all());
}

/// A profile for the operating system's sandbox that kills a program the moment it asks to listen on, or
/// bind, any network address.
#[cfg(target_os = "macos")]
const KILL_ON_BIND: &str = "(version 1)(allow default)(deny network-bind (with send-signal SIGKILL))";

#[cfg(target_os = "macos")]
#[test]
fn s2_t060_c5_the_command_never_asks_to_listen_when_the_project_path_does_not_exist() {
    let dir = scratch("s2_t060_c5_listen");
    let missing = dir.join("no-such-folder");
    let sandbox = Path::new("/usr/bin/sandbox-exec");
    // The control: with a project that exists, the same sandbox kills the command when it tries to listen.
    let project = frame_zero("s2_t060_c5_listen_project");
    let control = measure::run(sandbox, &["-p", KILL_ON_BIND, BIN, "preview", text(&project)], &dir, &[]);
    assert_eq!(
        control.code,
        128 + 9,
        "the sandbox did not stop a command that listens: {}",
        control.all()
    );
    // With a path that does not exist, the command ends on its own with the error: it never asked.
    let ran = measure::run(sandbox, &["-p", KILL_ON_BIND, BIN, "preview", text(&missing)], &dir, &[]);
    assert_eq!(ran.code, 1, "{}", ran.all());
    assert!(ran.stderr.contains("FS-E001"), "{}", ran.all());
}

// ---------------------------------------------------------------------------------------------------
// Criterion 7: the dependencies added for the preview are recorded.

/// The dependencies the preview crate has, and what is recorded for each: the crates of this workspace,
/// and `tracing` (MIT), which the commands used before the preview. A dependency added for the preview
/// that is not in this list is not recorded: add it to `PROGRESS.md` with its licence and purpose, and
/// then here.
const RECORDED_DEPENDENCIES: [&str; 5] = ["compile", "model", "protocol", "timebase", "tracing"];

#[test]
fn s2_t060_c7_the_preview_crate_has_no_dependency_that_is_not_recorded() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let manifest = std::fs::read_to_string(root.join("crates/server/Cargo.toml")).unwrap();
    let mut section = String::new();
    let mut names = Vec::new();
    for line in manifest.lines() {
        if let Some(heading) = line.trim().strip_prefix('[') {
            section = heading.trim_end_matches(']').to_string();
        } else if section.ends_with("dependencies") {
            if let Some((name, _)) = line.split_once('=') {
                assert_eq!(section, "dependencies", "a {section} of the preview crate is not recorded: {line}");
                names.push(name.trim().to_string());
            }
        }
    }
    names.sort();
    assert_eq!(names, RECORDED_DEPENDENCIES, "the dependencies of crates/server/Cargo.toml");

    // The lock file agrees: the package `server` depends on exactly these.
    let lock = std::fs::read_to_string(root.join("Cargo.lock")).unwrap();
    let block = lock
        .split("[[package]]")
        .find(|block| block.trim_start().starts_with("name = \"server\""))
        .expect("the package server is not in Cargo.lock");
    let mut locked: Vec<String> = block
        .lines()
        .skip_while(|line| !line.starts_with("dependencies"))
        .skip(1)
        .take_while(|line| line.trim() != "]")
        .map(|line| line.trim().trim_matches(|c| c == '"' || c == ',').to_string())
        .collect();
    locked.sort();
    assert_eq!(
        locked, RECORDED_DEPENDENCIES,
        "the dependencies of the package server in Cargo.lock"
    );
}

/// A stand-in between a browser and a preview that gives every answer through unchanged, except that the
/// page's `data-protocol` names another version. It is stopped when it goes out of scope.
struct Rewriting {
    port: u16,
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Rewriting {
    fn start(preview_port: u16, version: &str) -> Rewriting {
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        listener.set_nonblocking(true).unwrap();
        let port = listener.local_addr().unwrap().port();
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (stop, version) = (Arc::clone(&stop), version.to_string());
            std::thread::spawn(move || {
                while !stop.load(Ordering::SeqCst) {
                    let Ok((mut client, _)) = listener.accept() else {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    };
                    client.set_nonblocking(false).unwrap();
                    client.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                    let mut request = Vec::new();
                    let mut buffer = [0u8; 2048];
                    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
                        match client.read(&mut buffer) {
                            Ok(0) | Err(_) => break,
                            Ok(count) => request.extend_from_slice(&buffer[..count]),
                        }
                    }
                    let reply = parse_reply(&raw(preview_port, &request));
                    let mut body = reply.body.clone();
                    let marker = b"data-protocol=\"";
                    if let Some(at) = body.windows(marker.len()).position(|w| w == marker) {
                        let start = at + marker.len();
                        let end = start + body[start..].iter().position(|&b| b == b'"').unwrap();
                        body.splice(start..end, version.bytes());
                    }
                    let kind = reply.header("Content-Type").unwrap_or("application/octet-stream").to_string();
                    let head = format!(
                        "HTTP/1.1 {} X\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
                        reply.status,
                        body.len()
                    );
                    let _ = client.write_all(head.as_bytes());
                    let _ = client.write_all(&body);
                }
            })
        };
        Rewriting {
            port,
            stop,
            thread: Some(thread),
        }
    }
}

impl Drop for Rewriting {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[test]
fn s2_t060_c2_the_page_names_the_protocol_version_and_refuses_a_runtime_that_reports_another() {
    let project = frame_zero("s2_t060_c2_protocol");
    let preview = start(&project);
    // The page the server writes carries a version, and the runtime's report agrees with it: the picture is shown.
    let page = preview.get("/").text();
    assert!(page.contains("data-protocol=\"1\""), "{page}");
    let mut window = Window::open("s2_t060_c2_protocol_agree", &preview.address, 640, 360);
    window.wait_state("shown");
    assert_eq!(window.eval("document.body.dataset.protocol"), "1");
    drop(window);
    // The same page naming another version: the script holds no number of its own, so it compares the
    // runtime's report with the attribute and refuses, with the protocol line, and shows no picture.
    let other = Rewriting::start(preview.port, "999");
    let mut window = Window::open("s2_t060_c2_protocol_differ", &format!("http://127.0.0.1:{}/", other.port), 640, 360);
    window.wait_state("refused");
    assert_eq!(window.eval("document.body.dataset.protocol"), "999");
    let says = window
        .eval("document.getElementById('notice').innerText")
        .as_str()
        .unwrap()
        .to_string();
    assert!(
        says.contains("The preview cannot show this project.") && says.contains("protocol: the page speaks version 1"),
        "{says}"
    );
    assert_eq!(
        window.eval("getComputedStyle(document.getElementById('composition')).visibility"),
        "hidden"
    );
    assert_eq!(window.eval("document.body.dataset.frame || null"), Value::Null);
}

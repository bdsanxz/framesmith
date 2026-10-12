//! T061: a foreground `framesmith preview` that receives an interrupt or a terminate signal prints
//! `Preview stopped` and exits 0, its address no longer answers and its port is free at once, and no
//! program it started is left. Specified in `docs/spec/preview.md`, "Stopping".
//!
//! A new preview cannot be told which port to use before ticket T080, so "the same port can be used by a new
//! preview" is shown with a listener bound the way the preview binds (a plain listener on 127.0.0.1 and the
//! port), and by a new `framesmith preview` that starts at once after the stop.

mod common;
mod preview_common;

use common::*;
use measure::Ran;
use preview_common::*;
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::time::Duration;

fn frame_zero(test: &str) -> PathBuf {
    fixture(test, "preview/frame-zero")
}

fn start(project: &std::path::Path) -> Preview {
    Preview::start(project.parent().unwrap(), &["preview", text(project)], &[])
}

/// The two ways a preview is stopped, with their names.
const SIGNALS: [(&str, libc::c_int); 2] = [("interrupt", libc::SIGINT), ("terminate", libc::SIGTERM)];

/// What is left of a preview that was started, answered and stopped with a signal.
struct Stopped {
    ran: Ran,
    port: u16,
    pid: u32,
    project: PathBuf,
}

/// Starts a preview of a copy of `frame-zero`, checks that it answers, stops it with `signal`.
fn stopped_by(test: &str, signal: libc::c_int) -> Stopped {
    let project = frame_zero(test);
    let mut preview = start(&project);
    assert!(preview.running(), "{}", preview.errors());
    assert_eq!(preview.get("/").status, 200, "the preview did not answer before the signal");
    let (port, pid) = (preview.port, preview.pid());
    let ran = preview.stop_with(signal);
    Stopped { ran, port, pid, project }
}

fn refused(port: u16) -> bool {
    TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(2))
        .is_err_and(|error| error.kind() == std::io::ErrorKind::ConnectionRefused)
}

// ---------------------------------------------------------------------------------------------------
// Criterion 1: an interrupt prints `Preview stopped` and the exit code is 0.

#[test]
fn s2_t061_c1_an_interrupt_makes_the_preview_print_preview_stopped_and_exit_0() {
    let stopped = stopped_by("s2_t061_c1_interrupt", libc::SIGINT);
    assert_eq!(stopped.ran.code, 0, "{}", stopped.ran.all());
    assert_eq!(stopped.ran.stdout, "Preview stopped\n", "{}", stopped.ran.all());
    assert_eq!(stopped.ran.stderr, "", "{}", stopped.ran.all());
}

#[test]
fn s2_t061_c1_the_preview_keeps_running_until_the_signal_and_the_line_comes_only_after_it() {
    let project = frame_zero("s2_t061_c1_after");
    let mut preview = start(&project);
    // Control: without a signal nothing is printed and the command does not end.
    assert_eq!(preview.next_line(Duration::from_millis(800)), None);
    assert!(preview.running());
    preview.send(libc::SIGINT);
    assert_eq!(preview.next_line(Duration::from_secs(5)).as_deref(), Some("Preview stopped"));
    let ran = preview.finish();
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(ran.stdout, "", "nothing follows the line: {}", ran.all());
}

#[test]
fn s2_t061_c1_an_interrupt_with_a_page_open_in_a_browser_stops_the_preview_the_same_way() {
    let project = frame_zero("s2_t061_c1_page");
    let preview = start(&project);
    let mut window = Window::open("s2_t061_c1_page_browser", &preview.address, 800, 450);
    window.wait_state("shown");
    let port = preview.port;
    let ran = preview.stop_with(libc::SIGINT);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert_eq!(ran.stdout, "Preview stopped\n", "{}", ran.all());
    assert_eq!(ran.stderr, "", "{}", ran.all());
    assert!(refused(port), "the address still answers after the stop");
    // The browser is the author's own and Framesmith never stops it: it is still there, and answers.
    assert_eq!(window.eval("1 + 1").as_i64(), Some(2));
}

#[test]
fn s2_t061_c1_the_exit_code_is_never_the_cancelled_code_130() {
    for (name, signal) in SIGNALS {
        let stopped = stopped_by(&format!("s2_t061_c1_not_130_{name}"), signal);
        assert_ne!(stopped.ran.code, 130, "{name}: {}", stopped.ran.all());
        assert_eq!(stopped.ran.code, 0, "{name}: {}", stopped.ran.all());
    }
}

#[test]
fn s2_t061_c1_the_run_log_tells_the_stop_in_order_under_one_run_id_without_a_warning_or_an_error() {
    let project = frame_zero("s2_t061_c1_log");
    let logs = scratch("s2_t061_c1_log_folder");
    let env = [
        ("FRAMESMITH_LOG_DIR", logs.as_os_str()),
        ("FRAMESMITH_LOG", std::ffi::OsStr::new("info")),
    ];
    let preview = Preview::start(project.parent().unwrap(), &["preview", text(&project)], &env);
    let ran = preview.stop_with(libc::SIGINT);
    assert_eq!(ran.code, 0, "{}", ran.all());
    let log = std::fs::read_to_string(logs.join("framesmith.log")).unwrap();
    let lines: Vec<&str> = log.lines().collect();
    let position = |needle: &str| {
        lines
            .iter()
            .position(|line| line.contains(needle))
            .unwrap_or_else(|| panic!("no line with {needle:?} in\n{log}"))
    };
    let running = position("preview running");
    let requested = position("stop requested");
    let closed = position("preview stopped listening");
    let stopped = position("preview stopped command=");
    let finished = position("command finished");
    assert!(
        running < requested && requested < closed && closed < stopped && stopped < finished,
        "{log}"
    );
    assert!(
        lines[finished].contains("exit=0") && lines[finished].contains("outcome=\"stopped\""),
        "{log}"
    );
    let runs: std::collections::BTreeSet<&str> = lines.iter().filter_map(|line| line.split_whitespace().nth(3)).collect();
    assert_eq!(runs.len(), 1, "{runs:?} in\n{log}");
    assert!(
        lines.iter().all(|line| !line.contains(" ERROR ") && !line.contains(" WARN ")),
        "{log}"
    );
}

// ---------------------------------------------------------------------------------------------------
// Criterion 2: the address no longer answers and the same port can be used at once.

/// The checks of criterion 2 after a stop by `signal`.
fn assert_address_free(test: &str, signal: libc::c_int) {
    let project = frame_zero(test);
    let preview = start(&project);
    let port = preview.port;
    // Control: while it runs the address answers and the port cannot be listened on.
    assert_eq!(preview.get("/").status, 200);
    assert_eq!(
        TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map(|_| ()).map_err(|e| e.kind()),
        Err(std::io::ErrorKind::AddrInUse),
        "the port was free while the preview ran"
    );
    // Connections the preview closed itself are left behind, as a page leaves them.
    for _ in 0..5 {
        assert_eq!(preview.get("/index.html").status, 200);
    }
    let ran = preview.stop_with(signal);
    assert_eq!(ran.code, 0, "{}", ran.all());
    // The address no longer answers: the connection is refused.
    assert!(refused(port), "the address still answers after the stop");
    // The same port is free at once: a listener takes it and serves a connection.
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap_or_else(|e| panic!("the port {port} is not free: {e}"));
    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    let (mut served, _) = listener.accept().unwrap();
    client.write_all(b"ping").unwrap();
    let mut got = [0u8; 4];
    served.read_exact(&mut got).unwrap();
    assert_eq!(&got, b"ping");
}

#[test]
fn s2_t061_c2_after_an_interrupt_the_address_is_refused_and_the_port_can_be_listened_on_at_once() {
    assert_address_free("s2_t061_c2_interrupt", libc::SIGINT);
}

#[test]
fn s2_t061_c2_the_address_is_already_refused_when_the_line_is_read() {
    let project = frame_zero("s2_t061_c2_order");
    let preview = start(&project);
    let port = preview.port;
    assert_eq!(preview.get("/").status, 200);
    preview.send(libc::SIGINT);
    assert_eq!(preview.next_line(Duration::from_secs(5)).as_deref(), Some("Preview stopped"));
    // At the very moment the line is read, before the command is waited for.
    assert!(refused(port), "the address answered after the line was printed");
    drop(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap_or_else(|e| panic!("the port {port} is not free: {e}")));
    assert_eq!(preview.finish().code, 0);
}

#[test]
fn s2_t061_c2_a_new_preview_starts_at_once_after_the_stop_and_answers() {
    let project = frame_zero("s2_t061_c2_again");
    let first = start(&project);
    let first_port = first.port;
    let ran = first.stop_with(libc::SIGINT);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(refused(first_port));
    let mut second = start(&project);
    assert_eq!(second.get("/").status, 200, "{}", second.errors());
    assert!(second.running());
    let ran = second.stop_with(libc::SIGINT);
    assert_eq!((ran.code, ran.stdout.as_str()), (0, "Preview stopped\n"), "{}", ran.all());
}

#[test]
fn s2_t061_c2_a_connection_that_sends_nothing_does_not_delay_the_stop() {
    let project = frame_zero("s2_t061_c2_idle");
    let preview = start(&project);
    let port = preview.port;
    // A page that opened a connection and says nothing: the server holds it for five seconds at most.
    let idle = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
    std::thread::sleep(Duration::from_millis(200));
    let began = std::time::Instant::now();
    let ran = preview.stop_with(libc::SIGINT);
    assert!(began.elapsed() < Duration::from_secs(3), "the stop took {:?}", began.elapsed());
    assert_eq!((ran.code, ran.stdout.as_str()), (0, "Preview stopped\n"), "{}", ran.all());
    assert!(refused(port));
    drop(TcpListener::bind((Ipv4Addr::LOCALHOST, port)).unwrap_or_else(|e| panic!("the port {port} is not free: {e}")));
    drop(idle);
}

// ---------------------------------------------------------------------------------------------------
// Criterion 3: no program started for the preview is left.

/// A program that has a child of its own, so that the lookup of children is shown to see one.
fn parent_with_a_child() -> std::process::Child {
    std::process::Command::new("/bin/sh")
        .args(["-c", "sleep 30 & wait"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .unwrap()
}

#[test]
fn s2_t061_c3_no_program_is_started_for_the_preview_and_none_is_left_after_a_stop() {
    // Control: the lookups see a program and its child, and see the child go when it is stopped.
    let mut parent = parent_with_a_child();
    std::thread::sleep(Duration::from_millis(300));
    let children = live::descendants(parent.id());
    assert_eq!(children.len(), 1, "the lookup of children saw {children:?}");
    parent.kill().unwrap();
    parent.wait().unwrap();
    for child in &children {
        // SAFETY: the child of the shell started just above.
        unsafe { libc::kill(*child as libc::pid_t, libc::SIGKILL) };
    }

    let project = frame_zero("s2_t061_c3_none");
    let marker = project.display().to_string();
    let preview = start(&project);
    let pid = preview.pid();
    preview.get("/");
    // While it runs: the lookup finds the preview by its command line, and it has no child at all.
    assert_eq!(live::running_with(&marker).len(), 1, "{:?}", live::running_with(&marker));
    assert_eq!(live::descendants(pid), Vec::<u32>::new());
    let ran = preview.stop_with(libc::SIGINT);
    assert_eq!(ran.code, 0, "{}", ran.all());
    assert!(
        !live::alive(pid),
        "the command is still running after it printed its line and ended"
    );
    assert_eq!(live::running_with(&marker), Vec::<String>::new());
    assert_eq!(live::descendants(pid), Vec::<u32>::new());
}

// ---------------------------------------------------------------------------------------------------
// Criterion 4: a terminate signal has the same result.

#[test]
fn s2_t061_c4_a_terminate_signal_prints_preview_stopped_and_exits_0() {
    let stopped = stopped_by("s2_t061_c4_terminate", libc::SIGTERM);
    assert_eq!(stopped.ran.code, 0, "{}", stopped.ran.all());
    assert_eq!(stopped.ran.stdout, "Preview stopped\n", "{}", stopped.ran.all());
    assert_eq!(stopped.ran.stderr, "", "{}", stopped.ran.all());
}

#[test]
fn s2_t061_c4_after_a_terminate_signal_the_address_is_refused_and_the_port_can_be_listened_on_at_once() {
    assert_address_free("s2_t061_c4_address", libc::SIGTERM);
}

#[test]
fn s2_t061_c4_after_a_terminate_signal_no_program_of_the_preview_is_left() {
    let stopped = stopped_by("s2_t061_c4_none", libc::SIGTERM);
    assert_eq!(stopped.ran.code, 0, "{}", stopped.ran.all());
    assert!(!live::alive(stopped.pid));
    assert_eq!(live::running_with(&stopped.project.display().to_string()), Vec::<String>::new());
    assert_eq!(live::descendants(stopped.pid), Vec::<u32>::new());
    assert!(refused(stopped.port));
}

#[test]
fn s2_t061_c4_a_terminate_signal_with_a_page_open_in_a_browser_stops_the_preview_the_same_way() {
    let project = frame_zero("s2_t061_c4_page");
    let preview = start(&project);
    let mut window = Window::open("s2_t061_c4_page_browser", &preview.address, 800, 450);
    window.wait_state("shown");
    let port = preview.port;
    let ran = preview.stop_with(libc::SIGTERM);
    assert_eq!(
        (ran.code, ran.stdout.as_str(), ran.stderr.as_str()),
        (0, "Preview stopped\n", ""),
        "{}",
        ran.all()
    );
    assert!(refused(port));
    assert_eq!(window.eval("1 + 1").as_i64(), Some(2));
}

#[test]
fn s2_t061_c4_both_signals_give_the_same_words_the_same_code_and_a_free_port() {
    let ends: Vec<(i32, String, String, bool)> = SIGNALS
        .iter()
        .map(|(name, signal)| {
            let stopped = stopped_by(&format!("s2_t061_c4_same_{name}"), *signal);
            (
                stopped.ran.code,
                stopped.ran.stdout.clone(),
                stopped.ran.stderr.clone(),
                refused(stopped.port),
            )
        })
        .collect();
    assert_eq!(ends[0], ends[1]);
    assert_eq!(ends[0], (0, "Preview stopped\n".to_string(), String::new(), true));
}

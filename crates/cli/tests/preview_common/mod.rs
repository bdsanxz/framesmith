//! Shared by the preview tests: starting `framesmith preview`, speaking HTTP to it, and a browser opened
//! at its address. A test that includes this module also includes `common`.
#![allow(dead_code)]

mod window;

pub use window::Window;

use crate::common::{ffmpeg, BIN};
use measure::{Frame, Ran};
use std::ffi::OsStr;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV6, TcpStream};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long a started preview is given to print its address.
const START_TIME: Duration = Duration::from_secs(30);

/// A `framesmith preview` that a test started. It is stopped when this value goes out of scope, also
/// when the test fails.
pub struct Preview {
    child: Option<Child>,
    lines: Receiver<String>,
    errors: Arc<Mutex<String>>,
    /// The first line the command printed.
    pub line: String,
    /// `http://127.0.0.1:<port>/`, taken from that line.
    pub address: String,
    pub port: u16,
}

impl Preview {
    /// Starts the binary with `args` in `dir` and waits for its first line of output, which must hold
    /// an address. Panics with everything the command printed when it does not come.
    pub fn start(dir: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Preview {
        Preview::start_program(Path::new(BIN), dir, args, env, false)
    }

    /// As `start`, with the environment emptied first (`env` is then everything the command has).
    pub fn start_bare(program: &Path, dir: &Path, args: &[&str], env: &[(&str, &OsStr)]) -> Preview {
        Preview::start_program(program, dir, args, env, true)
    }

    fn start_program(program: &Path, dir: &Path, args: &[&str], env: &[(&str, &OsStr)], bare: bool) -> Preview {
        let mut command = Command::new(program);
        command.args(args).current_dir(dir);
        if bare {
            command.env_clear();
        }
        let mut child = command
            .envs(env.iter().copied())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| panic!("{} could not be started: {e}", program.display()));
        let (sender, lines) = mpsc::channel();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        std::thread::spawn(move || {
            for line in stdout.lines().map_while(Result::ok) {
                if sender.send(line).is_err() {
                    return;
                }
            }
        });
        let errors = Arc::new(Mutex::new(String::new()));
        let mut stderr = child.stderr.take().unwrap();
        let kept = Arc::clone(&errors);
        std::thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            while let Ok(count @ 1..) = stderr.read(&mut buffer) {
                kept.lock().unwrap().push_str(&String::from_utf8_lossy(&buffer[..count]));
            }
        });
        let mut preview = Preview {
            child: Some(child),
            lines,
            errors,
            line: String::new(),
            address: String::new(),
            port: 0,
        };
        let line = match preview.lines.recv_timeout(START_TIME) {
            Ok(line) => line,
            Err(why) => {
                let said = preview.errors();
                panic!(
                    "no first line from `framesmith {}` ({why:?}); standard error so far: {said}",
                    args.join(" ")
                );
            }
        };
        let address = line
            .split_whitespace()
            .find(|word| word.starts_with("http://"))
            .unwrap_or_else(|| panic!("the first line holds no address: {line:?}"))
            .to_string();
        let port = address
            .strip_prefix("http://127.0.0.1:")
            .and_then(|rest| rest.strip_suffix('/'))
            .and_then(|port| port.parse().ok())
            .unwrap_or_else(|| panic!("the address is not http://127.0.0.1:<port>/ : {address:?}"));
        preview.line = line;
        preview.address = address;
        preview.port = port;
        preview
    }

    pub fn pid(&self) -> u32 {
        self.child.as_ref().unwrap().id()
    }

    /// Whether the command is still running.
    pub fn running(&mut self) -> bool {
        self.child.as_mut().unwrap().try_wait().unwrap().is_none()
    }

    /// What the command has printed on standard error so far.
    pub fn errors(&self) -> String {
        self.errors.lock().unwrap().clone()
    }

    /// The lines printed on standard output after the first, up to now.
    pub fn more_lines(&self) -> Vec<String> {
        self.lines.try_iter().collect()
    }

    /// Stops the command with a terminate signal and returns how it ended. The standard output holds the
    /// lines printed after the first one.
    pub fn stop(mut self) -> Ran {
        self.end(libc::SIGTERM)
    }

    /// Sends the command `signal` (`libc::SIGINT`, `libc::SIGTERM`) and returns how it ended, as `stop` does.
    pub fn stop_with(mut self, signal: libc::c_int) -> Ran {
        self.end(signal)
    }

    /// Sends the command `signal` and returns at once. Use `finish` to collect the end.
    pub fn send(&self, signal: libc::c_int) {
        // SAFETY: a plain signal to a child this value started.
        unsafe { libc::kill(self.pid() as libc::pid_t, signal) };
    }

    /// The next line printed on standard output, or `None` when none comes within `within`.
    pub fn next_line(&self, within: Duration) -> Option<String> {
        self.lines.recv_timeout(within).ok()
    }

    /// Waits for the command to end on its own, as `stop` does after a signal, and returns how it ended.
    pub fn finish(mut self) -> Ran {
        self.collect()
    }

    fn end(&mut self, signal: libc::c_int) -> Ran {
        if let Some(child) = self.child.as_ref() {
            // SAFETY: a plain signal to a child this value started.
            unsafe { libc::kill(child.id() as libc::pid_t, signal) };
        }
        self.collect()
    }

    fn collect(&mut self) -> Ran {
        let Some(mut child) = self.child.take() else {
            return Ran {
                code: -1,
                stdout: String::new(),
                stderr: String::new(),
            };
        };
        let until = Instant::now() + Duration::from_secs(5);
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if Instant::now() > until {
                let _ = child.kill();
                break child.wait().unwrap();
            }
            std::thread::sleep(Duration::from_millis(20));
        };
        // The reading threads end with the pipes; give them a moment to hand over the last lines.
        let mut rest = String::new();
        while let Ok(line) = self.lines.recv_timeout(Duration::from_millis(200)) {
            rest.push_str(&line);
            rest.push('\n');
        }
        Ran {
            code: status.code().or_else(|| status.signal().map(|s| 128 + s)).unwrap_or(-1),
            stdout: rest,
            stderr: self.errors(),
        }
    }

    /// `GET target` on this preview.
    pub fn get(&self, target: &str) -> Reply {
        get(self.port, target)
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        let _ = self.end(libc::SIGTERM);
    }
}

/// An answer, taken apart.
#[derive(Debug)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// What the server sent for these exact bytes of request, up to the end of the connection.
pub fn raw(port: u16, request: &[u8]) -> Vec<u8> {
    let mut stream = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, port)), Duration::from_secs(5)).unwrap();
    stream.set_read_timeout(Some(Duration::from_secs(10))).unwrap();
    stream.write_all(request).unwrap();
    let mut answer = Vec::new();
    // The server closes the connection after its answer; a reset after the last byte is not a fault.
    let _ = stream.read_to_end(&mut answer);
    answer
}

pub fn parse_reply(answer: &[u8]) -> Reply {
    let split = answer
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no end of head in {:?}", String::from_utf8_lossy(answer)));
    let head = String::from_utf8_lossy(&answer[..split]).into_owned();
    let mut lines = head.lines();
    let status = lines
        .next()
        .and_then(|line| line.split(' ').nth(1))
        .and_then(|code| code.parse().ok())
        .unwrap_or_else(|| panic!("no status in {head:?}"));
    let headers = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_string(), value.trim().to_string()))
        .collect();
    Reply {
        status,
        headers,
        body: answer[split + 4..].to_vec(),
    }
}

/// `GET target` on the loopback address at `port`.
pub fn get(port: u16, target: &str) -> Reply {
    parse_reply(&raw(
        port,
        format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n").as_bytes(),
    ))
}

/// The addresses of this machine's network interfaces other than the loopback ones, each with the
/// interface's name. A link-local IPv6 address carries its interface's number.
pub fn other_addresses() -> Vec<(String, SocketAddrNoPort)> {
    let mut found = Vec::new();
    let mut list: *mut libc::ifaddrs = std::ptr::null_mut();
    // SAFETY: `list` is a valid place for getifaddrs to put the head of a list that it allocates.
    if unsafe { libc::getifaddrs(&mut list) } != 0 {
        panic!("the interfaces of this machine cannot be listed");
    }
    let mut at = list;
    while !at.is_null() {
        // SAFETY: `at` is a node of the list that getifaddrs returned; it stays valid until freeifaddrs.
        let node = unsafe { &*at };
        at = node.ifa_next;
        if node.ifa_addr.is_null() {
            continue;
        }
        // SAFETY: the name is a NUL-terminated string owned by the node.
        let name = unsafe { std::ffi::CStr::from_ptr(node.ifa_name) }.to_string_lossy().into_owned();
        // SAFETY: `ifa_addr` is not null; its first field says which kind of address it is.
        let family = i32::from(unsafe { (*node.ifa_addr).sa_family });
        let address = if family == libc::AF_INET {
            // SAFETY: the family says the address is a `sockaddr_in`.
            let v4 = unsafe { &*node.ifa_addr.cast::<libc::sockaddr_in>() };
            SocketAddrNoPort::new(IpAddr::V4(Ipv4Addr::from(u32::from_be(v4.sin_addr.s_addr))), 0)
        } else if family == libc::AF_INET6 {
            // SAFETY: the family says the address is a `sockaddr_in6`.
            let v6 = unsafe { &*node.ifa_addr.cast::<libc::sockaddr_in6>() };
            SocketAddrNoPort::new(IpAddr::V6(Ipv6Addr::from(v6.sin6_addr.s6_addr)), v6.sin6_scope_id)
        } else {
            continue;
        };
        if !address.ip.is_loopback() && !address.ip.is_unspecified() {
            found.push((name, address));
        }
    }
    // SAFETY: `list` came from getifaddrs and is freed once.
    unsafe { libc::freeifaddrs(list) };
    found
}

/// An address of this machine, to which a port is added to connect.
#[derive(Clone, Copy, Debug)]
pub struct SocketAddrNoPort {
    pub ip: IpAddr,
    pub scope: u32,
}

impl SocketAddrNoPort {
    fn new(ip: IpAddr, scope: u32) -> SocketAddrNoPort {
        SocketAddrNoPort { ip, scope }
    }

    pub fn with_port(&self, port: u16) -> SocketAddr {
        match self.ip {
            IpAddr::V4(ip) => SocketAddr::from((ip, port)),
            IpAddr::V6(ip) => SocketAddr::V6(SocketAddrV6::new(ip, port, 0, self.scope)),
        }
    }
}

/// The result of trying to reach `address`: the words for an answer or a refusal.
pub fn reach(address: SocketAddr) -> Result<String, String> {
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(3)).map_err(|e| format!("no connection ({e})"))?;
    stream.set_read_timeout(Some(Duration::from_secs(3))).unwrap();
    let _ = stream.write_all(b"GET / HTTP/1.1\r\nHost: probe\r\nConnection: close\r\n\r\n");
    let mut first = [0u8; 16];
    let count = stream.read(&mut first).unwrap_or(0);
    Ok(String::from_utf8_lossy(&first[..count]).into_owned())
}

/// The pixels of a screenshot, as the render's frames are read.
pub fn pixels(png: &[u8], folder: &Path) -> Frame {
    std::fs::create_dir_all(folder).unwrap();
    std::fs::write(folder.join("frame-00000.png"), png).unwrap();
    measure::frames_from_pngs(&ffmpeg(), folder).unwrap().remove(0)
}

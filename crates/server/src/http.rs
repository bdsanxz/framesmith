//! The HTTP/1.1 the preview speaks: one request, one answer, then the connection closes. Only what a
//! browser needs to read a page and its files; see `docs/spec/preview.md`, "What the server answers".

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// The longest head (request line and headers) that is read.
const HEAD_LIMIT: usize = 16 * 1024;
/// How long a connection may stay silent before it is closed.
pub const IDLE: Duration = Duration::from_secs(5);

/// What a connection asked for.
#[derive(Debug, PartialEq, Eq)]
pub enum Request {
    /// `GET` of a target that begins with `/`.
    Get { target: String },
    /// A method the server does not answer.
    Other,
    /// A request that cannot be read: the status to answer with.
    Unreadable(u16),
}

/// The index of the end of the head in `bytes`, when the head is complete.
fn head_end(bytes: &[u8]) -> Option<usize> {
    (0..bytes.len()).find(|&at| bytes[at..].starts_with(b"\r\n\r\n") || bytes[at..].starts_with(b"\n\n"))
}

/// Reads the head of one request. `None` when the connection closed or went silent before a head came.
pub fn read_request(stream: &mut TcpStream) -> Option<Request> {
    let mut head: Vec<u8> = Vec::new();
    let mut buffer = [0u8; 2048];
    loop {
        if head_end(&head).is_some() {
            break;
        }
        if head.len() > HEAD_LIMIT {
            return Some(Request::Unreadable(431));
        }
        match stream.read(&mut buffer) {
            Ok(0) | Err(_) => return None,
            Ok(count) => head.extend_from_slice(&buffer[..count]),
        }
    }
    let line = head.split(|&byte| byte == b'\n').next().unwrap_or_default();
    let line = String::from_utf8_lossy(line);
    let mut words = line.trim_end_matches('\r').split(' ');
    let (Some(method), Some(target), Some(version), None) = (words.next(), words.next(), words.next(), words.next()) else {
        return Some(Request::Unreadable(400));
    };
    if !version.starts_with("HTTP/1.") || !target.starts_with('/') {
        return Some(Request::Unreadable(400));
    }
    Some(if method == "GET" {
        Request::Get {
            target: target.to_string(),
        }
    } else {
        Request::Other
    })
}

/// One answer.
pub struct Response {
    pub status: u16,
    pub content_type: &'static str,
    pub body: Vec<u8>,
    /// Sent as `Allow` when set.
    pub allow: Option<&'static str>,
}

impl Response {
    pub fn new(status: u16, content_type: &'static str, body: Vec<u8>) -> Response {
        Response {
            status,
            content_type,
            body,
            allow: None,
        }
    }

    /// A short plain-text answer.
    pub fn text(status: u16, words: &str) -> Response {
        Response::new(status, "text/plain; charset=utf-8", format!("{words}\n").into_bytes())
    }
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        431 => "Request Header Fields Too Large",
        503 => "Service Unavailable",
        _ => "Status",
    }
}

/// Writes the answer and leaves the connection ready to close.
pub fn write_response(stream: &mut TcpStream, response: &Response) -> std::io::Result<()> {
    let mut head = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n",
        response.status,
        reason(response.status),
        response.content_type,
        response.body.len()
    );
    if let Some(allow) = response.allow {
        head.push_str(&format!("Allow: {allow}\r\n"));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(&response.body)?;
    stream.flush()
}

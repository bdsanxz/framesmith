//! The running preview: a listener on the loopback address and the threads that answer it.

use crate::http::{self, Request, Response};
use crate::site;
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

/// The one address the preview listens on and is reached at.
pub const ADDRESS_HOST: &str = "127.0.0.1";

/// How often a waiting preview looks whether it was asked to stop.
const WATCH: Duration = Duration::from_millis(20);

/// Why a preview could not start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartError {
    /// The system's reason, in words.
    pub reason: String,
}

impl std::fmt::Display for StartError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.reason)
    }
}

/// A preview that is answering. Dropping it stops it: the port is freed and the threads end.
pub struct Preview {
    port: u16,
    stop: Arc<AtomicBool>,
    accepting: Option<JoinHandle<()>>,
}

impl Preview {
    /// Starts answering for the project folder `project` on a free port of the loopback address.
    /// The server answers as soon as this returns.
    pub fn start(project: &Path) -> Result<Preview, StartError> {
        let root = std::fs::canonicalize(project).map_err(|e| StartError {
            reason: format!("the project folder {} cannot be read ({e})", project.display()),
        })?;
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).map_err(|e| StartError {
            reason: format!("the system refused the socket ({e})"),
        })?;
        let port = listener
            .local_addr()
            .map_err(|e| StartError {
                reason: format!("the address it listens on is not known ({e})"),
            })?
            .port();
        let stop = Arc::new(AtomicBool::new(false));
        let accepting = {
            let stop = Arc::clone(&stop);
            std::thread::Builder::new()
                .name("preview-accept".to_string())
                .spawn(move || accept(&listener, &Arc::new(root), &stop))
                .map_err(|e| StartError {
                    reason: format!("a thread could not be started ({e})"),
                })?
        };
        tracing::info!(target: "server", port, project = %project.display(), "preview listening");
        Ok(Preview {
            port,
            stop,
            accepting: Some(accepting),
        })
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The address to open, `http://127.0.0.1:<port>/`.
    pub fn address(&self) -> String {
        format!("http://{ADDRESS_HOST}:{}/", self.port)
    }

    /// Waits until `stop` turns true (or the server ends by itself). The preview keeps answering; dropping
    /// it then stops it.
    pub fn wait_until(&self, stop: &AtomicBool) {
        while !stop.load(Ordering::SeqCst) && self.accepting.as_ref().is_some_and(|accepting| !accepting.is_finished()) {
            std::thread::sleep(WATCH);
        }
    }
}

impl Drop for Preview {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(accepting) = self.accepting.take() {
            // The accepting thread is blocked in accept; a connection to ourselves wakes it.
            let _ = TcpStream::connect_timeout(&SocketAddr::from((Ipv4Addr::LOCALHOST, self.port)), Duration::from_secs(2));
            let _ = accepting.join();
            tracing::info!(target: "server", port = self.port, "preview stopped listening");
        }
    }
}

fn accept(listener: &TcpListener, root: &Arc<PathBuf>, stop: &AtomicBool) {
    for connection in listener.incoming() {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        match connection {
            Ok(stream) => {
                let root = Arc::clone(root);
                if let Err(error) = std::thread::Builder::new()
                    .name("preview-answer".to_string())
                    .spawn(move || answer(stream, &root))
                {
                    tracing::warn!(target: "server", error = %error, "a thread to answer a request could not be started");
                }
            }
            Err(error) => {
                tracing::warn!(target: "server", error = %error, "a connection could not be accepted");
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

/// Reads one request from the connection and answers it.
fn answer(mut stream: TcpStream, root: &Path) {
    let _ = stream.set_read_timeout(Some(http::IDLE));
    let _ = stream.set_write_timeout(Some(http::IDLE));
    let Some(request) = http::read_request(&mut stream) else {
        return;
    };
    let (shown, response) = match request {
        Request::Get { target } => {
            let response = site::answer(root, &target);
            (target, response)
        }
        Request::Other => {
            let mut response = Response::text(405, "Only GET is answered.");
            response.allow = Some("GET");
            ("(not a GET)".to_string(), response)
        }
        Request::Unreadable(status) => ("(unreadable)".to_string(), Response::text(status, "The request cannot be read.")),
    };
    tracing::debug!(target: "server", path = %shown, status = response.status, "request answered");
    if let Err(error) = http::write_response(&mut stream, &response) {
        // A page that was closed before the answer arrived is the client's choice, not a fault.
        tracing::debug!(target: "server", path = %shown, error = %error, "an answer could not be written");
    }
}

//! The HTTP side of the memory server: reading requests within limits,
//! routing `POST /mcp/<harness>` and `POST /hooks/<harness>`, and writing
//! responses.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use serde_json::{Value, json};

use super::Server;
use super::rpc::rpc_error;
use crate::application::memory::domain::SessionId;

// The limits and `UntilDeadline` are `pub(super)` only for the tests in
// mod.rs; the rest of the server uses them from this file.
/// Largest request body accepted.
pub(super) const MAX_BODY: usize = 1024 * 1024;
/// Largest request line and headers accepted, together.
pub(super) const MAX_HEAD: usize = 16 * 1024;
/// How long a connection may take to send its whole request, however slowly
/// it sends it.
const READ_TIMEOUT: Duration = Duration::from_secs(30);
/// Most connections served at once; more are told to come back later, so
/// a client cannot make the server start threads without end.
pub(super) const MAX_CONNECTIONS: usize = 32;

/// An HTTP request, as far as the server reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpRequest {
    /// `GET`, `POST`, ...
    pub method: String,
    /// The path, such as `/mcp/claude`.
    pub path: String,
    /// The session named by the `X-Casulo-Session` header, when sent.
    pub session: Option<SessionId>,
    /// The body.
    pub body: Vec<u8>,
}

/// An HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    /// Status code.
    pub status: u16,
    /// JSON body, empty for 202.
    pub body: String,
}

impl HttpResponse {
    pub(super) fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            body: body.to_string(),
        }
    }

    pub(super) fn empty(status: u16) -> Self {
        Self {
            status,
            body: String::new(),
        }
    }
}

impl Server {
    /// Answers one HTTP request.
    pub fn handle(&self, request: &HttpRequest) -> HttpResponse {
        if let Some(harness) = harness_after(&request.path, "/hooks/") {
            if request.method != "POST" {
                return HttpResponse::json(405, json!({ "error": "use POST" }));
            }
            return self.hook(&harness, request.session.as_ref(), &request.body);
        }
        let Some(harness) = harness_after(&request.path, "/mcp/") else {
            return HttpResponse::json(404, json!({ "error": "not found; use /mcp/<harness>" }));
        };
        if request.method != "POST" {
            // No server-sent events: every answer comes in the POST response.
            return HttpResponse::json(405, json!({ "error": "use POST" }));
        }
        let message: Value = match serde_json::from_slice(&request.body) {
            Ok(message) => message,
            Err(_) => {
                return HttpResponse::json(
                    400,
                    rpc_error(Value::Null, -32700, "the body is not valid JSON"),
                );
            }
        };
        if !message.is_object() {
            // A batch (an array) or a bare value: answer it rather than
            // dropping it as if it were a notification.
            return HttpResponse::json(
                400,
                rpc_error(
                    Value::Null,
                    -32600,
                    "send one JSON-RPC request per POST; batches are not supported",
                ),
            );
        }
        match self.rpc(&harness, request.session.as_ref(), &message) {
            Some(answer) => HttpResponse::json(200, answer),
            // Notifications get no answer.
            None => HttpResponse::empty(202),
        }
    }
}

/// The harness named in `path` after `prefix`, if it is a valid name.
fn harness_after(path: &str, prefix: &str) -> Option<String> {
    path.strip_prefix(prefix)
        .filter(|name| is_harness_name(name))
        .map(str::to_string)
}

/// Harness names in URLs: lowercase letters, digits and `-`.
fn is_harness_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 63
        && name.chars().all(|character| {
            character.is_ascii_lowercase() || character.is_ascii_digit() || character == '-'
        })
}

/// Serves requests on `listener` forever, one thread per connection.
pub fn serve(server: Arc<Server>, listener: TcpListener) -> Result<()> {
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(mut stream) = stream else {
            continue;
        };
        if open.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
            open.fetch_sub(1, Ordering::SeqCst);
            let busy = HttpResponse::json(503, json!({ "error": "too many connections; retry" }));
            let _ = write_response(&mut stream, &busy);
            continue;
        }
        let server = Arc::clone(&server);
        let open = Arc::clone(&open);
        thread::spawn(move || {
            if let Err(error) = answer(&server, stream) {
                eprintln!("memory: {error:#}");
            }
            open.fetch_sub(1, Ordering::SeqCst);
        });
    }
    Ok(())
}

/// Reads from a connection until a deadline: each read waits at most for
/// the time left, so sending one byte at a time cannot hold it open.
pub(super) struct UntilDeadline<'a> {
    pub(super) stream: &'a TcpStream,
    pub(super) deadline: Instant,
}

impl Read for UntilDeadline<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let left = self.deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            return Err(std::io::ErrorKind::TimedOut.into());
        }
        self.stream.set_read_timeout(Some(left))?;
        // `&TcpStream` reads too, so the stream itself is only borrowed.
        let mut stream = self.stream;
        stream.read(buffer)
    }
}

fn answer(server: &Server, mut stream: TcpStream) -> Result<()> {
    let mut reader = UntilDeadline {
        stream: &stream,
        deadline: Instant::now() + READ_TIMEOUT,
    };
    let response = match read_request(&mut reader) {
        Ok(request) => server.handle(&request),
        Err(response) => response,
    };
    write_response(&mut stream, &response)
}

fn write_response(stream: &mut TcpStream, response: &HttpResponse) -> Result<()> {
    let reason = match response.status {
        200 => "OK",
        202 => "Accepted",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        411 => "Length Required",
        413 => "Payload Too Large",
        431 => "Request Header Fields Too Large",
        500 => "Internal Server Error",
        503 => "Service Unavailable",
        _ => "Error",
    };
    write!(
        stream,
        "HTTP/1.1 {} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        response.status,
        response.body.len(),
        response.body
    )?;
    stream.flush()?;
    Ok(())
}

/// Reads one HTTP request, refusing a request line and headers over 16 KiB,
/// bodies over 1 MiB and bodies without a length.
pub fn read_request(stream: &mut impl Read) -> std::result::Result<HttpRequest, HttpResponse> {
    let bad = |status: u16, message: &str| HttpResponse::json(status, json!({ "error": message }));
    let mut reader = BufReader::new(stream);
    let mut head_left = MAX_HEAD;
    // Reads one line of the head, counting it against `head_left`; `take`
    // stops a line with no end from growing past the limit.
    let mut read_head_line = |reader: &mut BufReader<_>, line: &mut String| {
        let read = reader
            .by_ref()
            .take(head_left as u64)
            .read_line(line)
            .map_err(|_| bad(400, "cannot read the request"))?;
        head_left -= read;
        if head_left == 0 && !line.ends_with('\n') {
            return Err(bad(
                431,
                "the request line and headers are larger than 16 KiB",
            ));
        }
        Ok(read)
    };
    let mut line = String::new();
    read_head_line(&mut reader, &mut line)?;
    let mut parts = line.split_whitespace();
    let (Some(method), Some(path)) = (parts.next(), parts.next()) else {
        return Err(bad(400, "not an HTTP request"));
    };
    let (method, path) = (method.to_string(), path.to_string());
    let mut length: Option<usize> = None;
    let mut session: Option<SessionId> = None;
    loop {
        let mut header = String::new();
        let read = read_head_line(&mut reader, &mut header)?;
        if read == 0 || header.trim().is_empty() {
            break;
        }
        let Some((name, value)) = header.split_once(':') else {
            continue;
        };
        let name = name.trim();
        if name.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().ok();
        } else if name.eq_ignore_ascii_case("x-casulo-session") {
            session = Some(SessionId::parse(value.trim()).map_err(|_| {
                bad(
                    400,
                    "invalid X-Casulo-Session; use 1 to 64 letters, digits and -",
                )
            })?);
        }
    }
    let body = match (method.as_str(), length) {
        (_, Some(length)) if length > MAX_BODY => {
            return Err(bad(413, "the body is larger than 1 MiB"));
        }
        (_, Some(length)) => {
            let mut body = vec![0; length];
            reader
                .read_exact(&mut body)
                .map_err(|_| bad(400, "the body is shorter than Content-Length"))?;
            body
        }
        ("POST", None) => return Err(bad(411, "Content-Length is required")),
        _ => Vec::new(),
    };
    Ok(HttpRequest {
        method,
        path,
        session,
        body,
    })
}

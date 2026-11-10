//! The loopback HTTP surface: the board, a long poll for the next one and a
//! server-sent stream of every one. One short-lived thread per connection.

use crate::{
    Error, Result,
    hub::{Hub, Published},
};
use std::{
    io::{self, Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

const HEAD_LIMIT: usize = 8 * 1024;
const HEAD_TIMEOUT: Duration = Duration::from_secs(5);
const WRITE_TIMEOUT: Duration = Duration::from_secs(10);
/// Streams and long polls each hold a thread, so their number is bounded.
const CONNECTION_LIMIT: usize = 64;
const DEFAULT_WAIT: Duration = Duration::from_secs(25);
const LONGEST_WAIT: Duration = Duration::from_secs(60);
const HEARTBEAT: Duration = Duration::from_secs(15);

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Request {
    /// The board now, or with `after` the next one past that revision.
    Board {
        after: Option<u64>,
        wait: Duration,
    },
    Events,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    Malformed,
    /// The request named a host other than this machine's loopback, as a web
    /// page does when it reaches the port through a rebound name.
    ForeignHost,
    Method,
    NotFound,
    Busy,
}

impl Refusal {
    fn status(self) -> &'static str {
        match self {
            Self::Malformed => "400 Bad Request",
            Self::ForeignHost => "403 Forbidden",
            Self::Method => "405 Method Not Allowed",
            Self::NotFound => "404 Not Found",
            Self::Busy => "503 Service Unavailable",
        }
    }
}

/// Loopback only: the board carries session titles, and nothing here
/// authenticates a caller.
pub(crate) fn listen(port: u16) -> Result<TcpListener> {
    TcpListener::bind((Ipv4Addr::LOCALHOST, port)).map_err(Error::Listen)
}

pub(crate) fn serve(listener: &TcpListener, hub: &Arc<Hub>) {
    let open = Arc::new(AtomicUsize::new(0));
    for stream in listener.incoming() {
        let Ok(stream) = stream else {
            continue;
        };
        let (hub, open) = (hub.clone(), open.clone());
        let busy = open.fetch_add(1, Ordering::AcqRel) >= CONNECTION_LIMIT;
        thread::spawn(move || {
            // A client that goes away mid-answer is not an error worth a line.
            let _ = if busy {
                refuse(&stream, Refusal::Busy)
            } else {
                answer(&stream, &hub)
            };
            open.fetch_sub(1, Ordering::AcqRel);
        });
    }
}

fn answer(mut stream: &TcpStream, hub: &Hub) -> io::Result<()> {
    stream.set_read_timeout(Some(HEAD_TIMEOUT))?;
    stream.set_write_timeout(Some(WRITE_TIMEOUT))?;
    let request = match read_head(stream) {
        Some(head) => parse(&head),
        None => Request::Refused(Refusal::Malformed),
    };
    match request {
        Request::Board { after: None, .. } => send_board(stream, &hub.current()),
        Request::Board {
            after: Some(after),
            wait,
        } => match hub.newer_than(after, wait) {
            Some(board) => send_board(stream, &board),
            None => stream.write_all(
                b"HTTP/1.1 204 No Content\r\nCache-Control: no-store\r\nConnection: close\r\n\r\n",
            ),
        },
        Request::Events => stream_events(stream, hub),
        Request::Refused(refusal) => refuse(stream, refusal),
    }
}

fn read_head(mut stream: &TcpStream) -> Option<String> {
    let mut head = Vec::new();
    let mut chunk = [0; 1024];
    while !head.windows(4).any(|window| window == b"\r\n\r\n") {
        let read = stream.read(&mut chunk).ok()?;
        if read == 0 || head.len() + read > HEAD_LIMIT {
            return None;
        }
        head.extend_from_slice(&chunk[..read]);
    }
    String::from_utf8(head).ok()
}

pub(crate) fn parse(head: &str) -> Request {
    let mut lines = head.split("\r\n");
    let mut line = lines.next().unwrap_or_default().split(' ');
    let (Some(method), Some(target), Some(_), None) =
        (line.next(), line.next(), line.next(), line.next())
    else {
        return Request::Refused(Refusal::Malformed);
    };
    let foreign = lines
        .filter_map(|line| line.split_once(':'))
        .filter(|(name, _)| name.eq_ignore_ascii_case("host"))
        .any(|(_, host)| !loopback(host.trim()));
    if foreign {
        return Request::Refused(Refusal::ForeignHost);
    }
    if method != "GET" {
        return Request::Refused(Refusal::Method);
    }
    let (path, query) = target.split_once('?').unwrap_or((target, ""));
    match path {
        "/board" => board_request(query),
        "/events" => Request::Events,
        _ => Request::Refused(Refusal::NotFound),
    }
}

fn board_request(query: &str) -> Request {
    let mut after = None;
    let mut wait = DEFAULT_WAIT;
    for (name, value) in query.split('&').filter_map(|pair| pair.split_once('=')) {
        let Ok(value) = value.parse::<u64>() else {
            return Request::Refused(Refusal::Malformed);
        };
        match name {
            "after" => after = Some(value),
            "wait" => wait = Duration::from_secs(value).min(LONGEST_WAIT),
            _ => {}
        }
    }
    Request::Board { after, wait }
}

fn loopback(host: &str) -> bool {
    let name = host.rsplit_once(':').map_or(host, |(name, port)| {
        if port.bytes().all(|byte| byte.is_ascii_digit()) {
            name
        } else {
            host
        }
    });
    name == "127.0.0.1" || name.eq_ignore_ascii_case("localhost")
}

fn send_board(mut stream: &TcpStream, board: &Published) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        board.json.len()
    );
    stream.write_all(head.as_bytes())?;
    stream.write_all(board.json.as_bytes())
}

fn stream_events(mut stream: &TcpStream, hub: &Hub) -> io::Result<()> {
    stream.write_all(
        b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nCache-Control: no-store\r\n\
          Connection: close\r\n\r\n",
    )?;
    let mut board = hub.current();
    loop {
        // The JSON is one line: serde_json escapes every line break in a string.
        let event = format!(
            "event: board\nid: {}\ndata: {}\n\n",
            board.revision, board.json
        );
        stream.write_all(event.as_bytes())?;
        stream.flush()?;
        board = loop {
            match hub.newer_than(board.revision, HEARTBEAT) {
                Some(board) => break board,
                // A write is the only way to learn the client has gone.
                None => stream.write_all(b": waiting\n\n")?,
            }
        };
    }
}

fn refuse(mut stream: &TcpStream, refusal: Refusal) -> io::Result<()> {
    let head = format!(
        "HTTP/1.1 {}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
        refusal.status()
    );
    stream.write_all(head.as_bytes())
}

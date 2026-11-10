use super::*;
use crate::http::{Refusal, Request, listen, parse, serve};
use std::{
    io::{BufRead, BufReader, Read, Write},
    net::{SocketAddr, TcpStream},
    thread,
    time::Duration,
};

fn get(target: &str) -> String {
    format!("GET {target} HTTP/1.1\r\nHost: 127.0.0.1:7480\r\n\r\n")
}

#[test]
fn requests_route_by_path_and_query() {
    let wait = Duration::from_secs(25);
    assert_eq!(parse(&get("/board")), Request::Board { after: None, wait });
    assert_eq!(
        parse(&get("/board?after=12")),
        Request::Board {
            after: Some(12),
            wait
        }
    );
    assert_eq!(
        parse(&get("/board?after=12&wait=3&x=1")),
        Request::Board {
            after: Some(12),
            wait: Duration::from_secs(3),
        }
    );
    // A wait is capped, so a thread is never held for long.
    assert_eq!(
        parse(&get("/board?after=0&wait=86400")),
        Request::Board {
            after: Some(0),
            wait: Duration::from_secs(60),
        }
    );
    assert_eq!(parse(&get("/events")), Request::Events);
}

#[test]
fn other_requests_are_refused() {
    let refused = |head: &str| match parse(head) {
        Request::Refused(refusal) => refusal,
        request => panic!("{head:?} was accepted as {request:?}"),
    };
    assert_eq!(refused(&get("/")), Refusal::NotFound);
    assert_eq!(refused(&get("/board/extra")), Refusal::NotFound);
    assert_eq!(refused(&get("/board?after=soon")), Refusal::Malformed);
    assert_eq!(refused("GET /board\r\n\r\n"), Refusal::Malformed);
    assert_eq!(refused("\r\n\r\n"), Refusal::Malformed);
    assert_eq!(
        refused("POST /board HTTP/1.1\r\nHost: localhost\r\n\r\n"),
        Refusal::Method
    );
    // A page on another site reaches the port under its own name.
    assert_eq!(
        refused("GET /board HTTP/1.1\r\nhost: board.example:7480\r\n\r\n"),
        Refusal::ForeignHost
    );
    assert_eq!(
        refused("GET /board HTTP/1.1\r\nHost: 127.0.0.1.example\r\n\r\n"),
        Refusal::ForeignHost
    );
}

#[test]
fn local_names_are_accepted_with_or_without_a_port() {
    for host in ["127.0.0.1", "127.0.0.1:7480", "localhost", "LOCALHOST:80"] {
        let head = format!("GET /events HTTP/1.1\r\nHost: {host}\r\n\r\n");
        assert_eq!(parse(&head), Request::Events, "{host}");
    }
    assert_eq!(parse("GET /events HTTP/1.0\r\n\r\n"), Request::Events);
}

fn served(hub: &Arc<Hub>) -> SocketAddr {
    let listener = listen(0).unwrap();
    let address = listener.local_addr().unwrap();
    assert!(address.ip().is_loopback());
    let hub = hub.clone();
    thread::spawn(move || serve(&listener, &hub));
    address
}

/// The status line and the body of one request.
fn fetch(address: SocketAddr, target: &str) -> (String, String) {
    let mut stream = TcpStream::connect(address).unwrap();
    stream.write_all(get(target).as_bytes()).unwrap();
    let mut response = String::new();
    stream.read_to_string(&mut response).unwrap();
    let (head, body) = response.split_once("\r\n\r\n").unwrap();
    (head.lines().next().unwrap().to_owned(), body.to_owned())
}

#[test]
fn the_board_is_served_now_and_when_it_next_changes() {
    let hub = Arc::new(hub(&["local"]));
    let address = served(&hub);

    let (status, body) = fetch(address, "/board");
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(board(&body).revision, 7);

    assert_eq!(
        fetch(address, "/board?after=7&wait=0"),
        ("HTTP/1.1 204 No Content".to_owned(), String::new())
    );
    assert_eq!(fetch(address, "/nothing").0, "HTTP/1.1 404 Not Found");

    let poll = thread::spawn(move || fetch(address, "/board?after=7&wait=30"));
    connect(&hub, 0, "working");
    let (status, body) = poll.join().unwrap();
    assert_eq!(status, "HTTP/1.1 200 OK");
    assert_eq!(board(&body).revision, 8);
}

#[test]
fn the_stream_sends_the_board_then_each_change() {
    let hub = Arc::new(hub(&["local"]));
    let address = served(&hub);
    let mut stream = TcpStream::connect(address).unwrap();
    stream.write_all(get("/events").as_bytes()).unwrap();
    let mut lines = BufReader::new(stream).lines().map(Result::unwrap);
    assert_eq!(lines.next().unwrap(), "HTTP/1.1 200 OK");
    let mut event = || {
        let event: Vec<String> = lines
            .by_ref()
            .skip_while(|line| line != "event: board")
            .take_while(|line| !line.is_empty())
            .collect();
        let data = event[2].strip_prefix("data: ").unwrap();
        (event[1].clone(), board(data).revision)
    };
    assert_eq!(event(), ("id: 7".to_owned(), 7));
    connect(&hub, 0, "done");
    assert_eq!(event(), ("id: 8".to_owned(), 8));
}

#![allow(clippy::unwrap_used)]
use super::*;
use std::{io::Write, net::TcpListener};

#[test]
fn local_markdown_is_html_and_other_documents_keep_their_content_type() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("review.md"), "# Review\n\n**Ready**").unwrap();
    let (bytes, kind) = read(dir.path(), "/review.md").unwrap();
    assert_eq!(kind, "text/html; charset=utf-8");
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("<strong>Ready</strong>")
    );
    std::fs::write(dir.path().join("notes.txt"), "# Plain text").unwrap();
    let (bytes, kind) = read(dir.path(), "/notes.txt").unwrap();
    assert_eq!(kind, "text/plain; charset=utf-8");
    assert_eq!(bytes, b"# Plain text");
    let file = std::fs::File::create(dir.path().join("large.md")).unwrap();
    file.set_len(markdown::MAX_BYTES + 1).unwrap();
    assert_eq!(read(dir.path(), "/large.md"), Err(413));
}

#[test]
fn remote_markdown_refetches_on_reload_and_keeps_relative_assets() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        for response in ["# First version", "# Updated version"] {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = [0; 2048];
            let received = stream.read(&mut request).unwrap();
            assert!(received > 0);
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: text/markdown\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}", response.len()).unwrap();
        }
    });
    let url = WebUrl::try_from(format!("http://{address}/review.md?version=1").as_str()).unwrap();
    for expected in ["First version", "Updated version"] {
        let (bytes, kind) = read_markdown(&url).unwrap();
        assert_eq!(kind, "text/html; charset=utf-8");
        let html = String::from_utf8(bytes).unwrap();
        assert!(html.contains(expected));
        assert!(html.contains(&format!("<base href=\"{}\">", url.as_str())));
    }
    server.join().unwrap();
}

#[test]
fn missing_remote_documents_return_a_reloadable_error() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = [0; 2048];
        let received = stream.read(&mut request).unwrap();
        assert!(received > 0);
        stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
            .unwrap();
    });
    let url = WebUrl::try_from(format!("http://{address}/missing.md").as_str()).unwrap();
    assert_eq!(read_markdown(&url), Err(404));
    let html = String::from_utf8(markdown::unavailable(404)).unwrap();
    assert!(html.contains("reload this page"));
    server.join().unwrap();
}

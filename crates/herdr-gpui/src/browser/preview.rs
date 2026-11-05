//! The platform requests previews on the UI thread. File reads and Markdown
//! downloads stay on the bounded worker queue.
use super::{WebUrl, location::requested_file, markdown};
use std::{
    borrow::Cow,
    io::Read,
    path::{Path, PathBuf},
    sync::mpsc::{self, SyncSender, TrySendError},
    thread,
    time::Duration,
};
use wry::{RequestAsyncResponder, http};

/// Requests waiting for the worker; more are answered 503.
const QUEUE: usize = 256;
const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
/// Origins that belong to the preview scheme itself.
const PREVIEW_ORIGINS: [&str; 3] = [
    "herdr-preview://localhost",
    "http://herdr-preview.localhost",
    "https://herdr-preview.localhost",
];

struct Job {
    source: Source,
    responder: RequestAsyncResponder,
}

enum Source {
    File { root: PathBuf, path: String },
    Markdown(WebUrl),
}

pub(crate) struct Preview {
    jobs: SyncSender<Job>,
}

fn respond(responder: RequestAsyncResponder, status: u16, content_type: &str, body: Vec<u8>) {
    let response = http::Response::builder()
        .status(status)
        .header(http::header::CONTENT_TYPE, content_type)
        .header(http::header::CACHE_CONTROL, "no-store")
        .header("X-Content-Type-Options", "nosniff")
        .body(Cow::Owned(body));
    match response {
        Ok(response) => responder.respond(response),
        Err(error) => tracing::debug!(%error, "Preview response failed"),
    }
}

fn content_type(path: &Path) -> &'static str {
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(str::to_ascii_lowercase);
    match extension.as_deref() {
        Some("html" | "htm") => "text/html; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("json" | "map") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("png") => "image/png",
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("ico") => "image/x-icon",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("otf") => "font/otf",
        Some("wasm") => "application/wasm",
        Some("mp4") => "video/mp4",
        Some("webm") => "video/webm",
        Some("txt" | "md") => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Reads `path` under `root`, refusing anything a link leads out of the
/// folder, anything that is not a regular file, and anything too large.
fn read(root: &Path, request_path: &str) -> Result<(Vec<u8>, &'static str), u16> {
    let file = requested_file(root, request_path).ok_or(404_u16)?;
    let root = root.canonicalize().map_err(|_| 404_u16)?;
    let resolved = file.canonicalize().map_err(|_| 404_u16)?;
    if !resolved.starts_with(&root) {
        return Err(403);
    }
    let metadata = resolved.metadata().map_err(|_| 404_u16)?;
    if !metadata.is_file() {
        return Err(404);
    }
    if metadata.len() > MAX_FILE_BYTES {
        return Err(413);
    }
    let markdown = markdown::is_path(&resolved.to_string_lossy());
    if markdown && metadata.len() > markdown::MAX_BYTES {
        return Err(413);
    }
    let bytes = std::fs::read(&resolved).map_err(|_| 404_u16)?;
    if markdown {
        let source = std::str::from_utf8(&bytes).map_err(|_| 415_u16)?;
        let title = resolved
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Document");
        return Ok((
            markdown::render(source, title, None),
            "text/html; charset=utf-8",
        ));
    }
    Ok((bytes, content_type(&resolved)))
}

fn read_markdown(url: &WebUrl) -> Result<(Vec<u8>, &'static str), u16> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .max_redirects(3)
        .http_status_as_error(false)
        .build()
        .into();
    let mut response = agent.get(url.as_str()).call().map_err(|error| {
        tracing::warn!(%error, "Markdown request failed");
        502_u16
    })?;
    let status = response.status().as_u16();
    if !(200..=299).contains(&status) {
        return Err(status);
    }
    let mut bytes = Vec::new();
    response
        .body_mut()
        .as_reader()
        .take(markdown::MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| 502_u16)?;
    if bytes.len() as u64 > markdown::MAX_BYTES {
        return Err(413);
    }
    let source = std::str::from_utf8(&bytes).map_err(|_| 415_u16)?;
    let name = url
        .0
        .path_segments()
        .and_then(Iterator::last)
        .unwrap_or("Document");
    let title = percent_encoding::percent_decode_str(name).decode_utf8_lossy();
    Ok((
        markdown::render(source, &title, Some(url.as_str())),
        "text/html; charset=utf-8",
    ))
}

/// Whether a request comes from a web page rather than from the local page
/// itself or a navigation: a site must not load the folder's files.
fn from_the_web(request: &http::Request<Vec<u8>>) -> bool {
    [http::header::ORIGIN, http::header::REFERER]
        .into_iter()
        .filter_map(|name| request.headers().get(name))
        .filter_map(|value| value.to_str().ok())
        .filter(|value| !value.is_empty() && *value != "null")
        .any(|value| {
            !PREVIEW_ORIGINS.iter().any(|origin| {
                value
                    .strip_prefix(origin)
                    .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
            })
        })
}

impl Preview {
    pub(crate) fn start() -> std::io::Result<Self> {
        let (jobs, queue) = mpsc::sync_channel::<Job>(QUEUE);
        // Ends when the window's pages, and with them the sender, are gone.
        thread::Builder::new()
            .name("browser-preview".into())
            .spawn(move || {
                for job in queue {
                    let result = match job.source {
                        Source::File { root, path } => read(&root, &path),
                        Source::Markdown(url) => read_markdown(&url),
                    };
                    match result {
                        Ok((bytes, kind)) => respond(job.responder, 200, kind, bytes),
                        Err(status) => respond(
                            job.responder,
                            status,
                            "text/html; charset=utf-8",
                            markdown::unavailable(status),
                        ),
                    }
                }
            })?;
        Ok(Self { jobs })
    }

    /// `root` permits folder assets; remote Markdown needs only its source URL.
    pub(crate) fn handle(
        &self,
        root: Option<&Path>,
        request: &http::Request<Vec<u8>>,
        responder: RequestAsyncResponder,
    ) {
        let source = if let Some(url) = markdown::source_url(&request.uri().to_string()) {
            Source::Markdown(url)
        } else {
            let Some(root) = root.filter(|_| !from_the_web(request)) else {
                respond(responder, 403, "text/plain", Vec::new());
                return;
            };
            Source::File {
                root: root.to_owned(),
                path: request.uri().path().to_owned(),
            }
        };
        let job = Job { source, responder };
        if let Err(TrySendError::Full(job) | TrySendError::Disconnected(job)) =
            self.jobs.try_send(job)
        {
            respond(job.responder, 503, "text/plain", Vec::new());
        }
    }
}

#[cfg(test)]
mod markdown_tests;

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn files_are_read_only_from_inside_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let site = dir.path().join("site");
        std::fs::create_dir_all(site.join("css")).unwrap();
        std::fs::write(site.join("index.html"), "<h1>hi</h1>").unwrap();
        std::fs::write(site.join("css/app.css"), "h1{}").unwrap();
        std::fs::write(dir.path().join("secret.txt"), "no").unwrap();
        let (bytes, kind) = read(&site, "/").unwrap();
        assert_eq!(
            (bytes.as_slice(), kind),
            (&b"<h1>hi</h1>"[..], "text/html; charset=utf-8")
        );
        assert_eq!(
            read(&site, "/css/app.css").unwrap().1,
            "text/css; charset=utf-8"
        );
        assert_eq!(read(&site, "/missing.html"), Err(404));
        assert_eq!(read(&site, "/../secret.txt"), Err(404));
        assert_eq!(read(&site, "/css"), Err(404));
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(dir.path().join("secret.txt"), site.join("link.txt"))
                .unwrap();
            assert_eq!(read(&site, "/link.txt"), Err(403));
        }
    }

    #[test]
    fn web_pages_cannot_load_local_files() {
        let request = |header: Option<(&str, &str)>| {
            let mut builder = http::Request::builder().uri("herdr-preview://localhost/a.js");
            if let Some((name, value)) = header {
                builder = builder.header(name, value);
            }
            builder.body(Vec::new()).unwrap()
        };
        assert!(!from_the_web(&request(None)));
        assert!(!from_the_web(&request(Some((
            "Referer",
            "herdr-preview://localhost/index.html"
        )))));
        assert!(!from_the_web(&request(Some((
            "Origin",
            "http://herdr-preview.localhost"
        )))));
        assert!(!from_the_web(&request(Some(("Origin", "null")))));
        assert!(from_the_web(&request(Some((
            "Referer",
            "https://evil.test/"
        )))));
        assert!(from_the_web(&request(Some((
            "Origin",
            "https://evil.test"
        )))));
        assert!(from_the_web(&request(Some((
            "Origin",
            "http://herdr-preview.localhost.evil.test"
        )))));
    }
}

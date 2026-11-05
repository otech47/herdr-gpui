use super::{WebUrl, location::PREVIEW_ORIGINS};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd, html};
use std::collections::HashMap;

#[cfg(any(target_os = "macos", windows))]
pub(super) const MAX_BYTES: u64 = 2 * 1024 * 1024;
const ROUTE: &str = "_markdown";

pub(super) fn is_path(path: &str) -> bool {
    path.rsplit('.').next().is_some_and(|extension| {
        extension.eq_ignore_ascii_case("md") || extension.eq_ignore_ascii_case("markdown")
    })
}

pub(super) fn is_url(url: &WebUrl) -> bool {
    is_path(url.0.path())
}

pub(super) fn page_url(url: &WebUrl) -> String {
    if !is_url(url) {
        return url.as_str().to_owned();
    }
    let mut source = url.0.clone();
    source.set_fragment(None);
    let encoded =
        percent_encoding::utf8_percent_encode(source.as_str(), percent_encoding::NON_ALPHANUMERIC);
    let mut page = format!("{}{ROUTE}?source={encoded}", PREVIEW_ORIGINS[0]);
    if let Some(fragment) = url.0.fragment() {
        page.push('#');
        page.push_str(fragment);
    }
    page
}

pub(super) fn source_url(page: &str) -> Option<WebUrl> {
    let relative = PREVIEW_ORIGINS
        .iter()
        .find_map(|origin| page.strip_prefix(origin))?;
    if relative.split(['?', '#']).next()? != ROUTE {
        return None;
    }
    let parsed = url::Url::parse(page).ok()?;
    let value = parsed.query_pairs().find(|(key, _)| key == "source")?.1;
    let mut source = WebUrl::try_from(value.as_ref()).ok()?;
    if !is_url(&source) {
        return None;
    }
    source.0.set_fragment(parsed.fragment());
    Some(source)
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn safe_link(value: &str) -> bool {
    match url::Url::parse(value) {
        Ok(url) => matches!(url.scheme(), "http" | "https" | "mailto"),
        Err(url::ParseError::RelativeUrlWithoutBase) => true,
        Err(_) => false,
    }
}

fn headings(events: &mut [Event<'_>]) {
    let mut seen = HashMap::<String, usize>::new();
    let ids: Vec<_> = events
        .iter()
        .enumerate()
        .filter_map(|(index, event)| {
            if !matches!(event, Event::Start(Tag::Heading { .. })) {
                return None;
            }
            let text: String = events
                .iter()
                .skip(index + 1)
                .take_while(|event| !matches!(event, Event::End(TagEnd::Heading(_))))
                .filter_map(|event| match event {
                    Event::Text(text) | Event::Code(text) => Some(text.as_ref()),
                    Event::SoftBreak | Event::HardBreak => Some(" "),
                    _ => None,
                })
                .collect();
            let slug: String = text
                .to_lowercase()
                .chars()
                .filter_map(|ch| {
                    if ch.is_whitespace() {
                        Some('-')
                    } else if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                        Some(ch)
                    } else {
                        None
                    }
                })
                .collect();
            let slug = if slug.is_empty() {
                "section".to_owned()
            } else {
                slug
            };
            let count = seen.entry(slug.clone()).or_default();
            let id = if *count == 0 {
                slug
            } else {
                format!("{slug}-{count}")
            };
            *count += 1;
            Some((index, id))
        })
        .collect();
    for (index, heading) in ids {
        if let Some(Event::Start(Tag::Heading { id, .. })) = events.get_mut(index) {
            *id = Some(heading.into());
        }
    }
}

fn document(title: &str, base: Option<&str>, body: &str) -> Vec<u8> {
    let base = base
        .map(|url| format!("<base href=\"{}\">", escape(url)))
        .unwrap_or_default();
    format!(r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src http: https: herdr-preview:; style-src 'unsafe-inline'; base-uri *">
<title>{}</title>{base}<style>{}</style></head><body><main class="markdown-body">{body}</main></body></html>"#,
        escape(title), include_str!("markdown.css")).into_bytes()
}

pub(super) fn render(source: &str, title: &str, base: Option<&str>) -> Vec<u8> {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_TASKLISTS | Options::ENABLE_STRIKETHROUGH;
    let mut events: Vec<_> = Parser::new_ext(source, options)
        .map(|event| match event {
            // Markdown is a document, so embedded HTML cannot execute page code.
            Event::Html(text) | Event::InlineHtml(text) => Event::Text(text),
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => Event::Start(Tag::Link {
                link_type,
                dest_url: if safe_link(&dest_url) {
                    dest_url
                } else {
                    "#".into()
                },
                title,
                id,
            }),
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => Event::Start(Tag::Image {
                link_type,
                dest_url: if safe_link(&dest_url) {
                    dest_url
                } else {
                    "#".into()
                },
                title,
                id,
            }),
            other => other,
        })
        .collect();
    headings(&mut events);
    let mut body = String::new();
    html::push_html(&mut body, events.into_iter());
    document(title, base, &body)
}

#[cfg(any(target_os = "macos", windows))]
pub(super) fn unavailable(status: u16) -> Vec<u8> {
    let message = if status == 413 {
        "This document is too large to preview. Open it in your browser to read it."
    } else {
        "Check that the source is available, then reload this page."
    };
    document(
        "Document unavailable",
        None,
        &format!("<h1>Could not open this document</h1><p>{message}</p>"),
    )
}

#[cfg(test)]
mod tests;

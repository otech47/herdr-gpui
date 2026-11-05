#![allow(clippy::unwrap_used)]
use super::*;
use crate::browser::Location;

#[test]
fn rendered_documents_keep_structure_links_and_unique_anchors() {
    let source = "# Review\n\n## Next steps\n\n- [x] **Checked**\n- [ ] Pending\n\n| Task | State |\n| --- | --- |\n| Report | Ready |\n\n```rust\nlet x = 1 < 2;\n```\n\n[Details](details.md)\n\n![Chart](chart.png)\n\n## Next steps\n";
    let html = String::from_utf8(render(
        source,
        "review.md",
        Some("http://host.test/reports/review.md"),
    ))
    .unwrap();
    for expected in [
        "<h1 id=\"review\">Review</h1>",
        "id=\"next-steps\"",
        "id=\"next-steps-1\"",
        "<table>",
        "type=\"checkbox\"",
        "<strong>Checked</strong>",
        "class=\"language-rust\"",
        "1 &lt; 2",
        "href=\"details.md\"",
        "src=\"chart.png\"",
        "<base href=\"http://host.test/reports/review.md\">",
    ] {
        assert!(html.contains(expected), "missing {expected}");
    }
}

#[test]
fn markdown_cannot_execute_html_or_script_links() {
    let html = String::from_utf8(render("<script>alert(1)</script>\n\n[run](javascript:alert%281%29)\n\n![image](data:text/html,attack)", "<script>title</script>", None)).unwrap();
    assert!(!html.contains("<script>"));
    assert!(!html.contains("href=\"javascript:"));
    assert!(!html.contains("src=\"data:"));
    assert!(html.contains("&lt;script&gt;"));
    assert!(html.contains("default-src 'none'"));
}

#[test]
fn private_preview_addresses_round_trip_the_original_link() {
    let url =
        WebUrl::try_from("http://host.tail123.ts.net:8788/review%20one.MD?version=2#next-steps")
            .unwrap();
    let preview = page_url(&url);
    assert_eq!(source_url(&preview), Some(url.clone()));
    let windows = preview.replacen(
        "herdr-preview://localhost",
        "http://herdr-preview.localhost",
        1,
    );
    assert_eq!(source_url(&windows), Some(url.clone()));
    let location = Location::Web { url: url.clone() };
    assert_eq!(location.visited(&preview), Some(location));
    assert!(
        source_url("https://evil.test/_markdown?source=http%3A%2F%2Flocalhost%2Fsecret.md")
            .is_none()
    );
    assert!(
        source_url("herdr-preview://localhost/_markdown?source=file%3A%2F%2F%2Fsecret.md")
            .is_none()
    );
    let ordinary = WebUrl::try_from("https://example.test/report.html").unwrap();
    assert_eq!(page_url(&ordinary), ordinary.as_str());
}

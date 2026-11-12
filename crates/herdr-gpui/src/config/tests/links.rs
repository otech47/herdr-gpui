use super::*;

#[test]
fn links_open_a_web_page_with_an_icon_and_hover_text() -> anyhow::Result<()> {
    let config = Config::parse(
        "[[links]]\nlabel = 'otech'\nurl = ' http://127.0.0.1:7477 '\nicon = 'pulse'\n\
         [[links]]\nurl = 'https://example.com'\n",
    )?;
    assert_eq!(
        config.links,
        [
            Link {
                label: "otech".into(),
                url: "http://127.0.0.1:7477".into(),
                icon: "icons/pulse.svg".into(),
            },
            Link {
                label: "https://example.com".into(),
                url: "https://example.com".into(),
                icon: "icons/external.svg".into(),
            },
        ]
    );
    assert!(Config::parse("")?.links.is_empty());
    Ok(())
}

#[test]
fn a_link_needs_a_web_address_and_a_known_icon() {
    for (text, why) in [
        ("[[links]]\nurl = 'file:///etc/hosts'\n", "not a web page"),
        ("[[links]]\nurl = ''\n", "no address"),
        (
            "[[links]]\nurl = 'https://example.com'\nicon = 'rocket'\n",
            "unknown icon",
        ),
    ] {
        assert!(Config::parse(text).is_err(), "{why}");
    }
    let many = "[[links]]\nurl = 'https://example.com'\n".repeat(9);
    assert!(Config::parse(&many).is_err(), "too many");
}

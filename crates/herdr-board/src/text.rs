//! Display text taken from daemon metadata: the first value that says
//! something, and untrusted labels bounded and stripped before they are shown.

pub fn first_text<'a>(
    values: impl IntoIterator<Item = Option<&'a str>>,
    fallback: &'a str,
) -> &'a str {
    values
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|s| !s.is_empty())
        .unwrap_or(fallback)
}

pub fn safe_text(text: &str, limit: usize) -> String {
    // Bound scanning as well as output, even for a payload made entirely of controls.
    text.chars().take(limit).filter(|c| {
        !c.is_control() && !matches!(*c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}' | '\u{200e}' | '\u{200f}' | '\u{061c}' | '\u{2028}' | '\u{2029}')
    }).collect()
}

/// A tag keeps its full width while the title truncates, so long space and
/// device names are cut short to leave the title room.
pub(crate) fn clipped(name: &str) -> String {
    const MOST: usize = 16;
    if name.chars().count() <= MOST {
        return name.to_owned();
    }
    let mut clipped: String = name.chars().take(MOST - 1).collect();
    clipped.push('\u{2026}');
    clipped
}

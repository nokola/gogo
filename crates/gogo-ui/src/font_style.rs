//! The style suffix a font key may carry: `"Angelina#bold"`. Needed so a
//! callout's bold/italic reach the renderer through the same `font=`
//! word every label already has — the app spells the key, the text
//! engine reads the suffix (`text::TextEngine`), and neither depends on
//! the other for it. Faux styles by design: no shipped face carries real
//! bold or italic (shipped FontLoader.MakeStyled emboldened and skewed
//! any TTF the same way).

pub const BOLD_SUFFIX: &str = "#bold";
pub const ITALIC_SUFFIX: &str = "#italic";
pub const BOLD_ITALIC_SUFFIX: &str = "#bold-italic";

/// A font key with its faux style spelled on the end; the bare key when
/// neither is asked for.
pub fn styled_key(key: &str, bold: bool, italic: bool) -> String {
    match (bold, italic) {
        (false, false) => key.to_string(),
        (true, false) => format!("{key}{BOLD_SUFFIX}"),
        (false, true) => format!("{key}{ITALIC_SUFFIX}"),
        (true, true) => format!("{key}{BOLD_ITALIC_SUFFIX}"),
    }
}

/// `styled_key` undone: the face key, bold, italic.
pub fn split_style(key: &str) -> (&str, bool, bool) {
    if let Some(k) = key.strip_suffix(BOLD_ITALIC_SUFFIX) {
        (k, true, true)
    } else if let Some(k) = key.strip_suffix(BOLD_SUFFIX) {
        (k, true, false)
    } else if let Some(k) = key.strip_suffix(ITALIC_SUFFIX) {
        (k, false, true)
    } else {
        (key, false, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_style_rides_the_key_and_comes_back_off() {
        assert_eq!(styled_key("A", false, false), "A");
        assert_eq!(styled_key("A", true, false), "A#bold");
        assert_eq!(styled_key("A", false, true), "A#italic");
        assert_eq!(styled_key("A", true, true), "A#bold-italic");
        for (b, i) in [(false, false), (true, false), (false, true), (true, true)] {
            assert_eq!(split_style(&styled_key("res://x.ttf", b, i)), ("res://x.ttf", b, i));
        }
    }
}

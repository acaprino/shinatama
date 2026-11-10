//! Name normalization for card items (FRD-0012 R52-R53).
//!
//! Decision: a compatibility fold instead of full Unicode NFKC. Fullwidth ASCII forms
//! (U+FF01-U+FF5E) become ASCII, every Unicode space becomes a plain space, then the text is
//! trimmed and runs of spaces collapse. Full NFKC would need a normalization table crate that
//! is not in the locked dependency graph; other compatibility characters (ligatures, circled
//! digits, superscripts) are therefore NOT folded, and the allowed-character rule below then
//! rejects most of them. The limit is declared, not hidden.

pub fn fold(text: &str) -> String {
    let mapped: String = text
        .chars()
        .map(|c| match c {
            '\u{FF01}'..='\u{FF5E}' => char::from_u32(c as u32 - 0xFEE0).unwrap_or(c),
            c if c.is_whitespace() => ' ',
            c => c,
        })
        .collect();
    let mut out = String::with_capacity(mapped.len());
    for word in mapped.split(' ').filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
}

/// Letters, digits, spaces and `. - _ / + #`, at most `max` characters after folding.
pub fn valid_name(folded: &str, max: usize) -> bool {
    !folded.is_empty()
        && folded.chars().count() <= max
        && folded.chars().all(|c| c.is_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_' | '/' | '+' | '#'))
}

/// A concept's identity: folded, case-folded, spaces collapsed; no aliases (R53).
pub fn identity(name: &str) -> String {
    fold(name).to_lowercase()
}

/// Bytes cut on a character boundary; `keep_end` keeps the tail (where a question usually is).
pub fn cap(text: &str, max: usize, keep_end: bool) -> (String, bool) {
    if text.len() <= max {
        return (text.to_owned(), false);
    }
    if keep_end {
        let mut start = text.len() - max;
        while !text.is_char_boundary(start) {
            start += 1;
        }
        (text[start..].to_owned(), true)
    } else {
        let mut end = max;
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        (text[..end].to_owned(), true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_fullwidth_and_spaces() {
        assert_eq!(fold("  Ｒｕｓｔ\u{3000}\u{00A0}borrow   checker "), "Rust borrow checker");
        assert_eq!(identity("Borrow  CHECKER"), identity("borrow checker"));
        assert!(valid_name("C++ / C#", 60));
        assert!(valid_name("città", 60));
        assert!(!valid_name("a<b>", 60) && !valid_name("x\u{202E}y", 60) && !valid_name("", 60));
        assert!(!valid_name(&"a".repeat(61), 60));
    }

    #[test]
    fn caps_on_boundaries() {
        assert_eq!(cap("ééé", 3, false), ("é".into(), true));
        assert_eq!(cap("ééé", 3, true), ("é".into(), true));
        assert_eq!(cap("ok", 3, true), ("ok".into(), false));
    }
}

//! Vietnamese text handling: NFC at the boundary (AC2.7) and an accent-folded search key (AC2.1),
//! so "hà", "Hà", "ha" and a decomposed "Hà" typed on another keyboard all find the same customer.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

/// Trimmed, single-spaced, NFC-normalised display name.
pub fn clean_name(input: &str) -> String {
    input
        .nfc()
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Lowercase, diacritics removed, `đ`→`d`: the value stored in `customer.search_text`.
pub fn search_key(input: &str) -> String {
    input
        .nfd()
        .filter(|c| !is_combining_mark(*c))
        .map(|c| match c {
            'đ' | 'Đ' => 'd',
            other => other,
        })
        .flat_map(char::to_lowercase)
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Escapes `%`, `_` and `\` for use inside a SQL `LIKE` pattern.
pub fn like_escape(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for c in input.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_vietnamese_for_search() {
        assert_eq!(search_key("Nguyễn Thị Hà"), "nguyen thi ha");
        assert_eq!(search_key("ĐỖ ĐỨC"), "do duc");
        assert_eq!(search_key("  Hoàng   Thị  Thu Hằng "), "hoang thi thu hang");
    }

    #[test]
    fn composed_and_decomposed_input_are_the_same_name() {
        let composed = "H\u{00E0}"; // Hà, precomposed
        let decomposed = "Ha\u{0300}"; // H + a + combining grave
        assert_ne!(composed, decomposed);
        assert_eq!(clean_name(decomposed), composed);
        assert_eq!(search_key(composed), search_key(decomposed));
    }

    #[test]
    fn cleans_spacing_and_keeps_accents_for_display() {
        assert_eq!(clean_name("  Lê   Thị  Ánh "), "Lê Thị Ánh");
    }

    #[test]
    fn escapes_like_wildcards() {
        assert_eq!(like_escape("50%_off\\"), "50\\%\\_off\\\\");
    }
}

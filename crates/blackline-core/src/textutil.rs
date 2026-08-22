//! Text primitives shared by search and the redline / tracked-change engines.

/// Collapse Unicode smart quotes, dashes, and non-breaking spaces to ASCII
/// so literal searches match regardless of typography.
pub fn normalize_quotes(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{201F}' => '"',
            '\u{2013}' | '\u{2014}' => '-',
            '\u{00A0}' => ' ',
            other => other,
        })
        .collect()
}

/// Case-fold for comparison. Uses Unicode lowercase (not just ASCII).
pub fn fold_case(s: &str) -> String {
    s.to_lowercase()
}

/// Find `needle` in `haystack` after smart-quote normalization.
/// Returns the matched slice of `haystack` (original characters) and its
/// byte offset, or `None`.
pub fn find_normalized(haystack: &str, needle: &str) -> Option<(usize, String)> {
    if needle.is_empty() {
        return Some((0, String::new()));
    }
    let h_norm = normalize_quotes(haystack);
    let n_norm = normalize_quotes(needle);
    let pos = h_norm.find(&n_norm)?;
    // Map normalized byte offset back by walking original chars. Because
    // normalize_quotes is 1:1 on chars, char offsets match.
    let char_pos = h_norm[..pos].chars().count();
    let char_len = n_norm.chars().count();
    let start = haystack.char_indices().nth(char_pos)?.0;
    let end = haystack
        .char_indices()
        .nth(char_pos + char_len)
        .map(|(i, _)| i)
        .unwrap_or(haystack.len());
    Some((start, haystack[start..end].to_string()))
}

/// Case-insensitive variant of [`find_normalized`].
pub fn find_normalized_ci(haystack: &str, needle: &str) -> Option<(usize, String)> {
    if needle.is_empty() {
        return Some((0, String::new()));
    }
    let h_norm = fold_case(&normalize_quotes(haystack));
    let n_norm = fold_case(&normalize_quotes(needle));
    let pos = h_norm.find(&n_norm)?;
    let char_pos = h_norm[..pos].chars().count();
    let char_len = n_norm.chars().count();
    let start = haystack.char_indices().nth(char_pos)?.0;
    let end = haystack
        .char_indices()
        .nth(char_pos + char_len)
        .map(|(i, _)| i)
        .unwrap_or(haystack.len());
    Some((start, haystack[start..end].to_string()))
}

/// Tokenize text for word-level diffing.
///
/// Tokens are maximal runs of non-whitespace characters with any *leading*
/// whitespace attached. Concatenating the returned tokens reproduces the
/// input exactly.
pub fn tokenize_words(text: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut token_start = 0usize;
    let mut seen_word = false;
    let mut prev_was_ws = false;

    for (pos, c) in text.char_indices() {
        let is_ws = c.is_whitespace();
        if is_ws && !prev_was_ws && seen_word {
            tokens.push(&text[token_start..pos]);
            token_start = pos;
            seen_word = false;
        }
        if !is_ws {
            seen_word = true;
        }
        prev_was_ws = is_ws;
    }
    if token_start < text.len() {
        tokens.push(&text[token_start..]);
    }
    tokens
}

/// Common abbreviations that end with a period but do not end a sentence.
const ABBREVIATIONS: &[&str] = &[
    "no", "nos", "u.s", "u.s.a", "inc", "corp", "co", "ltd", "llc", "l.p", "llp", "sec", "art",
    "para", "e.g", "i.e", "cf", "v", "vs", "etc", "mr", "mrs", "ms", "dr", "jr", "sr", "st", "rev",
    "dept", "approx",
];

/// Split text into sentences. Trailing whitespace stays attached so the
/// segments concatenate back to the original text.
pub fn split_sentences(text: &str) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut sentences = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < chars.len() {
        let (pos, c) = chars[i];
        if matches!(c, '.' | '!' | '?' | ';') {
            let next_ws = chars
                .get(i + 1)
                .map(|&(_, n)| n.is_whitespace())
                .unwrap_or(false);
            let looks_like_boundary = next_ws
                && upcoming_starts_sentence(&chars, i + 1)
                && !(c == '.' && is_abbreviation(text, pos));
            if looks_like_boundary {
                let mut j = i + 1;
                while j < chars.len() && chars[j].1.is_whitespace() {
                    j += 1;
                }
                let end = chars.get(j).map(|&(p, _)| p).unwrap_or(text.len());
                sentences.push(&text[start..end]);
                start = end;
                i = j;
                continue;
            }
        }
        i += 1;
    }
    if start < text.len() {
        sentences.push(&text[start..]);
    }
    if sentences.is_empty() && !text.is_empty() {
        sentences.push(text);
    }
    sentences
}

fn upcoming_starts_sentence(chars: &[(usize, char)], idx: usize) -> bool {
    for &(_, c) in &chars[idx..] {
        if c.is_whitespace() {
            continue;
        }
        return c.is_uppercase()
            || c.is_ascii_digit()
            || matches!(c, '"' | '\'' | '(' | '[' | '\u{201C}' | '\u{2018}');
    }
    false
}

fn is_abbreviation(text: &str, dot_pos: usize) -> bool {
    let before = &text[..dot_pos];
    let word_start = before
        .rfind(|c: char| c.is_whitespace() || matches!(c, '(' | '[' | '"' | '\''))
        .map(|p| p + 1)
        .unwrap_or(0);
    let word = &before[word_start..];
    if word.is_empty() {
        return false;
    }
    if word.chars().count() == 1 && word.chars().next().unwrap().is_alphabetic() {
        return true;
    }
    ABBREVIATIONS.contains(&word.to_lowercase().as_str())
}

/// Whole-word test: `needle` is bounded by non-alphanumeric characters.
pub fn is_whole_word(haystack: &str, start: usize, len: usize) -> bool {
    let before_ok = start == 0
        || haystack[..start]
            .chars()
            .next_back()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
    let after = start + len;
    let after_ok = after >= haystack.len()
        || haystack[after..]
            .chars()
            .next()
            .map(|c| !c.is_alphanumeric())
            .unwrap_or(true);
    before_ok && after_ok
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_normalize() {
        assert_eq!(normalize_quotes("\u{201c}hi\u{201d}"), "\"hi\"");
        assert_eq!(normalize_quotes("it\u{2019}s"), "it's");
    }

    #[test]
    fn find_smart_quotes() {
        let hay = "The \u{201c}Purchase Price\u{201d} is due.";
        let (pos, matched) = find_normalized(hay, "\"Purchase Price\"").unwrap();
        assert_eq!(pos, 4);
        assert_eq!(matched, "\u{201c}Purchase Price\u{201d}");
    }

    #[test]
    fn tokenize_roundtrip() {
        let t = "a  b, c";
        let tokens = tokenize_words(t);
        assert_eq!(tokens, vec!["a", "  b,", " c"]);
        assert_eq!(tokens.concat(), t);
    }

    #[test]
    fn sentences_keep_ws() {
        let s = split_sentences("The U.S. company paid $5.00. It was reimbursed; No. 3 remains.");
        assert_eq!(s.len(), 3);
        assert_eq!(
            s.concat(),
            "The U.S. company paid $5.00. It was reimbursed; No. 3 remains."
        );
    }

    #[test]
    fn whole_word() {
        assert!(is_whole_word("the cat sat", 4, 3));
        assert!(!is_whole_word("the catalog", 4, 3));
    }
}

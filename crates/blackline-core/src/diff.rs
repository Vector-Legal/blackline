//! Token-level LCS diff. Used by tracked replacements and two-document redlines.
//!
//! Implemented here so the toolkit does not depend on an external diff crate.

use crate::textutil::{normalize_quotes, split_sentences, tokenize_words};

/// How tightly a tracked replacement / redline is chunked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Granularity {
    /// Character-level (tightest; can split words).
    Char,
    /// Word-level. Default.
    #[default]
    Word,
    /// Whole changed sentences become one deletion + insertion.
    Sentence,
}

impl Granularity {
    /// Parse `char` / `word` / `sentence`.
    pub fn parse(s: &str) -> Result<Self, String> {
        match s.to_ascii_lowercase().as_str() {
            "char" | "character" => Ok(Self::Char),
            "word" => Ok(Self::Word),
            "sentence" => Ok(Self::Sentence),
            other => Err(format!(
                "unknown granularity '{other}' (expected char, word, or sentence)"
            )),
        }
    }
}

impl std::str::FromStr for Granularity {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

/// One hunk of a diff. Concatenating Equal + Delete from `old` and
/// Equal + Insert from `new` reconstructs the respective strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffHunk {
    /// Unchanged text.
    Equal(String),
    /// Deleted text (present only in the original).
    Delete(String),
    /// Inserted text (present only in the revision).
    Insert(String),
}

/// Render a two-sided inline redline (`[-deleted-]` / `{+inserted+}`).
///
/// Word uses `w:ins` / `w:del`. Spreadsheet cells and slide frames have
/// no equivalent, so format redlines compile the same LCS hunks to this
/// mark-up instead.
pub fn mark(old: &str, new: &str, granularity: Granularity) -> String {
    if old == new {
        return old.to_string();
    }
    let mut out = String::new();
    for hunk in diff(old, new, granularity) {
        match hunk {
            DiffHunk::Equal(s) => out.push_str(&s),
            DiffHunk::Delete(s) => {
                out.push_str("[-");
                out.push_str(&s);
                out.push_str("-]");
            }
            DiffHunk::Insert(s) => {
                out.push_str("{+");
                out.push_str(&s);
                out.push_str("+}");
            }
        }
    }
    out
}

/// Render a two-sided inline redline using [`diff_minimal`].
///
/// Prefer this for track / surgical redline: identical delete+insert
/// pairs cancel, and adjacent replacements keep shared prefix/suffix
/// as [`DiffHunk::Equal`].
pub fn mark_minimal(old: &str, new: &str, granularity: Granularity) -> String {
    if old == new {
        return old.to_string();
    }
    let mut out = String::new();
    for hunk in diff_minimal(old, new, granularity) {
        match hunk {
            DiffHunk::Equal(s) => out.push_str(&s),
            DiffHunk::Delete(s) => {
                out.push_str("[-");
                out.push_str(&s);
                out.push_str("-]");
            }
            DiffHunk::Insert(s) => {
                out.push_str("{+");
                out.push_str(&s);
                out.push_str("+}");
            }
        }
    }
    out
}

/// Diff `old` against `new` at `granularity`, then minimize.
///
/// Word-level [`diff`] folds short equal spans into the surrounding
/// delete+insert (so markup stays on word boundaries). That can mark
/// text as deleted and immediately re-inserted. This pass:
///
/// 1. Cancels a delete+insert whose normalized text is identical
///    (the add is the same as the delete — a no-op).
/// 2. Otherwise re-diffs that pair at character granularity so shared
///    prefix/suffix is not marked.
pub fn diff_minimal(old: &str, new: &str, granularity: Granularity) -> Vec<DiffHunk> {
    minimize_hunks(&diff(old, new, granularity))
}

/// Refine LCS hunks so we do not delete text we immediately add back.
pub fn minimize_hunks(hunks: &[DiffHunk]) -> Vec<DiffHunk> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < hunks.len() {
        match (&hunks[i], hunks.get(i + 1)) {
            (DiffHunk::Delete(deleted), Some(DiffHunk::Insert(inserted))) => {
                out.extend(refine_pair(deleted, inserted));
                i += 2;
            }
            (DiffHunk::Insert(inserted), Some(DiffHunk::Delete(deleted))) => {
                out.extend(refine_pair(deleted, inserted));
                i += 2;
            }
            (hunk, _) => {
                out.push(hunk.clone());
                i += 1;
            }
        }
    }
    coalesce(out)
}

fn refine_pair(deleted: &str, inserted: &str) -> Vec<DiffHunk> {
    if deleted.is_empty() && inserted.is_empty() {
        return Vec::new();
    }
    if normalize_quotes(deleted) == normalize_quotes(inserted) {
        return if deleted.is_empty() {
            Vec::new()
        } else {
            vec![DiffHunk::Equal(deleted.to_string())]
        };
    }
    if deleted.is_empty() {
        return vec![DiffHunk::Insert(inserted.to_string())];
    }
    if inserted.is_empty() {
        return vec![DiffHunk::Delete(deleted.to_string())];
    }
    diff(deleted, inserted, Granularity::Char)
}

/// Diff `old` against `new` at `granularity`. Adjacent hunks of the same
/// kind are coalesced. At word granularity, changed regions separated by
/// at most two equal tokens are coalesced to avoid noisy mid-word markup.
pub fn diff(old: &str, new: &str, granularity: Granularity) -> Vec<DiffHunk> {
    if old == new {
        return if old.is_empty() {
            Vec::new()
        } else {
            vec![DiffHunk::Equal(old.to_string())]
        };
    }
    let hunks = match granularity {
        Granularity::Char => {
            let old_t: Vec<String> = old.chars().map(|c| c.to_string()).collect();
            let new_t: Vec<String> = new.chars().map(|c| c.to_string()).collect();
            lcs_hunks(&old_t, &new_t)
        }
        Granularity::Word => {
            let old_t: Vec<String> = tokenize_words(old)
                .into_iter()
                .map(str::to_string)
                .collect();
            let new_t: Vec<String> = tokenize_words(new)
                .into_iter()
                .map(str::to_string)
                .collect();
            let raw = lcs_hunks(&old_t, &new_t);
            coalesce_near(raw, 2)
        }
        Granularity::Sentence => {
            let old_t: Vec<String> = split_sentences(old)
                .into_iter()
                .map(str::to_string)
                .collect();
            let new_t: Vec<String> = split_sentences(new)
                .into_iter()
                .map(str::to_string)
                .collect();
            lcs_hunks(&old_t, &new_t)
        }
    };
    coalesce(hunks)
}

fn lcs_hunks(old: &[String], new: &[String]) -> Vec<DiffHunk> {
    let n = old.len();
    let m = new.len();
    // DP table of LCS lengths. n,m are paragraph-scale.
    let mut dp = vec![vec![0u32; m + 1]; n + 1];
    for i in 0..n {
        for j in 0..m {
            dp[i + 1][j + 1] = if old[i] == new[j] {
                dp[i][j] + 1
            } else {
                dp[i][j + 1].max(dp[i + 1][j])
            };
        }
    }
    let mut hunks = Vec::new();
    let mut i = n;
    let mut j = m;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && old[i - 1] == new[j - 1] {
            hunks.push(DiffHunk::Equal(old[i - 1].clone()));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || dp[i][j - 1] >= dp[i - 1][j]) {
            hunks.push(DiffHunk::Insert(new[j - 1].clone()));
            j -= 1;
        } else {
            hunks.push(DiffHunk::Delete(old[i - 1].clone()));
            i -= 1;
        }
    }
    hunks.reverse();
    hunks
}

fn coalesce(hunks: Vec<DiffHunk>) -> Vec<DiffHunk> {
    let mut out: Vec<DiffHunk> = Vec::new();
    for h in hunks {
        match (out.last_mut(), &h) {
            (Some(DiffHunk::Equal(a)), DiffHunk::Equal(b)) => a.push_str(b),
            (Some(DiffHunk::Delete(a)), DiffHunk::Delete(b)) => a.push_str(b),
            (Some(DiffHunk::Insert(a)), DiffHunk::Insert(b)) => a.push_str(b),
            _ => out.push(h),
        }
    }
    out
}

fn coalesce_near(hunks: Vec<DiffHunk>, max_equal: usize) -> Vec<DiffHunk> {
    // If an Equal of at most `max_equal` tokens sits between two change
    // regions, fold it into the surrounding changes (delete+insert the equal
    // text on both sides). Token count is approximated by whitespace splits.
    let mut out = Vec::new();
    let mut i = 0;
    while i < hunks.len() {
        if let DiffHunk::Equal(eq) = &hunks[i] {
            let tokens = tokenize_words(eq).len();
            let prev_change = i > 0 && !matches!(hunks[i - 1], DiffHunk::Equal(_));
            let next_change = i + 1 < hunks.len() && !matches!(hunks[i + 1], DiffHunk::Equal(_));
            if prev_change && next_change && tokens > 0 && tokens <= max_equal {
                out.push(DiffHunk::Delete(eq.clone()));
                out.push(DiffHunk::Insert(eq.clone()));
                i += 1;
                continue;
            }
        }
        out.push(hunks[i].clone());
        i += 1;
    }
    coalesce(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical() {
        let d = diff("hello", "hello", Granularity::Word);
        assert_eq!(d, vec![DiffHunk::Equal("hello".into())]);
    }

    #[test]
    fn word_replace() {
        let d = diff("the cat sat", "the dog sat", Granularity::Word);
        assert!(d
            .iter()
            .any(|h| matches!(h, DiffHunk::Delete(s) if s.contains("cat"))));
        assert!(d
            .iter()
            .any(|h| matches!(h, DiffHunk::Insert(s) if s.contains("dog"))));
        let old: String = d
            .iter()
            .filter_map(|h| match h {
                DiffHunk::Equal(s) | DiffHunk::Delete(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        let new: String = d
            .iter()
            .filter_map(|h| match h {
                DiffHunk::Equal(s) | DiffHunk::Insert(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(old, "the cat sat");
        assert_eq!(new, "the dog sat");
    }

    #[test]
    fn char_split() {
        let d = diff("ab", "ac", Granularity::Char);
        assert_eq!(
            d,
            vec![
                DiffHunk::Equal("a".into()),
                DiffHunk::Delete("b".into()),
                DiffHunk::Insert("c".into()),
            ]
        );
    }

    #[test]
    fn append_is_pure_insert() {
        let d = diff("hello", "hello world", Granularity::Word);
        assert!(matches!(d.last(), Some(DiffHunk::Insert(s)) if s.contains("world")));
        assert!(!d.iter().any(|h| matches!(h, DiffHunk::Delete(_))));
    }
    #[test]
    fn mark_shows_both_sides() {
        assert_eq!(
            mark("the cat", "the dog", Granularity::Word),
            "the[- cat-]{+ dog+}"
        );
        assert_eq!(mark("same", "same", Granularity::Word), "same");
        assert_eq!(mark("", "hi", Granularity::Word), "{+hi+}");
        assert_eq!(mark("bye", "", Granularity::Word), "[-bye-]");
    }

    #[test]
    fn minimize_cancels_identical_delete_insert() {
        let hunks = vec![
            DiffHunk::Equal("the".into()),
            DiffHunk::Delete(" x".into()),
            DiffHunk::Insert(" x".into()),
            DiffHunk::Equal(" cat".into()),
        ];
        assert_eq!(
            minimize_hunks(&hunks),
            vec![DiffHunk::Equal("the x cat".into())]
        );
    }

    #[test]
    fn minimize_cancels_smart_quote_no_op() {
        let hunks = vec![
            DiffHunk::Delete("don\u{2019}t".into()),
            DiffHunk::Insert("don't".into()),
        ];
        let refined = minimize_hunks(&hunks);
        assert_eq!(refined.len(), 1);
        assert!(matches!(&refined[0], DiffHunk::Equal(_)));
        assert!(!refined.iter().any(|h| matches!(h, DiffHunk::Delete(_))));
    }

    #[test]
    fn minimize_keeps_shared_prefix_of_a_replacement() {
        let d = diff_minimal("the cat sat", "the dog sat", Granularity::Word);
        let deleted: String = d
            .iter()
            .filter_map(|h| match h {
                DiffHunk::Delete(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        let inserted: String = d
            .iter()
            .filter_map(|h| match h {
                DiffHunk::Insert(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(deleted, "cat");
        assert_eq!(inserted, "dog");
        assert_eq!(
            mark_minimal("the cat sat", "the dog sat", Granularity::Word),
            "the [-cat-]{+dog+} sat"
        );
    }

    #[test]
    fn minimize_reconstructs_both_sides() {
        let old = "a x b y c";
        let new = "a z b y c";
        let d = diff_minimal(old, new, Granularity::Word);
        let rebuilt_old: String = d
            .iter()
            .filter_map(|h| match h {
                DiffHunk::Equal(s) | DiffHunk::Delete(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        let rebuilt_new: String = d
            .iter()
            .filter_map(|h| match h {
                DiffHunk::Equal(s) | DiffHunk::Insert(s) => Some(s.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(rebuilt_old, old);
        assert_eq!(rebuilt_new, new);
    }
}

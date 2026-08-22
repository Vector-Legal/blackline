//! Structure-preserving text splice: replace visible text without
//! discarding hyperlinks, bookmarks, comment markers, or other authors'
//! tracked changes.

use blackline_core::diff::Granularity;
use blackline_core::textutil::find_normalized;
use blackline_core::xml::XmlNode;

use crate::body::{get_mut_path, visible_text};
use crate::error::DocxError;
use crate::revision::{rebuild_with_diff, tracked_replace_rebuild};

/// Replace `old` with `new` inside a paragraph or table, keeping non-text
/// siblings (hyperlinks, bookmarks, comment markers, footnote refs).
pub fn plain_replace(node: &mut XmlNode, old: &str, new: &str) -> Result<bool, DocxError> {
    if node.is_element_with_local_name("tbl") {
        return replace_plain_in_table(node, old, new);
    }
    surgical_plain(node, old, new)
}

/// Tracked replacement that wraps only the matched span in `w:del` / `w:ins`.
pub fn tracked_replace(
    node: &mut XmlNode,
    old: &str,
    new: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) -> Result<bool, DocxError> {
    if node.is_element_with_local_name("tbl") {
        return replace_tracked_in_table(node, old, new, author, granularity, next_id);
    }
    if surgical_tracked(node, old, new, author, next_id)? {
        return Ok(true);
    }
    tracked_replace_rebuild(node, old, new, author, granularity, next_id)
}

fn replace_plain_in_table(node: &mut XmlNode, old: &str, new: &str) -> Result<bool, DocxError> {
    if node.is_element_with_local_name("p") {
        return surgical_plain(node, old, new);
    }
    let Some(kids) = node.try_children_mut() else {
        return Ok(false);
    };
    for kid in kids.iter_mut() {
        if replace_plain_in_table(kid, old, new)? {
            return Ok(true);
        }
    }
    Ok(false)
}

fn replace_tracked_in_table(
    node: &mut XmlNode,
    old: &str,
    new: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) -> Result<bool, DocxError> {
    if node.is_element_with_local_name("p") {
        return tracked_replace(node, old, new, author, granularity, next_id);
    }
    let Some(kids) = node.try_children_mut() else {
        return Ok(false);
    };
    for kid in kids.iter_mut() {
        if replace_tracked_in_table(kid, old, new, author, granularity, next_id)? {
            return Ok(true);
        }
    }
    Ok(false)
}

struct TextSpan {
    path: Vec<usize>,
    byte_start: usize,
    byte_len: usize,
}

fn collect_visible_t(
    node: &XmlNode,
    path: &mut Vec<usize>,
    cursor: &mut usize,
    out: &mut Vec<TextSpan>,
) {
    if node.is_element_with_local_name("del") {
        return;
    }
    if node.is_element_with_local_name("t") {
        let text = node.text_content();
        out.push(TextSpan {
            path: path.clone(),
            byte_start: *cursor,
            byte_len: text.len(),
        });
        *cursor += text.len();
        return;
    }
    for (i, child) in node.children().iter().enumerate() {
        path.push(i);
        collect_visible_t(child, path, cursor, out);
        path.pop();
    }
}

fn surgical_plain(para: &mut XmlNode, old: &str, new: &str) -> Result<bool, DocxError> {
    let text = visible_text(para);
    let Some((start, matched)) = find_normalized(&text, old) else {
        return Ok(false);
    };
    let end = start + matched.len();
    let mut spans = Vec::new();
    collect_visible_t(para, &mut Vec::new(), &mut 0, &mut spans);
    let overlapping: Vec<usize> = spans
        .iter()
        .enumerate()
        .filter(|(_, s)| s.byte_start < end && s.byte_start + s.byte_len > start)
        .map(|(i, _)| i)
        .collect();
    if overlapping.is_empty() {
        return Ok(false);
    }
    let mut placed = false;
    for i in overlapping {
        let span = &spans[i];
        let t = get_mut_path(para, &span.path)
            .ok_or_else(|| DocxError::invalid("text node vanished"))?;
        let current = t.text_content();
        let local_start = start.saturating_sub(span.byte_start);
        let local_end = (end - span.byte_start).min(current.len());
        let prefix = if span.byte_start <= start {
            current.get(..local_start).unwrap_or("").to_string()
        } else {
            String::new()
        };
        let suffix = if span.byte_start + span.byte_len >= end {
            current.get(local_end..).unwrap_or("").to_string()
        } else {
            String::new()
        };
        let revised = if !placed {
            placed = true;
            format!("{prefix}{new}{suffix}")
        } else {
            format!("{prefix}{suffix}")
        };
        t.set_text(&revised);
    }
    Ok(true)
}

fn surgical_tracked(
    para: &mut XmlNode,
    old: &str,
    new: &str,
    author: &str,
    next_id: &mut usize,
) -> Result<bool, DocxError> {
    wrap_visible_span(para, old, author, next_id, Some(new))
}

fn first_run_rpr(root: &XmlNode, run_path: &[usize]) -> Option<XmlNode> {
    let run = crate::body::get_path(root, run_path)?;
    run.find_child("rPr").cloned()
}

fn convert_t_to_deltext(node: &mut XmlNode) {
    if node.is_element_with_local_name("t") {
        if let XmlNode::Element { local_name, .. } = node {
            *local_name = "delText".into();
        }
    }
    if let Some(kids) = node.try_children_mut() {
        for kid in kids.iter_mut() {
            convert_t_to_deltext(kid);
        }
    }
}

/// Split the visible `w:t` that contains `byte_pos` so `byte_pos` falls on a
/// node boundary. Returns false when the position cannot be mapped.
fn split_visible_at(para: &mut XmlNode, byte_pos: usize) -> Result<bool, DocxError> {
    let mut spans = Vec::new();
    collect_visible_t(para, &mut Vec::new(), &mut 0, &mut spans);
    let Some(span) = spans
        .iter()
        .find(|s| byte_pos > s.byte_start && byte_pos < s.byte_start + s.byte_len)
    else {
        return Ok(true);
    };
    if span.path.len() < 2 {
        return Ok(false);
    }
    let run_path = span.path[..span.path.len() - 1].to_vec();
    let parent_path = run_path[..run_path.len() - 1].to_vec();
    let run_idx = run_path[run_path.len() - 1];
    let local = byte_pos - span.byte_start;

    let run = get_mut_path(para, &run_path).ok_or_else(|| DocxError::invalid("run vanished"))?;
    let t = run
        .find_child("t")
        .ok_or_else(|| DocxError::invalid("run has no w:t"))?;
    let full = t.text_content();
    if local > full.len() {
        return Ok(false);
    }
    let left = full[..local].to_string();
    let right = full[local..].to_string();
    let mut right_run = run.clone();
    if let Some(rt) = right_run.find_child_mut("t") {
        rt.set_text(&right);
    }
    if let Some(lt) = run.find_child_mut("t") {
        lt.set_text(&left);
    }

    let parent = if parent_path.is_empty() {
        para
    } else {
        get_mut_path(para, &parent_path).ok_or_else(|| DocxError::invalid("parent vanished"))?
    };
    parent.children_mut().insert(run_idx + 1, right_run);
    Ok(true)
}

/// Insert tracked text at a visible-text byte offset (used by redline).
pub fn tracked_insert_at(
    para: &mut XmlNode,
    byte_pos: usize,
    text: &str,
    author: &str,
    next_id: &mut usize,
) -> Result<bool, DocxError> {
    if text.is_empty() {
        return Ok(true);
    }
    if !split_visible_at(para, byte_pos)? {
        return Ok(false);
    }
    let mut spans = Vec::new();
    collect_visible_t(para, &mut Vec::new(), &mut 0, &mut spans);
    let Some(span) = spans.iter().find(|s| {
        s.byte_start == byte_pos
            || (s.byte_start < byte_pos && s.byte_start + s.byte_len >= byte_pos)
    }) else {
        // append after last visible run
        return tracked_append(para, text, author, next_id);
    };
    if span.path.len() < 2 {
        return Ok(false);
    }
    let run_path = &span.path[..span.path.len() - 1];
    let parent_path = run_path[..run_path.len() - 1].to_vec();
    let run_idx = run_path[run_path.len() - 1];
    let rpr = first_run_rpr(para, run_path);
    let date = blackline_core::time::utc_now_iso();
    let id = *next_id;
    *next_id += 1;
    let ins = crate::revision::ins_run(text, author, &date, id, &rpr);
    let insert_at = if span.byte_start >= byte_pos {
        run_idx
    } else {
        run_idx + 1
    };
    let parent = if parent_path.is_empty() {
        para
    } else {
        get_mut_path(para, &parent_path).ok_or_else(|| DocxError::invalid("parent vanished"))?
    };
    parent.children_mut().insert(insert_at, ins);
    Ok(true)
}

fn tracked_append(
    para: &mut XmlNode,
    text: &str,
    author: &str,
    next_id: &mut usize,
) -> Result<bool, DocxError> {
    let date = blackline_core::time::utc_now_iso();
    let id = *next_id;
    *next_id += 1;
    let ins = crate::revision::ins_run(text, author, &date, id, &None);
    let kids = para.children_mut();
    let at = kids
        .iter()
        .position(|c| c.is_element_with_local_name("sectPr"))
        .unwrap_or(kids.len());
    kids.insert(at, ins);
    Ok(true)
}

/// Apply an LCS diff as a sequence of surgical tracked edits (from the end
/// so earlier offsets stay valid). Falls back to a full rebuild.
pub fn apply_diff_surgically(
    para: &mut XmlNode,
    old_text: &str,
    new_text: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) -> Result<(), DocxError> {
    apply_diff_hunks(
        para,
        old_text,
        new_text,
        author,
        granularity,
        next_id,
        false,
    )
}

/// Like [`apply_diff_surgically`], but uses [`blackline_core::diff::diff_minimal`]
/// so identical delete+insert pairs cancel and shared prefix/suffix is not
/// marked.
pub fn apply_diff_surgically_minimal(
    para: &mut XmlNode,
    old_text: &str,
    new_text: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
) -> Result<(), DocxError> {
    apply_diff_hunks(para, old_text, new_text, author, granularity, next_id, true)
}

fn apply_diff_hunks(
    para: &mut XmlNode,
    old_text: &str,
    new_text: &str,
    author: &str,
    granularity: Granularity,
    next_id: &mut usize,
    minimal: bool,
) -> Result<(), DocxError> {
    if old_text == new_text {
        return Ok(());
    }
    let hunks = if minimal {
        blackline_core::diff::diff_minimal(old_text, new_text, granularity)
    } else {
        blackline_core::diff::diff(old_text, new_text, granularity)
    };
    let mut old_pos = 0usize;
    let mut ops: Vec<(usize, usize, String)> = Vec::new();
    for hunk in hunks {
        match hunk {
            blackline_core::diff::DiffHunk::Equal(s) => old_pos += s.len(),
            blackline_core::diff::DiffHunk::Delete(s) => {
                ops.push((old_pos, old_pos + s.len(), String::new()));
                old_pos += s.len();
            }
            blackline_core::diff::DiffHunk::Insert(s) => {
                if let Some(last) = ops.last_mut() {
                    if last.1 == old_pos && last.2.is_empty() {
                        last.2 = s;
                        continue;
                    }
                }
                ops.push((old_pos, old_pos, s));
            }
        }
    }
    for (a, b, new) in ops.into_iter().rev() {
        let ok = if a == b {
            tracked_insert_at(para, a, &new, author, next_id)?
        } else {
            // Offset-based: do not re-search the paragraph. Short
            // minimized hunks ("t", "i") would otherwise match the
            // wrong occurrence ("period" / "thirty").
            wrap_visible_range(
                para,
                a,
                b,
                author,
                next_id,
                (!new.is_empty()).then_some(new.as_str()),
            )?
        };
        if !ok {
            if minimal {
                crate::revision::rebuild_with_diff_minimal(
                    para,
                    old_text,
                    new_text,
                    author,
                    granularity,
                    next_id,
                );
            } else {
                rebuild_with_diff(para, old_text, new_text, author, granularity, next_id);
            }
            return Ok(());
        }
    }
    Ok(())
}

/// Wrap only the matched visible span in `w:del`. Surrounding runs,
/// hyperlinks, bookmarks, comment markers, and other authors' markup stay.
pub fn surgical_delete(
    para: &mut XmlNode,
    text: &str,
    author: &str,
    next_id: &mut usize,
) -> Result<bool, DocxError> {
    if text.is_empty() {
        return Ok(true);
    }
    wrap_visible_span(para, text, author, next_id, None)
}

fn wrap_visible_span(
    para: &mut XmlNode,
    old: &str,
    author: &str,
    next_id: &mut usize,
    insert: Option<&str>,
) -> Result<bool, DocxError> {
    let text = visible_text(para);
    let Some((start, matched)) = find_normalized(&text, old) else {
        return Ok(false);
    };
    wrap_visible_range(para, start, start + matched.len(), author, next_id, insert)
}

fn wrap_visible_range(
    para: &mut XmlNode,
    start: usize,
    end: usize,
    author: &str,
    next_id: &mut usize,
    insert: Option<&str>,
) -> Result<bool, DocxError> {
    if start > end {
        return Ok(false);
    }
    if start == end {
        return match insert {
            Some(text) => tracked_insert_at(para, start, text, author, next_id),
            None => Ok(true),
        };
    }
    if !split_visible_at(para, end)? {
        return Ok(false);
    }
    if !split_visible_at(para, start)? {
        return Ok(false);
    }
    let mut spans = Vec::new();
    collect_visible_t(para, &mut Vec::new(), &mut 0, &mut spans);
    let matched: Vec<TextSpan> = spans
        .into_iter()
        .filter(|s| s.byte_start >= start && s.byte_start + s.byte_len <= end && s.byte_len > 0)
        .collect();
    if matched.is_empty() {
        return Ok(false);
    }
    let run_paths: Vec<Vec<usize>> = matched
        .iter()
        .filter_map(|s| {
            if s.path.len() < 2 {
                return None;
            }
            Some(s.path[..s.path.len() - 1].to_vec())
        })
        .collect();
    if run_paths.is_empty() {
        return Ok(false);
    }
    let parent_path = run_paths[0][..run_paths[0].len() - 1].to_vec();
    if !run_paths
        .iter()
        .all(|p| p.len() == parent_path.len() + 1 && p[..parent_path.len()] == parent_path)
    {
        return Ok(false);
    }
    let mut run_indices: Vec<usize> = run_paths.iter().map(|p| p[p.len() - 1]).collect();
    run_indices.sort_unstable();
    run_indices.dedup();
    let first = *run_indices.first().unwrap();
    let last = *run_indices.last().unwrap();
    if last - first + 1 != run_indices.len() {
        return Ok(false);
    }

    let rpr = if insert.is_some() {
        first_run_rpr(para, &run_paths[0])
    } else {
        None
    };
    let parent = if parent_path.is_empty() {
        para
    } else {
        get_mut_path(para, &parent_path).ok_or_else(|| DocxError::invalid("parent vanished"))?
    };
    let kids = parent.children_mut();
    if last >= kids.len() {
        return Ok(false);
    }
    let mut taken = kids.drain(first..=last).collect::<Vec<_>>();
    for run in &mut taken {
        convert_t_to_deltext(run);
    }
    let date = blackline_core::time::utc_now_iso();
    let del_id = *next_id;
    *next_id += 1;
    let mut del = XmlNode::w("del")
        .with_attr("w:id", del_id.to_string())
        .with_attr("w:author", author)
        .with_attr("w:date", &date);
    for run in taken {
        del = del.with_child(run);
    }
    kids.insert(first, del);
    if let Some(new) = insert {
        let ins_id = *next_id;
        *next_id += 1;
        let ins = crate::revision::ins_run(new, author, &date, ins_id, &rpr);
        kids.insert(first + 1, ins);
    }
    Ok(true)
}

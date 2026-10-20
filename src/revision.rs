use std::{ops::Range, time::Duration};

use similar::{DiffTag, TextDiff};

/// Map a source anchor and collect changed byte ranges in the new revision.
/// Deletions are empty ranges at the surviving text immediately after them.
pub fn compare(old: &str, new: &str, anchor: usize) -> (usize, Vec<Range<usize>>) {
    let old_lines: Vec<_> = old.split_inclusive('\n').collect();
    let new_lines: Vec<_> = new.split_inclusive('\n').collect();
    let offsets = |lines: &[&str]| {
        let mut result = vec![0];
        for line in lines {
            result.push(result.last().copied().unwrap_or(0) + line.len());
        }
        result
    };
    let old_offsets = offsets(&old_lines);
    let new_offsets = offsets(&new_lines);
    let line = old_offsets
        .partition_point(|&start| start <= anchor)
        .saturating_sub(1);
    // Bound diff refinement on heavily rewritten documents. A coarse diff is
    // still valid; it may anchor within a replacement rather than an exact match.
    let diff = TextDiff::configure()
        .timeout(Duration::from_millis(50))
        .diff_slices(&old_lines, &new_lines);
    let mut mapped = new.len();
    let mut changes = Vec::new();
    for op in diff.ops() {
        let before = op.old_range();
        let after = op.new_range();
        if op.tag() != DiffTag::Equal {
            changes.push(new_offsets[after.start]..new_offsets[after.end]);
        }
        if before.contains(&line) {
            if after.is_empty() {
                mapped = new_offsets[after.start];
            } else {
                let target = after.start + (line - before.start).min(after.len() - 1);
                let column = anchor.saturating_sub(old_offsets[line]);
                mapped =
                    new_offsets[target] + column.min(new_lines[target].len().saturating_sub(1));
            }
        }
    }
    while !new.is_char_boundary(mapped) {
        mapped -= 1;
    }
    (mapped, changes)
}

//! Grep / ripgrep compressor: keep the highest-signal matches under the budget while
//! guaranteeing every file stays represented.
//!
//! Search dumps list `path:line:content` records, often many hits per file with one
//! that matters. Matches are scored by query overlap; the budget's worth of the
//! top-scoring matches survive, the first match of every file is force-kept (so no file
//! vanishes), and dropped runs become retrievable markers.

use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;

use super::{Ctx, FORCE_PRIORITY, MIN_KEEP, Mode, pick_mode, query_bonus, rebuild, select_keep};
use crate::stages::sizing::optimal_keep;

/// Captures the file field of a `path:line:` record (same shape as the detector, with
/// a capture group).
static GREP_REC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^((?:[A-Za-z]:)?[^:\n]*[A-Za-z./\\][^:\n]*):\d+:").unwrap());

fn file_of(line: &str) -> Option<&str> {
    GREP_REC
        .captures(line)
        .and_then(|c| c.get(1))
        .map(|m| m.as_str())
}

/// Compress a grep segment. Returns `None` when it already fits the budget.
pub fn compress(text: &str, ctx: &Ctx, query: &HashSet<String>) -> Option<String> {
    let lines: Vec<&str> = text.lines().collect();
    let files: Vec<Option<&str>> = lines.iter().map(|l| file_of(l)).collect();
    let distinct = files
        .iter()
        .filter_map(|f| *f)
        .collect::<HashSet<_>>()
        .len();

    // Signal = distinct files; many matches across few files goes aggressive (one match
    // per file). The adaptive path no-ops on an already-small dump.
    let mode = pick_mode(ctx.mode, lines.len(), distinct);
    if mode == Mode::Adaptive && lines.len() <= ctx.max_lines {
        return None;
    }

    let scores: Vec<f64> = lines.iter().map(|l| 0.2 + query_bonus(l, query)).collect();
    let keep = match mode {
        // Aggressive: the single highest-scored match per file, the rest dropped to refs.
        Mode::Aggressive => best_per_file(&files, &scores),
        // Adaptive: budget-many top matches, with the first match of every file forced
        // so no file vanishes.
        Mode::Adaptive => {
            let mut first_in_file = vec![false; lines.len()];
            let mut seen: HashSet<&str> = HashSet::new();
            for (i, f) in files.iter().enumerate() {
                if let Some(f) = f
                    && seen.insert(f)
                {
                    first_in_file[i] = true;
                }
            }
            let k = optimal_keep(&lines, MIN_KEEP, ctx.max_lines);
            let mut keep = select_keep(&scores, k, FORCE_PRIORITY);
            for (slot, &first) in keep.iter_mut().zip(&first_in_file) {
                *slot |= first;
            }
            keep
        }
    };

    if keep.iter().all(|&x| x) {
        return None;
    }
    Some(rebuild(&lines, &keep))
}

/// Keep the single highest-scored match per file (ties → first), dropping the rest.
fn best_per_file(files: &[Option<&str>], scores: &[f64]) -> Vec<bool> {
    let mut best: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (i, f) in files.iter().enumerate() {
        if let Some(f) = f {
            match best.get(f) {
                Some(&j) if scores[j] >= scores[i] => {}
                _ => {
                    best.insert(f, i);
                }
            }
        }
    }
    let keep: HashSet<usize> = best.values().copied().collect();
    (0..files.len()).map(|i| keep.contains(&i)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::toolout::{ModeSetting, test_ctx};

    fn grep_dump() -> String {
        let mut lines = Vec::new();
        for i in 0..40 {
            lines.push(format!("src/a.rs:{}:    let v = step({i});", i + 1));
        }
        for i in 0..40 {
            lines.push(format!("src/b.rs:{}:    helper({i});", i + 1));
        }
        lines.push("src/target.rs:5:    let widget = build();".to_string());
        lines.join("\n")
    }

    #[test]
    fn windows_matches_and_keeps_every_file() {
        let dump = grep_dump();
        let out = compress(&dump, &test_ctx(), &HashSet::new()).expect("compresses");

        assert!(out.lines().count() < dump.lines().count(), "fewer lines");
        for file in ["src/a.rs:", "src/b.rs:", "src/target.rs:"] {
            assert!(out.contains(file), "{file} still represented");
        }
        assert!(
            out.contains("omitted"),
            "dropped matches elided by position"
        );
    }

    #[test]
    fn query_relevant_match_survives() {
        let dump = grep_dump();
        let query: HashSet<String> = ["widget".to_string()].into_iter().collect();
        let out = compress(&dump, &test_ctx(), &query).expect("compresses");
        assert!(out.contains("let widget = build()"), "query hit is kept");
    }

    #[test]
    fn small_dump_is_left_alone() {
        let dump = "src/a.rs:1:one\nsrc/a.rs:2:two\nsrc/b.rs:3:three";
        assert_eq!(compress(dump, &test_ctx(), &HashSet::new()), None);
    }

    #[test]
    fn aggressive_keeps_one_match_per_file() {
        // 80 matches across 3 files → aggressive keeps exactly one (best) per file.
        let dump = grep_dump();
        let ctx = Ctx {
            max_lines: 30,
            template: true,
            mode: ModeSetting::Aggressive,
        };
        let out = compress(&dump, &ctx, &HashSet::new()).expect("compresses");
        let kept_matches = out.lines().filter(|l| l.contains(".rs:")).count();
        assert_eq!(
            kept_matches, 3,
            "one match per file (a, b, target), got {kept_matches}"
        );
        for file in ["src/a.rs:", "src/b.rs:", "src/target.rs:"] {
            assert!(out.contains(file), "{file} represented");
        }
    }
}

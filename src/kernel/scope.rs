//! Touches-overlap arithmetic for parallel claims (decision 0025).
//!
//! Pure functions over the `touches` grammar — no I/O, no state. The
//! dependencies are [`crate::source::glob_match`] and, for the shared
//! entry-validity check, [`crate::storage::safe_repo_relative`] (pure path
//! validation, no filesystem access). A caller hands in two pattern lists
//! and learns whether one claim can name a file the other holds; nothing
//! here knows about records, leases or the filesystem.
//!
//! The invariant is deliberately conservative in one direction: a false
//! positive serializes two tickets that would in fact never collide (a
//! scheduling loss the host retries next round), while a false negative
//! lets two workers write one file (silent corruption with receipts that
//! both look legitimate). When in doubt, overlap.

/// `overlaps` reports these when a list is empty: an empty `touches` is
/// exclusive by decision 0025, so the pair names no real pattern.
const EXCLUSIVE: &str = "<exclusive>";

/// Whether two `touches` lists can name the same file, returning one
/// overlapping `(a pattern, b pattern)` pair. An empty list on either side
/// overlaps everything (a ticket without `touches` claims exclusively).
pub fn overlaps(a: &[String], b: &[String]) -> Option<(String, String)> {
    if a.is_empty() || b.is_empty() {
        return Some((EXCLUSIVE.to_string(), EXCLUSIVE.to_string()));
    }
    for p in a {
        for q in b {
            if pair_overlaps(p, q) {
                return Some(((*p).clone(), (*q).clone()));
            }
        }
    }
    None
}

/// Whether any pattern in `touches` matches `path` (same grammar as
/// [`crate::source::glob_match`]). An empty list covers nothing.
pub fn covers(touches: &[String], path: &str) -> bool {
    touches
        .iter()
        .any(|pattern| crate::source::glob_match(pattern, path))
}

/// Whether `entry` is a well-formed touch: non-empty, repo-relative, and
/// staying inside the repo. `safe_repo_relative` accepts glob characters (a
/// `*` is a normal path segment) and refuses absolute paths and `..` —
/// exactly the grammar a touch must have to take part in a reservation.
/// One shared check, so the ready gate and `pulse reserve` (decision 0025
/// B4) cannot drift apart on what a legal touch is.
pub(crate) fn valid_touch(entry: &str) -> bool {
    !entry.trim().is_empty() && crate::storage::safe_repo_relative(entry).is_ok()
}

fn pair_overlaps(p: &str, q: &str) -> bool {
    // 1. Equal patterns name the same files trivially.
    if p == q {
        return true;
    }
    let p_is_dir = dir_prefix(p).is_some();
    let q_is_dir = dir_prefix(q).is_some();
    let p_has_star = p.contains('*');
    let q_has_star = q.contains('*');
    // 2. A literal (no `*`, no directory prefix) is decided by the shared
    //    glob matcher — one glob against one exact path.
    if !p_has_star && !p_is_dir && crate::source::glob_match(q, p) {
        return true;
    }
    if !q_has_star && !q_is_dir && crate::source::glob_match(p, q) {
        return true;
    }
    // 3. Two directory prefixes overlap when one nests the other, segment
    //    by segment (`src/a` does not prefix `src/ab`).
    if p_is_dir && q_is_dir {
        let (base_p, base_q) = (dir_prefix(p).unwrap_or(""), dir_prefix(q).unwrap_or(""));
        return is_segment_prefix(base_p, base_q) || is_segment_prefix(base_q, base_p);
    }
    // 4. A directory prefix against a starred pattern: the literal head of
    //    the starred side sits under the prefix, or the prefix sits under
    //    that head.
    if let Some(base) = one_dir_prefix(p, q, p_is_dir, q_is_dir) {
        let starred = if p_is_dir { q } else { p };
        let head = literal_head(starred);
        return is_segment_prefix(head, base) || is_segment_prefix(base, head);
    }
    // 5. Two starred patterns: if one's literal head is a string prefix of
    //    the other, some file may match both — anything else is refused as
    //    inconclusive, which here means overlapping.
    if p_has_star && q_has_star {
        let (head_p, head_q) = (literal_head(p), literal_head(q));
        return q.starts_with(head_p) || p.starts_with(head_q);
    }
    false
}

/// The directory a `dir/` or `dir/**` pattern keeps everything under.
fn dir_prefix(pattern: &str) -> Option<&str> {
    pattern
        .strip_suffix("/**")
        .or_else(|| pattern.strip_suffix('/'))
}

/// When exactly one side is a directory prefix, return that side's base.
fn one_dir_prefix<'a>(p: &'a str, q: &'a str, p_is_dir: bool, q_is_dir: bool) -> Option<&'a str> {
    match (p_is_dir, q_is_dir) {
        (true, false) => dir_prefix(p),
        (false, true) => dir_prefix(q),
        _ => None,
    }
}

/// Everything before the first `*`, which every file the pattern can name
/// must start with (`src/*/h.rs` -> `src/`).
fn literal_head(pattern: &str) -> &str {
    pattern.split('*').next().unwrap_or(pattern)
}

/// Whether `prefix` and `path` agree segment by segment through prefix's
/// length (`src/a` is not a prefix of `src/ab`, only of `src/ab/...`).
fn is_segment_prefix(prefix: &str, path: &str) -> bool {
    let prefix = segments(prefix);
    let path = segments(path);
    prefix.len() <= path.len() && prefix.iter().zip(&path).all(|(a, b)| a == b)
}

/// Path segments without empty pieces, so trailing slashes and the empty
/// head of a pattern like `*.rs` compare as zero depth.
fn segments(value: &str) -> Vec<&str> {
    value
        .split('/')
        .filter(|segment| !segment.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(value: &str) -> Vec<String> {
        vec![value.to_string()]
    }

    #[test]
    fn identical_patterns_overlap() {
        assert!(pair(pairs("a.rs", "a.rs")));
        assert!(pair(pairs("src/a.rs", "src/a.rs")));
        assert!(pair(pairs("src/**", "src/**")));
    }

    #[test]
    fn sibling_files_do_not_overlap() {
        assert!(!pair(pairs("src/a.rs", "src/b.rs")));
    }

    #[test]
    fn a_dir_glob_overlaps_its_files() {
        assert!(pair(pairs("src/**", "src/x/y.rs")));
        assert!(pair(pairs("src/", "src/x.rs")));
    }

    #[test]
    fn nested_dir_prefixes_only_overlap_when_they_nest() {
        assert!(pair(pairs("src/a/**", "src/a/deep/x.rs")));
        assert!(!pair(pairs("src/a/**", "src/ab/x.rs")));
        assert!(!pair(pairs("api/**", "web/**")));
    }

    #[test]
    fn one_level_glob_overlaps_only_same_dir_files() {
        assert!(pair(pairs("docs/*.md", "docs/a.md")));
        assert!(!pair(pairs("docs/*.md", "docs/a.txt")));
        assert!(!pair(pairs("docs/*.md", "docs/sub/a.md")));
        assert!(pair(pairs("src/*/h.rs", "src/x/h.rs")));
    }

    #[test]
    fn an_empty_side_is_exclusive_against_anything() {
        assert_eq!(
            overlaps(&[], &touch("src/**")),
            Some((EXCLUSIVE.to_string(), EXCLUSIVE.to_string()))
        );
        assert_eq!(
            overlaps(&touch("src/**"), &[]),
            Some((EXCLUSIVE.to_string(), EXCLUSIVE.to_string()))
        );
        assert_eq!(
            overlaps(&[], &[]),
            Some((EXCLUSIVE.to_string(), EXCLUSIVE.to_string()))
        );
    }

    #[test]
    fn overlaps_returns_the_offending_pair() {
        let a = touch("web/**");
        let b = vec!["src/api/**".to_string(), "web/components/*.tsx".to_string()];
        assert_eq!(
            overlaps(&a, &b),
            Some(("web/**".to_string(), "web/components/*.tsx".to_string()))
        );
    }

    #[test]
    fn disjoint_lists_do_not_overlap() {
        let a = vec!["src/api/**".to_string(), "src/api.rs".to_string()];
        let b = vec!["web/**".to_string(), "docs/*.md".to_string()];
        assert_eq!(overlaps(&a, &b), None);
    }

    #[test]
    fn covers_follows_the_shared_glob_grammar() {
        assert!(covers(&touch("src/**"), "src/a/b.rs"));
        assert!(!covers(&touch("src/a.rs"), "src/b.rs"));
        assert!(covers(&touch("docs/*.md"), "docs/readme.md"));
        assert!(!covers(&touch("docs/*.md"), "docs/sub/readme.md"));
        assert!(!covers(&[], "src/a.rs"));
    }

    fn pairs(p: &str, q: &str) -> (Vec<String>, Vec<String>) {
        (touch(p), touch(q))
    }

    fn pair((a, b): (Vec<String>, Vec<String>)) -> bool {
        overlaps(&a, &b).is_some()
    }
}

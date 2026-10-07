//! Minimal line diff used to preview settings changes before writing them.

/// Returns the changed lines only: `+ line` for additions and `- line` for removals,
/// in document order. Empty when both texts have the same lines.
pub fn diff_lines(old: &str, new: &str) -> String {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // Longest-common-subsequence table; settings files are a few hundred lines.
    let mut lcs = vec![vec![0u32; b.len() + 1]; a.len() + 1];
    for i in (0..a.len()).rev() {
        for j in (0..b.len()).rev() {
            lcs[i][j] = if a[i] == b[j] { lcs[i + 1][j + 1] + 1 } else { lcs[i + 1][j].max(lcs[i][j + 1]) };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = String::new();
    while i < a.len() || j < b.len() {
        if i < a.len() && j < b.len() && a[i] == b[j] {
            i += 1;
            j += 1;
        } else if j < b.len() && (i == a.len() || lcs[i][j + 1] >= lcs[i + 1][j]) {
            out.push_str("+ ");
            out.push_str(b[j]);
            out.push('\n');
            j += 1;
        } else {
            out.push_str("- ");
            out.push_str(a[i]);
            out.push('\n');
            i += 1;
        }
    }
    out
}

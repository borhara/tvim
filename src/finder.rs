use std::path::Path;
use std::time::{Duration, Instant};

const SKIP: &[&str] = &["target", "node_modules", "__pycache__", "venv", "dist", "build"];

const MAX_FILES: usize = 50_000;
const MAX_TIME: Duration = Duration::from_millis(1500);

pub fn walk(root: &Path) -> (Vec<String>, bool) {
    let mut files = Vec::new();
    let started = Instant::now();
    let mut cut_short = false;
    let mut stack = vec![(root.to_path_buf(), 0usize)];
    while let Some((dir, depth)) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let Ok(kind) = entry.file_type() else { continue };
            let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                continue;
            };
            if ignored.iter().any(|p| p.matches(&name)) {
                continue;
            }
            if kind.is_dir() {
                if !SKIP.contains(&name.as_str()) && !name.starts_with('.') && depth < 32 {
                    stack.push((entry.path(), depth + 1));
                }
            } else if kind.is_file() && name != ".DS_Store" {
                if let Ok(rel) = entry.path().strip_prefix(root) {
                    files.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        if files.len() >= MAX_FILES || started.elapsed() > MAX_TIME {
            cut_short = true;
            break;
        }
    }
    files.sort_by(|a, b| {
        a.matches('/')
            .count()
            .cmp(&b.matches('/').count())
            .then_with(|| a.cmp(b))
    });
    (files, cut_short)
}

fn needle(query: &str) -> Vec<char> {
    query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

fn score(path: &str, query: &[char]) -> Option<(i64, Vec<usize>)> {
    let chars: Vec<char> = path.chars().collect();
    if query.is_empty() {
        return Some((-(chars.len() as i64), Vec::new()));
    }
    let name_start = chars.iter().rposition(|c| *c == '/').map_or(0, |i| i + 1);
    let hits = subsequence(&chars[name_start..], query)
        .map(|h| h.into_iter().map(|i| i + name_start).collect::<Vec<_>>())
        .or_else(|| subsequence(&chars, query))?;
    let mut score: i64 = 0;
    for (n, &i) in hits.iter().enumerate() {
        score += 10;
        if i >= name_start {
            score += 12;
        }
        if n > 0 && hits[n - 1] + 1 == i {
            score += 18;
        }
        let boundary = i == 0
            || matches!(chars[i - 1], '/' | '_' | '-' | '.' | ' ')
            || chars[i].is_uppercase() && chars[i - 1].is_lowercase();
        if boundary {
            score += 20;
        }
        if i == name_start {
            score += 25;
        }
    }
    let spread = hits.last().unwrap_or(&0) - hits.first().unwrap_or(&0);
    score -= spread as i64;
    score -= chars.len() as i64 / 3;
    Some((score, hits))
}

fn subsequence(chars: &[char], query: &[char]) -> Option<Vec<usize>> {
    let lower: Vec<char> = chars.iter().map(|c| c.to_lowercase().next().unwrap_or(*c)).collect();
    // leftmost match, then pull letters right so they cluster
    let mut hits = Vec::with_capacity(query.len());
    let mut from = 0;
    for q in query {
        let at = (from..lower.len()).find(|&i| lower[i] == *q)?;
        hits.push(at);
        from = at + 1;
    }
    let mut limit = lower.len();
    for k in (0..hits.len()).rev() {
        let latest = (hits[k]..limit)
            .rev()
            .find(|&i| lower[i] == query[k])
            .unwrap_or(hits[k]);
        hits[k] = latest;
        limit = latest;
    }
    for k in (0..hits.len().saturating_sub(1)).rev() {
        if hits[k] + 1 != hits[k + 1] && hits[k + 1] > 0 && lower[hits[k + 1] - 1] == query[k] {
            let lowest = if k == 0 { 0 } else { hits[k - 1] + 1 };
            if hits[k + 1] > lowest {
                hits[k] = hits[k + 1] - 1;
            }
        }
    }
    Some(hits)
}

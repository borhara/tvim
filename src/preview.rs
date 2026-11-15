// split at blank lines outside fences and around headings, where tinsel restarts anyway
fn blocks(lines: &[String]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut start: Option<usize> = None;
    let mut fence: Option<&str> = None;
    for (i, line) in lines.iter().enumerate() {
        let t = line.trim();
        if let Some(f) = fence {
            if t.starts_with(f) {
                fence = None;
            }
            continue;
        }
        let hashes = t.chars().take_while(|&c| c == '#').count();
        let heading = (1..=6).contains(&hashes) && t[hashes..].starts_with(' ');
        if t.is_empty() || heading {
            if let Some(s) = start.take() {
                out.push((s, i));
            }
            if heading {
                out.push((i, i + 1));
            }
            continue;
        }
        fence = ["```", "~~~"].into_iter().find(|f| t.starts_with(f));
        start.get_or_insert(i);
    }
    if let Some(s) = start {
        out.push((s, lines.len()));
    }
    out
}

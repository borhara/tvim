fn needle(query: &str) -> Vec<char> {
    query
        .chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
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

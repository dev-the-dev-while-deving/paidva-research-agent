//! Free search sources (no keys) + page fetch + chunking + keyword ranking.
use anyhow::Result;
use futures::future::join_all;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// One retrieved document. `text` is the best text available (article, abstract or page).
#[derive(Debug, Clone, serde::Serialize)]
pub struct Doc {
    pub source: String,
    pub title: String,
    pub url: String,
    pub text: String,
}

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("research-agent/0.1 (personal research CLI)")
        .timeout(Duration::from_secs(15))
        .build()
        .expect("http client")
}

/// Wikipedia: top 3 pages for `query`, with full plain-text extracts.
pub async fn wikipedia(http: &reqwest::Client, query: &str) -> Result<Vec<Doc>> {
    let hits: Value = http
        .get("https://en.wikipedia.org/w/rest.php/v1/search/page")
        .query(&[("q", query), ("limit", "3")])
        .send().await?.error_for_status()?.json().await?;
    let titles: Vec<String> = hits["pages"].as_array().into_iter().flatten()
        .filter_map(|p| p["title"].as_str().map(String::from)).collect();
    // Full-text extracts are limited to one title per request, so fetch them concurrently.
    let pages = join_all(titles.iter().map(|t| async move {
        let v: Value = http.get("https://en.wikipedia.org/w/api.php")
            .query(&[("action", "query"), ("prop", "extracts"), ("explaintext", "1"), ("redirects", "1"), ("format", "json"), ("titles", t.as_str())])
            .send().await.ok()?.json().await.ok()?;
        let p = v["query"]["pages"].as_object()?.values().next()?.clone();
        let title = p["title"].as_str()?.to_string();
        let text = p["extract"].as_str()?.to_string();
        let url = format!("https://en.wikipedia.org/wiki/{}", title.replace(' ', "_"));
        Some(Doc { source: "wikipedia".into(), title, url, text })
    })).await;
    Ok(pages.into_iter().flatten().filter(|d| d.text.len() > 100).collect())
}

/// Split into ~`size`-char chunks on line boundaries.
pub fn chunks(text: &str, size: usize) -> Vec<String> {
    let mut out = vec![];
    let mut cur = String::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if cur.len() + line.len() > size && !cur.is_empty() {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() { cur.push('\n'); }
        cur.push_str(line);
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

fn terms(s: &str) -> Vec<String> {
    const STOP: &[&str] = &["the", "and", "for", "are", "was", "with", "that", "this", "from", "what", "how", "does", "which", "who", "why", "into", "have", "has"];
    s.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2 && !STOP.contains(w)).map(String::from).collect()
}

/// Indices of `docs` sorted best-first by a BM25-style keyword score against `query`.
// ponytail: keyword ranking misses paraphrases; upgrade to fastembed embeddings if traces show poor recall.
pub fn rank(query: &str, docs: &[String]) -> Vec<(usize, f64)> {
    let q: HashSet<String> = terms(query).into_iter().collect();
    let toks: Vec<Vec<String>> = docs.iter().map(|d| terms(d)).collect();
    let n = docs.len() as f64;
    let avg = toks.iter().map(Vec::len).sum::<usize>() as f64 / n.max(1.0);
    let df: HashMap<&String, f64> = q.iter().map(|t| (t, toks.iter().filter(|d| d.contains(t)).count() as f64)).collect();
    let mut scored: Vec<(usize, f64)> = toks.iter().enumerate().map(|(i, d)| {
        let len = d.len() as f64;
        let s = q.iter().map(|t| {
            let tf = d.iter().filter(|w| *w == t).count() as f64;
            let idf = ((n - df[t] + 0.5) / (df[t] + 0.5) + 1.0).ln();
            idf * tf * 2.2 / (tf + 1.2 * (0.25 + 0.75 * len / avg.max(1.0)))
        }).sum();
        (i, s)
    }).collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    scored
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ranks_relevant_chunk_first() {
        let docs = vec!["Bananas are yellow fruit.".into(), "The Welch t-test handles unequal variances between groups.".into(), "Weather today is sunny.".into()];
        assert_eq!(rank("welch test unequal variances", &docs)[0].0, 1);
        let c = chunks("aaaa\nbbbb\ncccc", 9);
        assert_eq!(c, vec!["aaaa\nbbbb", "cccc"]);
    }
}

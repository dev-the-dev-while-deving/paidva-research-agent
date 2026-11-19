//! Paper search: arXiv (Atom XML) and Semantic Scholar (JSON). Same `Doc` shape as search.rs.
use crate::search::Doc;
use anyhow::Result;
use serde_json::Value;

/// arXiv asks API clients for at most one request every 3 seconds; parallel queries get refused.
async fn arxiv_turn() {
    static NEXT: std::sync::OnceLock<tokio::sync::Mutex<tokio::time::Instant>> = std::sync::OnceLock::new();
    let slot = NEXT.get_or_init(|| tokio::sync::Mutex::new(tokio::time::Instant::now()));
    let mut next = slot.lock().await;
    let at = (*next).max(tokio::time::Instant::now());
    *next = at + std::time::Duration::from_secs(3);
    drop(next);
    tokio::time::sleep_until(at).await;
}

/// arXiv: top 5 papers for `query`. `text` = "title\nabstract".
pub async fn arxiv(http: &reqwest::Client, query: &str) -> Result<Vec<Doc>> {
    arxiv_turn().await;
    let q = format!("all:{query}");
    let body = http.get("https://export.arxiv.org/api/query")
        .query(&[("search_query", q.as_str()), ("start", "0"), ("max_results", "5"), ("sortBy", "relevance")])
        .send().await?.error_for_status()?.text().await?;
    Ok(parse_arxiv(&body))
}

/// Inner text of the first `<name>...</name>` in `s`.
fn tag(s: &str, name: &str) -> Option<String> {
    let open = format!("<{name}>");
    let start = s.find(&open)? + open.len();
    let end = s[start..].find(&format!("</{name}>"))? + start;
    Some(s[start..end].to_string())
}

/// Parse an arXiv Atom feed into docs.
pub fn parse_arxiv(xml: &str) -> Vec<Doc> {
    xml.split("<entry>").skip(1).filter_map(|entry| {
        let entry = entry.split("</entry>").next()?;
        let clean = |s: String| s.split_whitespace().collect::<Vec<_>>().join(" ")
            .replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"")
            .replace("&apos;", "'").replace("&amp;", "&");
        let title = clean(tag(entry, "title")?);
        let summary = clean(tag(entry, "summary")?);
        let id = tag(entry, "id")?;
        let id = id.trim();
        let url = match id.strip_prefix("http://") {
            Some(rest) => format!("https://{rest}"),
            None => id.to_string(),
        };
        Some(Doc { source: "arxiv".into(), title: title.clone(), url, text: format!("{title}\n{summary}") })
    }).collect()
}

/// Replace each `<math ...>...</math>` with its `alttext` (or a space when absent).
fn strip_math(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(start) = rest.find("<math") {
        out.push_str(&rest[..start]);
        let after = &rest[start..];
        let (Some(open_end), Some(close)) = (after.find('>'), after.find("</math>")) else {
            out.push_str(after);
            return out;
        };
        let open = &after[..open_end];
        let alt = open.find("alttext=\"").and_then(|i| open[i + 9..].split('"').next()).unwrap_or("");
        out.push(' ');
        if !alt.is_empty() {
            out.push_str(alt);
            out.push(' ');
        }
        rest = &after[close + "</math>".len()..];
    }
    out.push_str(rest);
    out
}

/// Full text of an arXiv HTML (ar5iv-style `ltx_` markup) paper: paragraphs, headings, list items, captions.
pub fn parse_arxiv_html(html: &str) -> String {
    let html = strip_math(html);
    let doc = scraper::Html::parse_document(&html);
    let mut els: Vec<_> = doc.select(&scraper::Selector::parse("article p, article h1, article h2, article h3, article h4, article li, article figcaption").unwrap()).collect();
    if els.is_empty() {
        els = doc.select(&scraper::Selector::parse("p, h1, h2, h3, h4, li, figcaption").unwrap()).collect();
    }
    let in_bib = |e: &scraper::ElementRef| e.ancestors().any(|n| {
        scraper::ElementRef::wrap(n).and_then(|r| r.value().attr("class")).is_some_and(|c| c.contains("ltx_bibliography"))
    });
    let kept: Vec<String> = els.into_iter().filter(|e| !in_bib(e)).filter_map(|e| {
        let text = e.text().collect::<Vec<_>>().join(" ");
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        (text.chars().count() > 30).then_some(text)
    }).collect();
    let mut text = kept.join("\n");
    if text.len() > 200_000 {
        let mut cut = 200_000;
        while !text.is_char_boundary(cut) { cut -= 1; }
        text.truncate(cut);
    }
    text
}

/// Full text of a paper from its arXiv abs URL, via arxiv.org/html/{id}.
pub async fn arxiv_fulltext(http: &reqwest::Client, abs_url: &str) -> Result<String> {
    let Some(i) = abs_url.rfind("/abs/") else {
        return Err(anyhow::anyhow!("not an arXiv abs url"));
    };
    let id = &abs_url[i + 5..];
    arxiv_turn().await;
    let body = http.get(format!("https://arxiv.org/html/{id}"))
        .send().await?.error_for_status()?.text().await?;
    let text = parse_arxiv_html(&body);
    if text.len() < 1000 {
        return Err(anyhow::anyhow!("no usable full text"));
    }
    Ok(text)
}

/// Semantic Scholar: top 5 papers with abstracts.
pub async fn semantic_scholar(http: &reqwest::Client, query: &str) -> Result<Vec<Doc>> {
    let mut req = http.get("https://api.semanticscholar.org/graph/v1/paper/search")
        .query(&[("query", query), ("limit", "5"), ("fields", "title,abstract,url,year,citationCount")]);
    // Without a (free) key, requests share one global rate limit and often get 429.
    if let Ok(k) = std::env::var("S2_API_KEY") { req = req.header("x-api-key", k); }
    let v: Value = req.send().await?.error_for_status()?.json().await?;
    Ok(v["data"].as_array().into_iter().flatten().filter_map(|item| {
        let title = item["title"].as_str()?;
        let abs = item["abstract"].as_str()?;
        let year = item["year"].as_i64().map_or("?".to_string(), |y| y.to_string());
        let cites = item["citationCount"].as_i64().map_or("?".to_string(), |c| c.to_string());
        let url = item["url"].as_str().unwrap_or("").to_string();
        Some(Doc {
            source: "semantic_scholar".into(),
            title: title.into(),
            url,
            text: format!("{title} ({year}, {cites} citations)\n{abs}"),
        })
    }).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_arxiv_feed() {
        let xml = "<feed><title>ArXiv Query</title><id>http://arxiv.org/api/x</id><entry><id>http://arxiv.org/abs/2401.00001v1</id><title>Fast &amp; Loose:\n  A &lt;Study&gt;</title><summary>  We test\n things. </summary></entry></feed>";
        let docs = parse_arxiv(xml);
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].title, "Fast & Loose: A <Study>");
        assert_eq!(docs[0].url, "https://arxiv.org/abs/2401.00001v1");
        assert!(docs[0].text.starts_with("Fast & Loose: A <Study>\nWe test things."));
    }

    #[test]
    fn parses_arxiv_html() {
        let html = "<html><body><article><h2>Results section text that is long enough to keep here.</h2><p>We show that the loss <math alttext=\"L = x^2\"><mi>L</mi><mo>=</mo></math> is minimised at zero for all inputs considered.</p><section class=\"ltx_bibliography\"><li>Bibliography entry that should be dropped entirely from output.</li></section></article></body></html>";
        let text = parse_arxiv_html(html);
        assert!(text.contains("the loss L = x^2 is minimised at zero"));
        assert!(text.contains("Results section text that is long enough to keep here."));
        assert!(!text.contains("Bibliography entry that should be dropped entirely"));
    }
}

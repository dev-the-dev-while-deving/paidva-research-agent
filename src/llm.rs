//! Model client over OpenAI-compatible providers (Groq, OpenRouter): JSON-schema output, SQLite reply cache,
//! fallback across models and providers on rate limits.
//! A model spec is a plain id for Groq ("openai/gpt-oss-120b") or "openrouter:<id>" for OpenRouter.
use anyhow::{Context, Result, anyhow, bail};
use rusqlite::{Connection, OptionalExtension};
use schemars::JsonSchema;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const GROQ: &str = "https://api.groq.com/openai/v1/chat/completions";
const OPENROUTER: &str = "https://openrouter.ai/api/v1/chat/completions";
const GEMINI: &str = "https://generativelanguage.googleapis.com/v1beta/openai/chat/completions";

#[derive(Clone, Copy, Debug)]
pub enum Role {
    /// Router, checker, consistency samples.
    Fast,
    /// Planner, analyst, writer.
    Main,
}

impl Role {
    /// The fallback chain from ~/.agent/config.toml (defaults in config.rs).
    fn models(self) -> Vec<String> {
        let m = crate::config::get().models;
        match self { Role::Fast => m.fast.clone(), Role::Main => m.main.clone() }
    }
}

/// (endpoint, api key env var, model id as the provider names it)
pub fn route(spec: &str) -> (&'static str, &'static str, &str) {
    if let Some(id) = spec.strip_prefix("openrouter:") { return (OPENROUTER, "OPENROUTER_API_KEY", id); }
    if let Some(id) = spec.strip_prefix("gemini:") { return (GEMINI, "GEMINI_API_KEY", id); }
    (GROQ, "GROQ_API_KEY", spec)
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct CallLog {
    pub step: String,
    pub model: String,
    pub ms: u128,
    pub tokens: u64,
    pub cached: bool,
}

pub struct Llm {
    http: reqwest::Client,
    cache: Mutex<Connection>,
    pub calls: Mutex<Vec<CallLog>>,
    /// Pins the Main role to one model with no fallback (for fair comparisons in evals).
    pub main_model: Option<String>,
    /// Models that hit a long (daily) limit this run: skipped instead of waited on.
    exhausted: Mutex<HashSet<String>>,
    /// Models that rejected json_schema: they get json_object mode with the schema in the prompt.
    loose_json: Mutex<HashSet<String>>,
    /// Per model: tokens left in the current minute window and when it resets (from Groq's x-ratelimit headers).
    window: Mutex<std::collections::HashMap<String, (f64, Instant)>>,
}

/// Groq reset durations: "7.66s", "1m2.5s", "120ms", "2h3m4s".
fn parse_duration(s: &str) -> Option<f64> {
    let (mut total, mut num) = (0.0, String::new());
    let mut chars = s.trim().chars().peekable();
    while let Some(c) = chars.next() {
        if c.is_ascii_digit() || c == '.' { num.push(c); continue; }
        let v: f64 = num.parse().ok()?;
        num.clear();
        total += match c {
            'h' => v * 3600.0,
            'm' if chars.peek() == Some(&'s') => { chars.next(); v / 1000.0 }
            'm' => v * 60.0,
            's' => v,
            _ => return None,
        };
    }
    if !num.is_empty() { total += num.parse::<f64>().ok()?; }
    Some(total)
}

/// Checks a provider key by listing its models (free, no tokens used). Returns model specs as the config names them.
pub async fn list_models(key_var: &str, key: &str) -> Result<Vec<String>> {
    let (chat_url, prefix) = match key_var {
        "GROQ_API_KEY" => (GROQ, ""),
        "GEMINI_API_KEY" => (GEMINI, "gemini:"),
        "OPENROUTER_API_KEY" => (OPENROUTER, "openrouter:"),
        _ => bail!("{key_var} is not a model provider"),
    };
    let url = chat_url.replace("chat/completions", "models");
    let resp = reqwest::Client::builder().timeout(Duration::from_secs(15)).build()?.get(&url).bearer_auth(key).send().await?;
    let status = resp.status();
    if status.as_u16() == 401 || status.as_u16() == 403 { bail!("the provider rejected this key ({status})"); }
    if !status.is_success() { bail!("the provider answered {status}"); }
    let v: Value = resp.json().await?;
    let mut ids: Vec<String> = v["data"].as_array().into_iter().flatten()
        .filter_map(|m| m["id"].as_str())
        .map(|id| id.trim_start_matches("models/"))
        // OpenRouter lists paid models too; Paidva stays on free ones.
        .filter(|id| prefix != "openrouter:" || id.ends_with(":free"))
        .map(|id| format!("{prefix}{id}")).collect();
    ids.sort();
    Ok(ids)
}

impl Llm {
    pub fn new(cache_path: &std::path::Path) -> Result<Self> {
        anyhow::ensure!(["GROQ_API_KEY", "OPENROUTER_API_KEY", "GEMINI_API_KEY"].iter().any(|k| std::env::var(k).is_ok()),
            "no API key yet: run `paidva setup` (or set GROQ_API_KEY, GEMINI_API_KEY or OPENROUTER_API_KEY)");
        let db = Connection::open(cache_path)?;
        db.execute("CREATE TABLE IF NOT EXISTS cache (k TEXT PRIMARY KEY, v TEXT NOT NULL)", [])?;
        Ok(Self {
            http: reqwest::Client::builder().timeout(Duration::from_secs(120)).build()?,
            cache: Mutex::new(db),
            calls: Mutex::new(Vec::new()),
            main_model: None,
            exhausted: Mutex::new(HashSet::new()),
            loose_json: Mutex::new(HashSet::new()),
            window: Mutex::new(Default::default()),
        })
    }

    /// One structured call. `sample` > 0 gives a distinct, uncached-by-default sample (for self-consistency).
    pub async fn structured<T: DeserializeOwned + JsonSchema>(
        &self,
        step: &str,
        role: Role,
        system: &str,
        user: &str,
        temperature: f32,
        sample: u32,
    ) -> Result<T> {
        let schema = strict_schema::<T>();
        let raw = self.chat(step, role, system, user, &schema, temperature, sample).await?;
        match serde_json::from_str::<T>(&raw) {
            Ok(v) => Ok(v),
            Err(e) => {
                // One retry, showing the model its own parse error.
                let retry_user = format!("{user}\n\nYour previous reply failed to parse: {e}. Reply again with valid JSON only.");
                let raw = self.chat(step, role, system, &retry_user, &schema, temperature, sample).await?;
                serde_json::from_str(&raw).with_context(|| format!("{step}: unparseable reply: {raw}"))
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn chat(&self, step: &str, role: Role, system: &str, user: &str, schema: &Value, temperature: f32, sample: u32) -> Result<String> {
        let mut last_err = anyhow!("no models configured");
        let chain = role.models();
        let models: Vec<&str> = match (role, &self.main_model) {
            (Role::Main, Some(m)) => vec![m.as_str()],
            _ => chain.iter().map(String::as_str).collect(),
        };
        for spec in models {
            let model = spec;
            if self.exhausted.lock().unwrap().contains(spec) { last_err = anyhow!("{spec}: daily limit reached earlier this run"); continue; }
            let (url, key_var, id) = route(spec);
            let Ok(api_key) = std::env::var(key_var) else { last_err = anyhow!("{spec}: {key_var} not set"); continue };
            // ponytail: retry-after backoff instead of a token-bucket limiter; add pacing from x-ratelimit headers if 429s dominate traces.
            let mut attempt = 0;
            // Raised after a generation error so the retry isn't the same greedy output again.
            let mut temp = temperature;
            while attempt < 3 {
                attempt += 1;
                let loose = self.loose_json.lock().unwrap().contains(spec);
                let (sys_text, format) = if loose {
                    (format!("{system}\n\nReply with one JSON object only, matching this JSON schema exactly:\n{schema}"), json!({"type": "json_object"}))
                } else {
                    (system.to_string(), json!({"type": "json_schema", "json_schema": {"name": "reply", "strict": true, "schema": schema}}))
                };
                let body = json!({
                    "model": id,
                    "temperature": temp,
                    "messages": [{"role": "system", "content": sys_text}, {"role": "user", "content": user}],
                    "response_format": format,
                });
                // Groq bodies hash exactly as before, so existing cache entries stay valid.
                let ck = cache_key(&body, sample);
                if let Some(v) = self.cache_get(&ck)? {
                    self.log(step, model, 0, 0, true);
                    return Ok(v);
                }
                // Pace instead of erroring: if this request won't fit in the model's remaining tokens for the
                // current minute, wait for the window to reset (Groq's x-ratelimit headers; ~4 chars per token).
                let need = (sys_text.len() + user.len()) as f64 / 4.0 + 1500.0;
                let pause = self.window.lock().unwrap().get(spec).and_then(|&(left, reset)| {
                    (left < need).then(|| reset.saturating_duration_since(Instant::now())).filter(|d| !d.is_zero())
                });
                if let Some(d) = pause {
                    let d = d.min(Duration::from_secs(60));
                    crate::ui::note(&format!("pacing {model}: {:.0}s until its token window resets", d.as_secs_f64()));
                    tokio::time::sleep(d + Duration::from_millis(300)).await;
                }
                let t = Instant::now();
                let resp = match self.http.post(url).bearer_auth(&api_key).json(&body).send().await {
                    Ok(r) => r,
                    Err(e) => { last_err = e.into(); continue; }
                };
                let status = resp.status();
                {
                    let hv = |k: &str| resp.headers().get(k).and_then(|v| v.to_str().ok()).map(str::to_string);
                    if let (Some(left), Some(reset)) = (hv("x-ratelimit-remaining-tokens").and_then(|s| s.parse::<f64>().ok()),
                                                        hv("x-ratelimit-reset-tokens").and_then(|s| parse_duration(&s))) {
                        self.window.lock().unwrap().insert(spec.to_string(), (left, Instant::now() + Duration::from_secs_f64(reset)));
                    }
                }
                if status.as_u16() == 429 {
                    let h = |k: &str| resp.headers().get(k).and_then(|v| v.to_str().ok()).and_then(|s| s.parse::<f64>().ok());
                    // Groq sends retry-after (s); OpenRouter sends x-ratelimit-reset (epoch ms).
                    let now_ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as f64).unwrap_or(0.0);
                    let wait = h("retry-after").or_else(|| h("x-ratelimit-reset").map(|r| ((r - now_ms) / 1000.0).max(1.0))).unwrap_or(5.0);
                    last_err = anyhow!("{model}: rate limited (retry-after {wait:.0}s)");
                    // Waiting beats falling back: the fallback model is weaker at strict JSON.
                    if wait <= 60.0 && attempt < 3 {
                        crate::ui::note(&format!("rate limited on {model}, waiting {wait:.0}s"));
                        tokio::time::sleep(Duration::from_secs_f64(wait + 0.5)).await;
                        continue;
                    }
                    if wait > 60.0 { self.exhausted.lock().unwrap().insert(spec.to_string()); }
                    break; // long wait: try the next model
                }
                let v: Value = resp.json().await.unwrap_or(Value::Null);
                if !status.is_success() {
                    let msg = v["error"]["message"].as_str().unwrap_or("?").to_string();
                    last_err = anyhow!("{model}: HTTP {status}: {msg}");
                    // A provider/model without json_schema support: switch it to json_object + schema-in-prompt once.
                    let unsupported = (status.as_u16() == 400 || status.as_u16() == 404)
                        && !loose && (msg.contains("response_format") || msg.contains("json_schema") || msg.contains("structured"));
                    if unsupported { self.loose_json.lock().unwrap().insert(spec.to_string()); attempt -= 1; continue; }
                    // Groq rejects generations that break the schema, or where gpt-oss calls a built-in tool we never
                    // offered; resampling at a slightly higher temperature usually succeeds.
                    let bad_gen = status.as_u16() == 400 && (msg.contains("JSON") || msg.contains("called a tool"));
                    if bad_gen && attempt < 3 { temp = (temp + 0.3).min(1.0); continue; }
                    // Overloaded providers (Gemini 503s): back off 2, 4, 8 s instead of retrying instantly.
                    if status.is_server_error() {
                        tokio::time::sleep(Duration::from_secs(1 << attempt.min(3))).await;
                        continue;
                    }
                    break;
                }
                let content = v["choices"][0]["message"]["content"].as_str().ok_or_else(|| anyhow!("{model}: empty reply"))?;
                // json_object mode may wrap the JSON in a code fence.
                let content = content.trim().trim_start_matches("```json").trim_start_matches("```").trim_end_matches("```").trim().to_string();
                self.log(step, model, t.elapsed().as_millis(), v["usage"]["total_tokens"].as_u64().unwrap_or(0), false);
                self.cache_put(&ck, &content)?;
                return Ok(content);
            }
        }
        bail!("{step}: all models failed: {last_err}")
    }

    fn log(&self, step: &str, model: &str, ms: u128, tokens: u64, cached: bool) {
        self.calls.lock().unwrap().push(CallLog { step: step.into(), model: model.into(), ms, tokens, cached });
    }
    fn cache_get(&self, k: &str) -> Result<Option<String>> {
        Ok(self.cache.lock().unwrap().query_row("SELECT v FROM cache WHERE k=?1", [k], |r| r.get(0)).optional()?)
    }
    fn cache_put(&self, k: &str, v: &str) -> Result<()> {
        self.cache.lock().unwrap().execute("INSERT OR REPLACE INTO cache (k, v) VALUES (?1, ?2)", [k, v])?;
        Ok(())
    }
}

fn cache_key(body: &Value, sample: u32) -> String {
    hex::encode(Sha256::digest(format!("{body}#{sample}").as_bytes()).as_slice())
}

/// JSON schema for `T` in the shape strict mode wants: inlined, every property required, no extras, no `format`.
pub fn strict_schema<T: JsonSchema>() -> Value {
    let settings = schemars::generate::SchemaSettings::draft2020_12().with(|s| s.inline_subschemas = true);
    let mut v = serde_json::to_value(settings.into_generator().into_root_schema_for::<T>()).unwrap();
    if let Some(o) = v.as_object_mut() {
        o.remove("$schema");
        o.remove("title");
    }
    strictify(&mut v);
    v
}

fn strictify(v: &mut Value) {
    match v {
        Value::Object(o) => {
            o.remove("format");
            o.remove("default");
            if let Some(Value::Object(props)) = o.get("properties") {
                let keys: Vec<Value> = props.keys().map(|k| Value::String(k.clone())).collect();
                o.insert("required".into(), Value::Array(keys));
                o.insert("additionalProperties".into(), Value::Bool(false));
            }
            o.values_mut().for_each(strictify);
        }
        Value::Array(a) => a.iter_mut().for_each(strictify),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(serde::Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct Inner { x: f64, #[serde(default)] tags: Vec<String> }
    #[derive(serde::Deserialize, JsonSchema)]
    #[allow(dead_code)]
    struct Outer { name: String, inner: Vec<Inner> }

    #[test]
    fn parses_groq_reset_durations() {
        assert_eq!(parse_duration("7.66s"), Some(7.66));
        assert_eq!(parse_duration("1m2.5s"), Some(62.5));
        assert_eq!(parse_duration("120ms"), Some(0.12));
        assert_eq!(parse_duration("2h3m4s"), Some(7384.0));
    }

    #[test]
    fn schema_is_strict() {
        let s = strict_schema::<Outer>().to_string();
        assert!(!s.contains("$ref") && !s.contains("format") && !s.contains("$schema"), "{s}");
        let v = strict_schema::<Outer>();
        let inner = &v["properties"]["inner"]["items"];
        assert_eq!(inner["required"].as_array().unwrap().len(), 2, "{inner}");
        assert_eq!(inner["additionalProperties"], false);
    }
}

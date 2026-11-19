//! What the model returns (Answer, Claim, Bayes) and what the tools produce (Evidence).
//! Empty strings / empty vecs mean "none": strict JSON mode handles them better than nulls.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, JsonSchema, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Tag {
    /// Read off a computation the agent ran.
    Observed,
    /// Stated by a retrieved source, backed by a verbatim quote.
    Sourced,
    /// Reasoned from listed evidence.
    Inferred,
    /// Well-established general knowledge, not checked against evidence this session. No computed numbers.
    Knowledge,
    /// Plausible but unsupported.
    Speculation,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Claim {
    pub text: String,
    pub tag: Tag,
    /// Evidence ids, e.g. ["C1"] or ["S3"].
    pub evidence: Vec<String>,
    /// Verbatim quote from the source (sourced claims only), else "".
    pub quote: String,
    /// Names of the RESULT lines this claim's numbers come from (observed claims), e.g. ["ppv"]; else [].
    #[serde(default)]
    pub results: Vec<String>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct BayesStep {
    pub evidence: String,
    /// P(evidence | hypothesis) / P(evidence | not hypothesis).
    pub likelihood_ratio: f64,
    pub reason: String,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Bayes {
    pub hypothesis: String,
    pub prior: f64,
    pub prior_reason: String,
    pub steps: Vec<BayesStep>,
}

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Answer {
    /// One or two sentences that answer the question directly.
    pub direct_answer: String,
    pub claims: Vec<Claim>,
    /// Optional explicit belief updates for yes/no questions weighed by evidence.
    pub bayes: Vec<Bayes>,
    /// What is still uncertain and what would settle it.
    pub unknowns: Vec<String>,
    /// The model's own 0–1 confidence (one input to the score, never trusted alone).
    pub self_confidence: f64,
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Computation,
    Source,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Evidence {
    pub id: String,
    pub kind: Kind,
    pub title: String,
    pub url: String,
    /// Source chunk, or the computation's stdout.
    pub text: String,
    /// Python that produced it (computations only).
    pub code: String,
    /// Source text is only an abstract or summary, not the full document.
    #[serde(default)]
    pub partial: bool,
}

//! Deterministic statistics the agent never leaves to the model:
//! number matching, exact-vs-simulation cross-checks, random-effects pooling, Bayesian updates.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

// ---------- numbers ----------

/// Numbers in free text as (value, decimal places). "16.2%" yields 16.2 and 0.162; "1,234" yields 1234.
pub fn numbers(s: &str) -> Vec<(f64, i32)> {
    let b = s.as_bytes();
    let mut out = vec![];
    let mut i = 0;
    while i < b.len() {
        let starts = b[i].is_ascii_digit() || (b[i] == b'-' && i + 1 < b.len() && b[i + 1].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()));
        if !starts || (i > 0 && (b[i - 1].is_ascii_alphabetic() || b[i - 1] == b'.')) {
            i += 1;
            continue;
        }
        let start = i;
        i += 1;
        while i < b.len() && (b[i].is_ascii_digit() || ((b[i] == b'.' || b[i] == b',') && i + 1 < b.len() && b[i + 1].is_ascii_digit())) {
            i += 1;
        }
        if i + 1 < b.len() && (b[i] == b'e' || b[i] == b'E') && (b[i + 1].is_ascii_digit() || (b[i + 1] == b'-' && i + 2 < b.len() && b[i + 2].is_ascii_digit())) {
            i += 2;
            while i < b.len() && b[i].is_ascii_digit() { i += 1; }
        }
        let raw = s[start..i].replace(',', "");
        if let Ok(v) = raw.parse::<f64>() {
            let mant = raw.split(['e', 'E']).next().unwrap_or("");
            let dec = mant.split('.').nth(1).map_or(0, |d| d.len() as i32);
            out.push((v, dec));
            if b.get(i) == Some(&b'%') { out.push((v / 100.0, dec + 2)); }
        }
    }
    out
}

/// Numbers in a claim that must be backed by a computation: anything with decimals or |x| >= 10, except bare years.
pub fn checkable(claim: &str) -> Vec<(f64, i32)> {
    // Confidence levels ("95% CI") are labels, not results.
    let mut c = claim.to_string();
    for lvl in ["90%", "95%", "99%", "90 %", "95 %", "99 %"] {
        for what in [" CI", " ci", " confidence", " credible", " interval", " prediction"] {
            c = c.replace(&format!("{lvl}{what}"), what);
        }
    }
    numbers(&c).into_iter()
        .filter(|&(v, d)| d > 0 || v.abs() >= 10.0)
        .filter(|&(v, d)| !(d == 0 && (1800.0..=2100.0).contains(&v)))
        .collect()
}

/// Does `claimed` (shown to `dec` places) match some number in `output`, after rounding?
pub fn supported(claimed: f64, dec: i32, output: &str) -> bool {
    let tol = 0.5 * 10f64.powi(-dec) + 1e-12;
    numbers(output).iter().any(|&(s, _)| (s - claimed).abs() <= tol || (s * 100.0 - claimed).abs() <= tol)
}

/// Named results printed by analyst code as `RESULT <name> = <value>`.
pub fn results(stdout: &str) -> std::collections::HashMap<String, f64> {
    stdout.lines().filter_map(|l| {
        let (name, v) = l.trim().strip_prefix("RESULT ")?.split_once('=')?;
        Some((name.trim().to_string(), v.trim().parse::<f64>().ok()?))
    }).collect()
}

// ---------- exact vs simulation ----------

#[derive(Serialize, Clone, Debug)]
pub struct CrossCheck {
    pub name: String,
    pub exact: f64,
    pub sim: f64,
    pub se: f64,
    /// Simulation noise small enough to catch a ~1% error in the exact value.
    pub precise: bool,
    pub agree: bool,
}

/// Parses `CHECK <name> exact=<x> sim=<y> se=<z>` lines printed by analyst code.
/// Agreement = within 3 standard errors (false alarm ~0.3% when the exact value is right).
/// Precision gate: se <= 0.25% of |exact| (floor 1e-4), so 3 se is under 1% — a noisy simulation
/// "agreeing" with a wrong exact value is what let int(0.1*1000) == 99 slip through once.
pub fn cross_checks(stdout: &str) -> Vec<CrossCheck> {
    stdout.lines().filter_map(|l| {
        let mut parts = l.trim().strip_prefix("CHECK ")?.split_whitespace();
        let name = parts.next()?.to_string();
        let mut get = |k: &str| -> Option<f64> { parts.next()?.strip_prefix(k)?.parse().ok() };
        let (exact, sim, se) = (get("exact=")?, get("sim=")?, get("se=")?);
        if !(exact.is_finite() && sim.is_finite() && se.is_finite()) { return None; }
        let precise = se <= 0.0025 * exact.abs().max(0.04);
        let agree = if se > 0.0 { (exact - sim).abs() <= 3.0 * se } else { (exact - sim).abs() <= 1e-9 * exact.abs().max(1.0) };
        Some(CrossCheck { name, exact, sim, se, precise, agree })
    }).collect()
}

// ---------- pooling (meta-analysis lite) ----------

#[derive(Serialize, Deserialize, JsonSchema, Clone, Debug)]
pub struct Effect {
    /// Source evidence id, e.g. "S2".
    pub evidence: String,
    /// Verbatim quote containing the estimate.
    pub quote: String,
    /// One of: odds_ratio, risk_ratio, hazard_ratio, mean_difference, standardized_mean_difference, correlation, proportion.
    pub measure: String,
    pub estimate: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    /// What the effect is about, e.g. "CoT vs no CoT, accuracy".
    pub outcome: String,
}

#[derive(Serialize, Clone, Debug)]
pub struct Pooled {
    pub measure: String,
    pub outcome: String,
    pub k: usize,
    pub estimate: f64,
    pub ci_low: f64,
    pub ci_high: f64,
    /// Share of variation due to real differences between studies (0–1).
    pub i2: f64,
    pub tau2: f64,
    pub evidence: Vec<String>,
}

const Z95: f64 = 1.959964;

fn transform(measure: &str) -> (fn(f64) -> f64, fn(f64) -> f64) {
    match measure {
        "odds_ratio" | "risk_ratio" | "hazard_ratio" => (f64::ln, f64::exp),
        "correlation" => (f64::atanh, f64::tanh),
        _ => (|x| x, |x| x),
    }
}

/// DerSimonian–Laird random-effects pooling of effects that share a measure. Needs k >= 2 valid CIs.
// ponytail: assumes independent studies and symmetric CIs on the transformed scale; no small-k (HKSJ) correction.
pub fn pool(effects: &[Effect]) -> Option<Pooled> {
    let measure = effects.first()?.measure.clone();
    let (f, inv) = transform(&measure);
    let pts: Vec<(f64, f64, &Effect)> = effects.iter()
        .filter(|e| e.measure == measure && e.ci_low < e.estimate && e.estimate < e.ci_high)
        .filter_map(|e| {
            let (y, lo, hi) = (f(e.estimate), f(e.ci_low), f(e.ci_high));
            let se = (hi - lo) / (2.0 * Z95);
            (y.is_finite() && se.is_finite() && se > 0.0).then_some((y, se * se, e))
        }).collect();
    let k = pts.len();
    if k < 2 { return None; }
    let w: Vec<f64> = pts.iter().map(|p| 1.0 / p.1).collect();
    let sw: f64 = w.iter().sum();
    let fixed = pts.iter().zip(&w).map(|(p, w)| w * p.0).sum::<f64>() / sw;
    let q: f64 = pts.iter().zip(&w).map(|(p, w)| w * (p.0 - fixed).powi(2)).sum();
    let c = sw - w.iter().map(|w| w * w).sum::<f64>() / sw;
    let tau2 = ((q - (k as f64 - 1.0)) / c).max(0.0);
    let wr: Vec<f64> = pts.iter().map(|p| 1.0 / (p.1 + tau2)).collect();
    let swr: f64 = wr.iter().sum();
    let y = pts.iter().zip(&wr).map(|(p, w)| w * p.0).sum::<f64>() / swr;
    let se = (1.0 / swr).sqrt();
    Some(Pooled {
        measure,
        outcome: pts[0].2.outcome.clone(),
        k,
        estimate: inv(y),
        ci_low: inv(y - Z95 * se),
        ci_high: inv(y + Z95 * se),
        i2: if q > 0.0 { ((q - (k as f64 - 1.0)) / q).max(0.0) } else { 0.0 },
        tau2,
        evidence: pts.iter().map(|p| p.2.evidence.clone()).collect(),
    })
}

// ---------- Bayesian updates ----------

/// Probability after each step, starting with the prior. Prior clamped to [0.01, 0.99], each LR to [0.05, 20],
/// so no single model-estimated likelihood can settle a question on its own.
pub fn bayes_trail(prior: f64, lrs: &[f64]) -> Vec<f64> {
    let p = prior.clamp(0.01, 0.99);
    let mut odds = p / (1.0 - p);
    let mut out = vec![p];
    for lr in lrs {
        odds *= lr.clamp(0.05, 20.0);
        out.push(odds / (1.0 + odds));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_match_after_rounding() {
        let out = "t=2.4103 p=0.01803 prob=0.16234 n=1,234";
        for (v, d) in checkable("t = 2.41, p = 0.018, 16.2% positive, n = 1,234, in 2019") {
            assert!(supported(v, d, out), "{v} ({d}dp) not found");
        }
        assert!(!supported(2.47, 2, out));
        assert_eq!(checkable("in 2019 there were 3 groups"), vec![]);
        assert_eq!(checkable("the 95% CI is 4.37 to 10.94").len(), 2);
    }

    #[test]
    fn cross_check_flags_disagreement() {
        let c = cross_checks("CHECK ppv exact=0.1624 sim=0.1626 se=0.0003\nCHECK bad exact=0.5 sim=0.4 se=0.001\nCHECK noisy exact=0.163765 sim=0.161569 se=0.001520");
        assert!(c[0].agree && c[0].precise && !c[1].agree);
        assert!(!c[2].precise, "the real failure: se too large to catch a 1% error");
    }

    #[test]
    fn pooling_matches_hand_calculation() {
        let e = |id: &str, y: f64| Effect { evidence: id.into(), quote: String::new(), measure: "mean_difference".into(), estimate: y, ci_low: y - Z95 * 0.1, ci_high: y + Z95 * 0.1, outcome: "x".into() };
        let p = pool(&[e("S1", 1.0), e("S2", 2.0)]).unwrap();
        // Q = 50, C = 100 -> tau2 = 0.49; random weights 2 each -> se 0.5; I2 = 49/50.
        assert!((p.estimate - 1.5).abs() < 1e-9 && (p.tau2 - 0.49).abs() < 1e-9 && (p.i2 - 0.98).abs() < 1e-9);
        assert!((p.ci_high - (1.5 + Z95 * 0.5)).abs() < 1e-6);
        // Ratios pool on the log scale: OR 2 and OR 8 (same SE) -> OR 4.
        let r = |y: f64| Effect { measure: "odds_ratio".into(), estimate: y, ci_low: y / 1.5, ci_high: y * 1.5, ..e("S", 0.0) };
        assert!((pool(&[r(2.0), r(8.0)]).unwrap().estimate - 4.0).abs() < 1e-9);
    }

    #[test]
    fn bayes_clamps_extremes() {
        let t = bayes_trail(0.5, &[3.0, 100.0]);
        assert!((t[1] - 0.75).abs() < 1e-12 && (t[2] - 60.0 / 61.0).abs() < 1e-12);
    }
}

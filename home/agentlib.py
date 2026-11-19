"""Tested statistics and probability helpers for the agent's Python sandbox.

Use these instead of writing formulas by hand. Every reported number should go through result();
every probability you also simulate should go through check().
Self-test: python -c "import agentlib; agentlib._selftest()"
"""
from fractions import Fraction
import math
import numpy as np
from scipy import stats

class R(dict):
    """Result dict whose KeyError names the valid keys, so a retry can fix a guessed key."""
    def __missing__(self, key):
        raise KeyError(f"{key!r} is not a key here; available keys: {sorted(self)}")

# ---------- reporting ----------

def result(name, value):
    """Print a named result the verifier can match: RESULT name = value."""
    v = float(value) if not isinstance(value, (int, np.integer)) else int(value)
    print(f"RESULT {name} = {v!r}")
    return value

def check(name, exact, sim, se):
    """Print an exact-vs-simulation cross-check line and the exact value as a RESULT."""
    print(f"CHECK {name} exact={float(exact)!r} sim={float(sim)!r} se={float(se)!r}")
    result(name, exact)

def frac(x):
    """Exact Fraction from a number or string. frac(0.9) == Fraction(9, 10) (goes through str, never float bits)."""
    return x if isinstance(x, Fraction) else Fraction(str(x))

def mc_mean(sample, n=10_000_000, batch=200_000, seed=0, time_budget=15.0):
    """Monte Carlo mean and standard error. sample(rng, size) -> array of per-trial values (0/1 for probabilities).
    Stops early after time_budget seconds and returns what it has (the sandbox kills scripts at 30 s), so slow
    path-by-path simulations give a noisier estimate instead of a crash."""
    import time
    rng = np.random.default_rng(seed)
    total = total_sq = 0.0
    done = 0
    t0 = time.monotonic()
    while done < n and (done == 0 or time.monotonic() - t0 < time_budget):
        k = min(batch, n - done)
        x = np.asarray(sample(rng, k), dtype=float)
        total += x.sum(); total_sq += (x * x).sum(); done += k
    mean = total / done
    var = max(total_sq / done - mean * mean, 0.0)
    return mean, math.sqrt(var / done)

# ---------- probability ----------

def bayes_posterior(prior, sensitivity, specificity, positive=True):
    """P(condition | one test result). Exact Fractions in, Fraction out."""
    p, se, sp = frac(prior), frac(sensitivity), frac(specificity)
    if positive:
        return p * se / (p * se + (1 - p) * (1 - sp))
    return p * (1 - se) / (p * (1 - se) + (1 - p) * sp)

def tests_chain(prior, tests):
    """Posterior after several conditionally independent tests: tests = [(sensitivity, specificity, positive_bool), ...]."""
    p = frac(prior)
    for se, sp, pos in tests:
        p = bayes_posterior(p, se, sp, pos)
    return p

def binom_tail(n, p, k, at_least=True):
    """P(X >= k) (or P(X <= k) if at_least=False) for X ~ Binomial(n, p)."""
    return float(stats.binom.sf(k - 1, n, p) if at_least else stats.binom.cdf(k, n, p))

def poisson_tail(mean, k, at_least=True):
    """P(X >= k) (or P(X <= k)) for X ~ Poisson(mean). For a rate per hour over t hours, mean = rate * t."""
    return float(stats.poisson.sf(k - 1, mean) if at_least else stats.poisson.cdf(k, mean))

def gamblers_ruin(start, target, p):
    """P(reach target before 0) starting at start, +1 w.p. p, -1 otherwise."""
    p = frac(p); q = 1 - p
    if p == q:
        return Fraction(start, target)
    r = q / p
    return (1 - r ** start) / (1 - r ** target)

def coupon_expected(probs):
    """Expected draws to collect every item, item i drawn with probability probs[i] (inclusion-exclusion; len <= ~20)."""
    ps = [frac(x) for x in probs]
    total = Fraction(0)
    m = len(ps)
    for mask in range(1, 1 << m):
        s = sum(ps[i] for i in range(m) if mask >> i & 1)
        bits = bin(mask).count("1")
        total += (1 if bits % 2 else -1) / s
    return total

def pattern_wait(pattern, p_heads=0.5):
    """Expected flips until `pattern` (string of H/T) first appears, via the absorbing Markov chain on prefixes."""
    n = len(pattern)
    p = {"H": frac(p_heads), "T": 1 - frac(p_heads)}
    def nxt(state, c):
        s = pattern[:state] + c
        while s and not pattern.startswith(s):
            s = s[1:]
        return len(s)
    # E[state] = 1 + sum_c p_c E[next]; E[n] = 0. Solve exactly with Fractions (Gaussian elimination).
    A = [[Fraction(0)] * n for _ in range(n)]
    b = [Fraction(1)] * n
    for s in range(n):
        A[s][s] += 1
        for c in "HT":
            t = nxt(s, c)
            if t < n:
                A[s][t] -= p[c]
    for i in range(n):
        piv = next(r for r in range(i, n) if A[r][i] != 0)
        A[i], A[piv] = A[piv], A[i]; b[i], b[piv] = b[piv], b[i]
        for r in range(n):
            if r != i and A[r][i] != 0:
                f = A[r][i] / A[i][i]
                A[r] = [a - f * c for a, c in zip(A[r], A[i])]; b[r] -= f * b[i]
    return b[0] / A[0][0]

# ---------- statistics ----------

def describe(df):
    """n, missing and dtype per column, printed."""
    print(f"rows = {len(df)}, columns = {list(df.columns)}")
    for c in df.columns:
        print(f"  {c}: dtype={df[c].dtype}, missing={int(df[c].isna().sum())}, unique={df[c].nunique()}")

def cohens_d(a, b):
    """Cohen's d with the pooled SD, and Hedges' g (small-sample corrected)."""
    a, b = np.asarray(a, float), np.asarray(b, float)
    n1, n2 = len(a), len(b)
    sp = math.sqrt(((n1 - 1) * a.var(ddof=1) + (n2 - 1) * b.var(ddof=1)) / (n1 + n2 - 2))
    d = (a.mean() - b.mean()) / sp
    return R({"d": d, "g": d * (1 - 3 / (4 * (n1 + n2) - 9))})

def welch(a, b):
    """Welch two-sample t-test (a minus b): t, df, p, mean difference and its 95% CI, Cohen's d."""
    a, b = np.asarray(a, float), np.asarray(b, float)
    r = stats.ttest_ind(a, b, equal_var=False)
    ci = r.confidence_interval()
    return R({"t": float(r.statistic), "df": float(r.df), "p": float(r.pvalue), "diff": float(a.mean() - b.mean()),
            "ci_low": float(ci.low), "ci_high": float(ci.high), **cohens_d(a, b)})

def paired_t(before, after):
    """Paired t-test on before minus after: t, df, p, mean difference and 95% CI."""
    x, y = np.asarray(before, float), np.asarray(after, float)
    r = stats.ttest_rel(x, y)
    ci = r.confidence_interval()
    return R({"t": float(r.statistic), "df": float(r.df), "p": float(r.pvalue), "diff": float((x - y).mean()),
            "ci_low": float(ci.low), "ci_high": float(ci.high)})

def table_test(table):
    """2-D count table: chi-square, or Fisher's exact when 2x2 and any expected count < 5."""
    t = np.asarray(table)
    chi2, p, dof, exp = stats.chi2_contingency(t)
    if t.shape == (2, 2) and (exp < 5).any():
        odds, pf = stats.fisher_exact(t)
        return R({"test": "fisher", "odds_ratio": float(odds), "p": float(pf)})
    return R({"test": "chi2", "chi2": float(chi2), "df": int(dof), "p": float(p)})

def bootstrap_ci(data, stat=np.mean, n=10_000, seed=0, level=0.95):
    """Percentile bootstrap CI for stat(data)."""
    x = np.asarray(data, float)
    rng = np.random.default_rng(seed)
    boots = np.array([stat(x[rng.integers(0, len(x), len(x))]) for _ in range(n)])
    lo, hi = np.quantile(boots, [(1 - level) / 2, (1 + level) / 2])
    return float(lo), float(hi)

def holm(pvals):
    """Holm-adjusted p-values, same order as input."""
    p = np.asarray(pvals, float); m = len(p); order = np.argsort(p)
    adj = np.empty(m); running = 0.0
    for rank, i in enumerate(order):
        running = max(running, (m - rank) * p[i]); adj[i] = min(running, 1.0)
    return adj.tolist()

def bh(pvals):
    """Benjamini-Hochberg adjusted p-values, same order as input."""
    p = np.asarray(pvals, float); m = len(p); order = np.argsort(p)[::-1]
    adj = np.empty(m); running = 1.0
    for k, i in enumerate(order):
        running = min(running, p[i] * m / (m - k)); adj[i] = running
    return adj.tolist()

def ols(x, y):
    """Simple regression of y on x with diagnostics: slope, intercept, 95% CI, R², Cook's distances,
    influential points (Cook's D > 4/n), slope without them, and the robust Theil-Sen slope."""
    x, y = np.asarray(x, float), np.asarray(y, float)
    n = len(x)
    r = stats.linregress(x, y)
    t = stats.t.ppf(0.975, n - 2)
    fitted = r.intercept + r.slope * x
    resid = y - fitted
    mse = (resid ** 2).sum() / (n - 2)
    lev = 1 / n + (x - x.mean()) ** 2 / ((x - x.mean()) ** 2).sum()
    cooks = resid ** 2 / (2 * mse) * lev / (1 - lev) ** 2
    infl = np.where(cooks > 4 / n)[0].tolist()
    keep = np.setdiff1d(np.arange(n), infl)
    slope_wo = stats.linregress(x[keep], y[keep]).slope if len(infl) and len(keep) > 2 else r.slope
    return R({"slope": float(r.slope), "intercept": float(r.intercept), "slope_ci_low": float(r.slope - t * r.stderr),
            "slope_ci_high": float(r.slope + t * r.stderr), "r2": float(r.rvalue ** 2), "p": float(r.pvalue),
            "influential_rows": infl, "max_cooks_d": float(cooks.max()), "slope_without_influential": float(slope_wo),
            "theil_sen_slope": float(stats.theilslopes(y, x)[0])})

def stratified_rates(df, outcome, group, stratum):
    """Mean of `outcome` by group overall and within each stratum; flags a Simpson reversal
    (the overall ordering of two groups is reversed in every stratum)."""
    overall = df.groupby(group)[outcome].mean()
    within = df.groupby([stratum, group])[outcome].mean().unstack(group)
    flag = False
    if len(overall) == 2:
        g1, g2 = overall.index
        sign = np.sign(overall[g1] - overall[g2])
        signs = np.sign(within[g1] - within[g2]).dropna()
        flag = bool(sign != 0 and len(signs) > 0 and (signs == -sign).all())
    return R({"overall": overall.round(4).to_dict(), "within": within.round(4).to_dict("index"), "simpson_reversal": flag})

# ---------- automatic data audit ----------

def audit(path, max_cat=10):
    """Deterministic first look at a CSV: shape, types, missing values, MAD outliers, and Simpson checks
    across every (binary-or-numeric outcome, 2-level group, categorical stratum) triple."""
    import pandas as pd
    df = pd.read_csv(path)
    print(f"AUDIT {path}: {len(df)} rows x {len(df.columns)} columns")
    num = [c for c in df.columns if pd.api.types.is_numeric_dtype(df[c])]
    cat = [c for c in df.columns if c not in num and df[c].nunique() <= max_cat]
    cat += [c for c in num if df[c].nunique() <= 2 and c not in cat]
    for c in df.columns:
        line = f"  {c}: {df[c].dtype}, missing {int(df[c].isna().sum())}, unique {df[c].nunique()}"
        if c in num and df[c].nunique() > 2:
            x = df[c].dropna(); med = x.median(); mad = (x - med).abs().median()
            if mad > 0:
                out = x[(x - med).abs() > 5 * 1.4826 * mad]
                if len(out):
                    line += f", OUTLIERS (>5 robust SD) at rows {out.index.tolist()[:10]} values {out.round(4).tolist()[:10]}"
        print(line)
    outcomes = [c for c in num]
    for o in outcomes:
        for g in cat:
            if g == o or df[g].nunique() != 2:
                continue
            for s in cat:
                if s in (g, o) or df[s].nunique() < 2:
                    continue
                r = stratified_rates(df, o, g, s)
                if r["simpson_reversal"]:
                    print(f"  SIMPSON REVERSAL: {o} by {g} reverses within every level of {s}: overall {r['overall']}, within {r['within']}")
    return df

# ---------- self-test ----------

def _selftest():
    assert frac(0.9) == Fraction(9, 10) and frac("0.003") == Fraction(3, 1000)
    assert abs(float(bayes_posterior(0.02, 0.95, 0.90)) - 0.162393) < 1e-6
    assert abs(float(tests_chain("0.003", [("0.98", "0.97", True), ("0.92", "0.995", True)])) - 0.947606) < 1e-6
    assert abs(binom_tail(5, 0.5, 3) - 0.5) < 1e-12
    assert abs(poisson_tail(4.2 * 1.5, 7) - 0.441767) < 1e-6
    assert abs(float(gamblers_ruin(7, 15, "0.47")) - 0.260472) < 1e-6
    assert abs(float(coupon_expected([0.3, 0.2, 0.2, 0.1, 0.1, 0.1])) - 19.369048) < 1e-6
    assert pattern_wait("THTH") == 20 and pattern_wait("HH") == 6
    m, se = mc_mean(lambda rng, k: rng.random(k) < 0.25, n=2_000_000)
    assert abs(m - 0.25) < 4 * se
    import time
    t = time.monotonic(); mc_mean(lambda rng, k: (time.sleep(0.2), rng.random(k))[1], n=10**9, time_budget=0.5)
    assert time.monotonic() - t < 2.0  # stops on budget
    assert [round(v, 4) for v in holm([0.01, 0.04, 0.03])] == [0.03, 0.06, 0.06]
    assert [round(v, 4) for v in bh([0.01, 0.04, 0.03])] == [0.03, 0.04, 0.04]
    x = np.arange(1, 21, dtype=float); y = 2 + 0.5 * x; y[19] = 60
    o = ols(x, y)
    assert 19 in o["influential_rows"] and abs(o["slope_without_influential"] - 0.5) < 1e-9 and abs(o["theil_sen_slope"] - 0.5) < 1e-9
    try:
        welch([1, 2, 3], [2, 3, 4])["mean_diff"]
        raise AssertionError("expected KeyError")
    except KeyError as e:
        assert "available keys" in str(e) and "diff" in str(e)
    print("agentlib selftest OK")

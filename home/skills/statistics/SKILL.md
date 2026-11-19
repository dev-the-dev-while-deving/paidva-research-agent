---
name: statistics
triggers: test, significant, p-value, regression, correlation, mean, median, variance, sample, csv, data, dataset, effect, confidence interval, anova, chi, survey, trial, experiment
---
1. Load the data; report n, missing values and type per column before anything else.
2. State the question as a hypothesis (H0 and H1) and the outcome measure.
3. Check assumptions in code: normality (Shapiro-Wilk for n<5000, plus skew), equal variances (Levene), independence (from the design).
4. Choose the test: two groups → Welch's t (default) or Mann-Whitney if badly non-normal; paired → paired t or Wilcoxon; 3+ groups → Welch ANOVA or Kruskal-Wallis; categorical → chi-square, or Fisher's exact if any expected count < 5; relationships → Pearson if linear and normal, else Spearman; prediction → OLS with residual checks (statsmodels).
5. Report effect size with a 95% CI (Cohen's d / Hedges' g, odds ratio, r, or R²), not only p. Name the variant: Cohen's d uses the pooled SD sqrt(((n1-1)s1² + (n2-1)s2²)/(n1+n2-2)); if you divide by sqrt((s1²+s2²)/2) instead, call it d_av. Bootstrap the CI (10,000 resamples, fixed seed) when no closed form fits.
6. Several tests → adjust p-values (Holm by default) and say so.
7. Say whether the effect is practically meaningful, and name one thing that could make the result misleading (confounding, selection, small n, outliers).

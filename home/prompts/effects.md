Extract quantitative effect estimates from the sources below, so they can be pooled.

Include an estimate only if the source text gives the estimate AND its 95% confidence interval. Copy `quote` character for character from the source text; it must contain the estimate. Use the source's S id.

measure must be one of: odds_ratio, risk_ratio, hazard_ratio, mean_difference, standardized_mean_difference, correlation, proportion.
outcome: a short label of what is compared (e.g. "CoT vs direct, GSM8K accuracy"). Use the same label for estimates of the same effect.

If no source reports an estimate with an interval, return an empty list. Never estimate or convert numbers yourself.

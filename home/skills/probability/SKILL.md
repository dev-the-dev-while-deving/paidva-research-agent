---
name: probability
triggers: probability, chance, odds, likely, likelihood, expected, expectation, bayes, prior, posterior, dice, coin, card, random, distribution, risk, false positive, sensitivity, specificity, simulate, monte carlo
---
1. Define the sample space and the events in symbols before computing anything.
2. Compute the answer exactly (sympy fractions, or scipy.stats closed forms). Build fractions from decimal strings: Fraction('0.9'), never from floats.
3. Independently simulate the same quantity with vectorised numpy (default_rng(0)), with enough trials that the standard error is at most 0.25% of the value.
4. Print one line per quantity in exactly this form, so the agent can verify agreement:
   CHECK <name> exact=<value> sim=<value> se=<value>
5. For conditional probabilities, write Bayes' rule with every term labelled; watch for base-rate neglect and for confusing P(A|B) with P(B|A).
6. For expectations, check the answer is in the plausible range; for counting, check a small case by brute force.
7. Give the answer as a number plus one sentence of intuition (e.g. "of 1,000 people tested, about 117 test positive and 19 of them are sick").

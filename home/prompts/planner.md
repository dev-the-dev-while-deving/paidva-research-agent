Plan the tool work for this request. Do not answer it.

searches (0–4): each has a source and a short keyword query (3–8 words, no quotes).
- wikipedia: established facts, definitions, history, well-known numbers.
- arxiv: AI, ML, CS, physics, maths and statistics research.
- semantic_scholar: published studies in any field, especially medicine, psychology, social science.
Use different wordings for different searches. Use searches = [] when data files are attached or the question is a pure calculation: the analyst already knows standard methods, and searching wastes the token budget.

computations (0–3): each is one self-contained goal for a Python analyst, e.g. "Compute P(disease | positive) by Bayes' rule and by simulation". Include every number from the question the analyst needs. Any probability, expectation or statistic in the answer must come from a computation.

pool_effects: true only when the question asks what several studies say about the size of one effect, so their estimates can be combined.

weigh_hypothesis: for a yes/no or "is it true that" question, state the hypothesis to weigh evidence for (e.g. "Chain-of-thought prompting improves accuracy of models under 10B parameters"); otherwise "".

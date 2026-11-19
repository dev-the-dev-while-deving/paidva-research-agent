# How research agents are evaluated (as of 2026-11)

Scope: facts below come from abstracts and excerpts retrieved in this run. "not found" means no source was located; "not confirmed" means the source was located but the detail was not in the retrieved text. Figures are as reported by each source.

## Benchmarks

| Name | Measures | Grading | Size | URL |
|-|-|-|-|-|
| SimpleQA (OpenAI, 2024) | Short fact-seeking factual recall | Prompted ChatGPT classifier grades each answer correct / incorrect / not attempted against a reference. Authors hand-checked 300 completions and found 2 disagreements with the grader | 4,326 questions | https://arxiv.org/abs/2411.04368 |
| SimpleQA Verified (2025) | SimpleQA with fixes for noisy labels, topical bias and redundancy | Not confirmed in this run | 1,000 prompts | https://arxiv.org/abs/2509.07968 |
| BrowseComp (OpenAI, 2025) | Persistent browsing to find hard-to-locate, entangled facts | Answer checked against a reference; grader details not confirmed | 1,266 questions | https://arxiv.org/abs/2504.12516 |
| GAIA (Mialon et al., 2023) | Real-world questions needing reasoning, browsing, multimodality and tool use | Not confirmed in this run. Reported gap: humans 92%, GPT-4 with plugins 15% | 466 questions; answers withheld for 300 to power a leaderboard | https://arxiv.org/abs/2311.12983 |
| FRAMES (Google, 2024) | Multi-hop factuality, retrieval and reasoning in end-to-end RAG | Not confirmed in this run. Baseline accuracy 0.40 (abstract) | 824 questions (count from secondary source parallel.ai) | https://arxiv.org/abs/2409.12941 |
| Humanity's Last Exam (CAIS and Scale, 2025) | Expert closed-ended questions, multiple-choice and short-answer | Automated grading against unambiguous known answers. Judge model not confirmed. Calibration error reported; o3-mini scored 13.4% accuracy and 80% calibration error in the original paper | 2,500 questions | https://arxiv.org/abs/2501.14249 |
| DeepResearch Bench (Du et al., 2025) | Quality of deep-research reports and citation grounding | Reference-based LLM judging with adaptive criteria (RACE). Citation-support judging by a separate LLM (FACT) | 100 PhD-level tasks across 22 fields (50 English, 50 Chinese) | https://arxiv.org/abs/2506.11763 |
| GSM-Symbolic (Mirzadeh et al., 2024) | Math reasoning robustness, using templated variants of GSM8K | Final-answer correctness per generated instance. Adding one irrelevant clause dropped accuracy by up to 65% (abstract) | Templates derived from GSM8K; count not confirmed | https://arxiv.org/abs/2410.05229 |
| MATH-Perturb | Perturbed MATH variants | not found | not found | not found |
| LiveDRBench, ResearchQA, ResearchBench, DeepResearchGym | 2025-26 deep-research benchmarks | not searched in this run | not found | not found |

## Factuality and citation metrics

| Name | Measures | Grading | Size | URL |
|-|-|-|-|-|
| FActScore (Min et al., 2023) | Precision of atomic facts in long biographies | Text split into atomic facts; each checked against a knowledge source (Wikipedia). Automated estimator reported under 2% error versus humans. ChatGPT scored 58% | Biographies of people; 6,500 generations from 13 LMs in the study | https://arxiv.org/abs/2305.14251 |
| SAFE / LongFact (Wei et al., 2024) | Long-form factuality with F1 that balances precision and a length-based recall target | LLM agent splits responses into facts and checks each with Google Search. Agrees with crowdsourced annotators 72% of the time on about 16k facts; wins 76% of 100 disagreement cases | LongFact: thousands of prompts over 38 topics | https://arxiv.org/abs/2403.18802 |
| ALCE (Gao et al., 2023) | Citation quality of generated text: fluency, correctness, citation support | Automatic: MAUVE for fluency, claim-correctness metric, and an NLI model for whether citations support statements. Reports the best systems lack complete citation support 50% of the time on ELI5 | 3 datasets: ASQA, QAMPARI, ELI5 | https://arxiv.org/abs/2305.14627 |
| VeriScore | Claim-level verification | not found | not found | not found |
| AttributionBench | Attribution and citation faithfulness | not confirmed in this run | not found | not found |

## Abstention and false premise

| Name | Measures | Grading | Size | URL |
|-|-|-|-|-|
| AbstentionBench (2025) | Whether reasoning LLMs abstain on unanswerable questions: unknown answers, underspecification, false premises, subjectivity, outdated information | Abstention is a response that refrains from a direct answer. An LLM judge scores correctness | 20 datasets | https://arxiv.org/abs/2506.09038 |
| SelfAware (Yin et al., 2023) | Whether models recognise unanswerable questions | Not confirmed in this run | Dataset of unanswerable questions across several categories | https://arxiv.org/pdf/2305.18153 |
| Know What You Don't Know (SQuAD 2.0) | Unanswerable questions in reading comprehension (Rajpurkar et al., 2018) | Not confirmed in this run; cited as the source of unanswerable questions in SelfAware | not confirmed | not found as a standalone URL in this run |
| FalseQA | False-premise questions | not found as a specific paper in this run | not found | not found |

## Calibration metrics

- Expected calibration error (ECE) and temperature scaling: Guo et al. (2017) show modern neural networks are poorly calibrated and that temperature scaling is effective on most datasets. Their abstract does not define ECE or its binning; binning pitfalls were not confirmed in this run. https://arxiv.org/abs/1706.04599
- Verbalised confidence: Xiong et al. (2023) test prompting, sampling and aggregation across five LLMs, including GPT-4 and LLaMA 2 Chat. Their finding: verbalised confidence tends to be overconfident, calibration improves with scale, and no single method wins consistently. https://arxiv.org/abs/2306.13063
- HLE calibration: calibration error is reported as a root-mean-square gap between stated confidence and accuracy, with models giving a 0-100% confidence. https://casrai.org/guides/humanitys-last-exam and https://scale.com/blog/o3-o4-mini-calibration
- Selective prediction: risk-coverage curves and AURC are the standard summary. The abstract states that finite-sample AURC estimates are susceptible to bias and variance on small datasets. https://arxiv.org/abs/2410.15361 ; AURC used as an alignment objective in https://arxiv.org/html/2607.03528v1
- HELM reports calibration as one of seven metrics across 16 core scenarios. https://arxiv.org/abs/2211.09110
- Brier score: not found in this run.

## LLM-as-judge reliability

- Zheng et al. (2023): GPT-4 judges reach over 80% agreement with human preferences on MT-Bench and Chatbot Arena, the same level as human-human agreement. https://arxiv.org/abs/2306.05685
- Position bias: GPT-4 was consistent across swapped positions 65.0% of the time under the default prompt, and favoured the first position in 30.0% of cases. claude-v1 was consistent 23.8% of the time under the default prompt, favouring the first position 75.0%. Renaming the assistants raised claude-v1 consistency to 56.2%. (Table 11 and Appendix D.1.)
- Verbosity bias: judges favoured longer answers under a "repetitive list" attack. Self-enhancement bias: GPT-4 favoured its own answers; the paper reports GPT-3.5 did not.
- ARES: lightweight judges are fine-tuned on synthetic data and calibrated with "a few hundred" human annotations over eight tasks. https://arxiv.org/abs/2311.09476
- FActScore's automated estimator and SAFE's agreement figures are listed above. DeepResearch Bench's project page reports a human-consistency score of 72.56 for RACE (Full). https://deepresearch-bench.github.io/
- Standard number of human spot-checks per item count: not found.

## Harness frameworks

| Name | What it offers | URL |
|-|-|-|
| Inspect AI (UK AI Security Institute, with Meridian Labs) | Python framework for evals; over 200 prebuilt evaluations including SimpleQA | https://inspect.aisi.org.uk/ and https://github.com/UKGovernmentBEIS/inspect_ai |
| lm-evaluation-harness (EleutherAI) | Few-shot evaluation framework for language models; latest release v0.4.13 | https://github.com/EleutherAI/lm-evaluation-harness |
| HELM (Stanford CRFM) | Multi-metric evaluation (accuracy, calibration, robustness, fairness, bias, toxicity, efficiency) on 42 scenarios and 30 models; releases raw prompts and completions | https://arxiv.org/abs/2211.09110 and https://github.com/stanford-crfm/helm |
| promptfoo | CLI and library for evals and red-teaming LLM apps and agents; MIT licensed; now part of OpenAI | https://github.com/promptfoo/promptfoo |
| OpenAI simple-evals / evals | Not confirmed in this run. An OpenAI cookbook guide covers moving from OpenAI Evals to promptfoo | https://github.com/openai/openai-cookbook/blob/main/examples/evaluation/moving-from-openai-evals-to-promptfoo.md |
| RAGAS | Automated RAG evaluation metrics | https://arxiv.org/abs/2309.15217 |
| ARES | Automated RAG evaluation with fine-tuned LM judges across context relevance, answer faithfulness and answer relevance | https://arxiv.org/abs/2311.09476 |

## Contamination

- Canary strings: BIG-bench embeds unique tokens so developers can detect and filter memorised copies. The method does not stop deliberate leakage. https://arxiv.org/html/2502.17521v2
- Temporal cutoff: LiveBench draws questions from recent sources and adds new ones every few months. LiveAoPSBench uses live Art of Problem Solving problems. https://arxiv.org/html/2502.17521v2
- Perturbed variants: GSM-Symbolic generates new instances from templates; the authors say the drop on original questions hints at possible contamination. https://arxiv.org/abs/2410.05229
- Held-out answers: GAIA releases 466 questions but keeps answers for 300 of them. https://arxiv.org/abs/2311.12983
- Label cleaning: SimpleQA Verified samples one question per reference URL to reduce overlap. https://arxiv.org/abs/2509.07968

## Statistical rigour

- Miller (2024) treats eval questions as draws from a super-population, gives formulas for standard errors, model differences and experiment planning, and recommends paired analysis. https://arxiv.org/abs/2411.00640 ; Anthropic summary: https://www.anthropic.com/research/statistical-approach-to-model-evals
- CLT interval: SE = s / sqrt(n); 95% CI = mean ± 1.96 × SE. For two unpaired models, the difference uses 1.96 × sqrt(SE_A² + SE_B²), which is less conservative than checking whether two separate intervals overlap. https://cameronrwolfe.substack.com/p/stats-llm-evals
- Paired differences: per-question score differences reduce variance when the two models' scores are positively correlated. https://cameronrwolfe.substack.com/p/stats-llm-evals
- CLT caution: arXiv 2503.01747 (2025) reports CLT-based intervals usually understate uncertainty with fewer than a few hundred datapoints. https://arxiv.org/abs/2503.01747
- Tests for paired comparisons: McNemar's test on discordant pairs for binary metrics, using an exact binomial test when there are fewer than 10 discordant pairs; paired t-test for continuous metrics. https://arxiv.org/html/2603.28769v1
- Worked interval: a Wilson 95% interval for 95 correct out of 100 is about 88.8% to 97.8% (third-party calculation in a blog post on GSM-Symbolic). https://desirivanova.com/post/gsm-symbolic/
- Sample size needed to detect a given difference: formula not extracted in this run. Miller (2024) covers experiment planning.

## Sources

- SimpleQA: https://arxiv.org/abs/2411.04368 ; https://openai.com/index/introducing-simpleqa/
- SimpleQA Verified: https://arxiv.org/abs/2509.07968
- BrowseComp: https://arxiv.org/abs/2504.12516
- GAIA: https://arxiv.org/abs/2311.12983
- FRAMES: https://arxiv.org/abs/2409.12941 ; https://parallel.ai/articles/frames-benchmark
- Humanity's Last Exam: https://arxiv.org/abs/2501.14249 ; https://lastexam.ai/ ; https://casrai.org/guides/humanitys-last-exam
- DeepResearch Bench: https://arxiv.org/abs/2506.11763 ; https://deepresearch-bench.github.io/
- GSM-Symbolic: https://arxiv.org/abs/2410.05229 ; https://desirivanova.com/post/gsm-symbolic/
- FActScore: https://arxiv.org/abs/2305.14251 ; https://github.com/shmsw25/factscore
- SAFE / LongFact: https://arxiv.org/abs/2403.18802 ; https://github.com/google-deepmind/long-form-factuality
- ALCE: https://arxiv.org/abs/2305.14627
- AbstentionBench: https://arxiv.org/abs/2506.09038
- SelfAware: https://arxiv.org/pdf/2305.18153
- Guo et al. 2017: https://arxiv.org/abs/1706.04599
- Xiong et al. 2023: https://arxiv.org/abs/2306.13063
- AURC: https://arxiv.org/abs/2410.15361 ; https://arxiv.org/html/2607.03528v1
- HELM: https://arxiv.org/abs/2211.09110
- Zheng et al. (MT-Bench judge): https://arxiv.org/abs/2306.05685
- ARES: https://arxiv.org/abs/2311.09476 ; RAGAS: https://arxiv.org/abs/2309.15217
- Inspect AI: https://inspect.aisi.org.uk/ ; https://github.com/UKGovernmentBEIS/inspect_ai
- lm-evaluation-harness: https://github.com/EleutherAI/lm-evaluation-harness
- promptfoo: https://github.com/promptfoo/promptfoo
- Contamination survey: https://arxiv.org/html/2502.17521v2
- Miller (2024): https://arxiv.org/abs/2411.00640
- CLT caution paper (2025): https://arxiv.org/abs/2503.01747
- Spark-LLM-Eval (McNemar, paired tests): https://arxiv.org/html/2603.28769v1
- Wolfe, statistics for LLM evals: https://cameronrwolfe.substack.com/p/stats-llm-evals

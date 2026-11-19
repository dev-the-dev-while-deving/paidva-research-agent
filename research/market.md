# Market landscape: AI research agents (as of 2026-11)

## Products

### OpenAI Deep Research
- a) Plans, searches the public web, uploaded files and connected apps, and writes a structured report. (https://help.openai.com/en/articles/10500283-deep-research-in-chatgpt)
- b) Outputs carry citations or source links and a "sources used" section; the user can review the research plan before it starts and interrupt it. (https://help.openai.com/en/articles/10500283-deep-research-in-chatgpt)
- c) No product-level confidence or uncertainty score found. The launch post says the model is weak at confidence calibration and often fails to convey uncertainty accurately. (https://openai.com/index/introducing-deep-research/)
- d) Launch post: hallucinated facts, incorrect inferences, minor citation formatting errors at launch. No independent product-specific failure rate found. (https://openai.com/index/introducing-deep-research/)
- e) Current help page: connected apps, site restriction, plan editing and mid-run interruption; No dated 2025–2026 roadmap found. (https://help.openai.com/en/articles/10500283-deep-research-in-chatgpt)

### Google Gemini Deep Research
- a) Gemini API research agent. "Deep Research" (speed-optimized) and "Deep Research Max" (longer test-time compute) were announced 2026-04-21, built on Gemini 3.1 Pro. (https://blog.google/innovation-and-ai/models-and-research/gemini-models/next-generation-gemini-deep-research/)
- b) Blog says analyses are "fully cited"; grounding uses Google Search, MCP servers, files or any subset. No claim-level verification method documented. (https://blog.google/innovation-and-ai/models-and-research/gemini-models/next-generation-gemini-deep-research/)
- c) Not found.
- d) Documented API limits: no custom function calling or structured output (https://aistudio.google.com/docs/deep-research). DeepTRACE tested Gemini configurations; per-product numbers not read.
- e) Apr 2026: native charts, collaborative plan review, MCP support; Cloud availability undated. (https://blog.google/innovation-and-ai/models-and-research/gemini-models/next-generation-gemini-deep-research/)

### Perplexity (Deep Research / Pro Search)
- a) Deep Research runs dozens of searches, reads many sources, and writes a report in roughly 2–4 minutes; Pro Search answers with cited sources. (https://www.perplexity.ai/hub/blog/introducing-perplexity-deep-research; https://www.perplexity.ai/help-center/en/articles/10352903-what-is-pro-search)
- b) Help center says to open cited sources to check claims; no automated verification described. (https://www.perplexity.ai/help-center/en/articles/10352903-what-is-pro-search)
- c) Not found.
- d) Liu et al. (2023) audited Perplexity among four engines: aggregate 51.5% of sentences fully supported, 74.5% of citations supportive (per-engine breakdown not read). (https://arxiv.org/abs/2304.09848)
- e) Vendor-reported: 21.1% on Humanity's Last Exam and 93.9% on SimpleQA. (https://www.perplexity.ai/hub/blog/introducing-perplexity-deep-research)

### Anthropic Claude Research
- a) Explores several angles of a question with successive searches across the web and enabled connectors (e.g., Google Workspace), then writes a cited report. (https://claude.com/resources/articles/research)
- b) Citations are described as easy to check; no verification pipeline documented. (https://claude.com/resources/articles/research)
- c) Not found.
- d) No independent citation-accuracy evaluation of Claude Research found; a secondary guide says so explicitly. (https://count.co/ai/claude-research)
- e) Mobile Research announced in an Anthropic post (date not shown). No dated roadmap found. (https://x.com/AnthropicAI/status/1923062113741770905; https://count.co/ai/claude-research)

### Elicit
- a) Research agent for scientific evidence: searches 138M academic papers and 545k clinical trials, produces cited reports, tables, and automates screening and extraction for systematic reviews. (https://elicit.com/)
- b) Claims sentence-level citations to underlying sources. (https://elicit.com/)
- c) Not found.
- d) Vendor case study: 1,502 of 1,511 extracted data points correct (99.4%) for one systematic review. No independent evaluation found. (https://elicit.com/)
- e) 2026 releases: Research Agent (Aug 4), API and MCP (Jul 15), shared Library (Sep 17), Routines (Sep 30). (https://elicit.com/)

### Consensus
- a) AI academic search over about 400M scholarly works; Deep Search, Medical mode, and a Consensus Meter for yes/no questions. (https://consensus.app/)
- b) Not found; the page describes licensed full text but no citation-verification method.
- c) Not calibrated; the Meter shows evidence agreement, not a probability. (https://consensus.app/)
- d) Not found.
- e) No dated roadmap found. (https://consensus.app/)

### scite
- a) Smart Citations: classifies each citing statement as supporting, contrasting or mentioning, with the surrounding context. (https://scite.ai/features; https://direct.mit.edu/qss/article/2/3/882/102990/scite-A-smart-citation-index-that-displays-the)
- b) Users can flag a classification as wrong; two independent reviewers check flagged items before a change is made. (https://direct.mit.edu/qss/article/2/3/882/102990/scite-A-smart-citation-index-that-displays-the)
- c) Each citation statement shows the model's confidence in its label (example given: 90%). (https://direct.mit.edu/qss/article/2/3/882/102990/scite-A-smart-citation-index-that-displays-the)
- d) Holdout F-scores in the scite paper: contrasting 0.590, supporting 0.648, mentioning 0.973 (author-run). A Bakker et al. critique (numbers not read) argued it misclassifies supporting citations; reply at the Hypothesis journal page. (https://direct.mit.edu/qss/article/2/3/882/102990/scite-A-smart-citation-index-that-displays-the; https://journals.indianapolis.iu.edu/index.php/hypothesis/article/view/28018)
- e) Product page cites 1.6B+ Smart Citations and 300M+ full-text articles; no dated roadmap found. (https://scite.ai/features)

### Undermind
- a) Deep Search: an iterative literature-search agent returning a curated paper list with match scores and reasons; reads figures and full texts. (https://www.undermind.ai/)
- b) Page says "every citation is verified"; method not documented. (https://www.undermind.ai/)
- c) Per-paper match scores only; no calibrated uncertainty documented. (https://www.undermind.ai/)
- d) Vendor-run benchmark (23 queries): 73% recall at 10 minutes versus 29–39% for frontier web agents. No independent test found. (https://www.undermind.ai/)
- e) Pro plan $16/month (annual) and an agent-connector offering. A third-party guide says the index moved to Semantic Scholar (about 200M papers). (https://www.undermind.ai/; https://casrai.org/guides/undermind-ai)

### OpenScholar (Ai2)
- a) Open retrieval-augmented model that searches 45M open-access papers and writes citation-backed answers. (https://www.nature.com/articles/s41586-025-10072-4)
- b) Iterative self-feedback with retrieval and citation verification during inference. (https://www.nature.com/articles/s41586-025-10072-4)
- c) Not found.
- d) Authors' ScholarQABench: GPT-4o fabricated citations in 78–90% of cases on recent-literature prompts; OpenScholar's citation accuracy matched human experts; PaperQA2 matched or beat it on citation accuracy but leaned on one or a few papers. (https://www.nature.com/articles/s41586-025-10072-4)
- e) Published in Nature (2025 DOI); the paper lists user feedback as future work. No dated roadmap found. (https://www.nature.com/articles/s41586-025-10072-4)

### FutureHouse PaperQA2 (and successors)
- a) Agent for literature search and cited, grounded answers; WikiCrow was built on it for summaries. (https://www.futurehouse.org/news/wikicrow; https://github.com/future-house/paper-qa)
- b) Answers cite retrieved papers; averages 14.5 papers per question. (https://arxiv.org/html/2409.13740v1)
- c) Can answer "insufficient information": 21.9% of LitQA2 answers (abstention, not a calibrated score). (https://arxiv.org/html/2409.13740v1)
- d) LitQA2: precision 85.2%, accuracy 66.0% (authors' benchmark). OpenScholar paper reports PaperQA2 answers rely on few papers and score lower on coverage and organization. (https://arxiv.org/html/2409.13740v1; https://www.nature.com/articles/s41586-025-10072-4)
- e) No successor product confirmed in searches; "successors" not found.

### Stanford STORM
- a) Writes Wikipedia-style articles from web search, using multi-perspective simulated question-asking to build an outline, then drafting with citations. (https://github.com/stanford-oval/storm; https://arxiv.org/abs/2402.14207)
- b) Writing stage generates the article from collected references with citations; no verification step described in the repo. (https://github.com/stanford-oval/storm)
- c) Not found.
- d) Paper reports automatic metrics and expert review (Wikipedia editors found it helpful for pre-writing); no citation-accuracy figure found. (https://storm-project.stanford.edu/research/storm/)
- e) Interactive prototype and Co-STORM variant exist; no dated roadmap found. (https://storm.genie.stanford.edu/; https://github.com/stanford-oval/storm)

### GPT Researcher (open source)
- a) Open-source agent that gathers web sources in parallel and writes a report with inline citations. (https://gptr.dev/)
- b) Site claims it validates information; method not documented. (https://gptr.dev/)
- c) Not found.
- d) Independent evaluation not found. The site's "#1 citation quality" claim is vendor-stated and unverified. (https://gptr.dev/)
- e) MCP data-source support is documented. No dated roadmap found. (https://smithery.ai/skills/assafelovic/gpt-researcher)

### Local Deep Research (open source)
- a) Open-source research assistant that decomposes questions and searches 10+ engines (arXiv, PubMed, local documents) in parallel. (https://github.com/LearningCircuit/local-deep-research)
- b) Citation tracking and cross-reference validation in detailed mode (community docs, may be inaccurate). (https://github.com/LearningCircuit/local-deep-research/blob/main/docs/features.md)
- c) Not found.
- d) Repo claims about 95% SimpleQA with a local model (self-reported); no independent evaluation found. (https://github.com/LearningCircuit/local-deep-research)
- e) No dated roadmap found. (https://github.com/LearningCircuit/local-deep-research/blob/main/docs/features.md)

### ChatGPT data analysis (code interpreter)
- a) Writes and runs Python in a stateful Jupyter environment over uploaded files, with charts and statistics. (https://help.openai.com/en/articles/8437071-data-analysis-with-chatgpt)
- b) Not found.
- c) Not found.
- d) Not found for ChatGPT. A 2026 study of a Claude agent (not ChatGPT) found repeatable execution errors; see table. (https://pmc.ncbi.nlm.nih.gov/articles/PMC13552643/)
- e) Not found.

### Julius AI
- a) Generates Python or Excel-style code for statistics and plots from uploaded CSVs or spreadsheets. (https://effortlessacademic.com/julius-ai-in-depth-review/)
- b) Shows generated code for inspection; the reviewer advises checking it. (https://effortlessacademic.com/julius-ai-in-depth-review/)
- c) Not found.
- d) Not found; no failure-rate study found.
- e) Now also covers presentations, websites, images; no dated roadmap found. (https://julius.ai/)

## Independent evaluations

| Study | Year | What was measured | Key numbers (as reported) | URL |
|-|-|-|-|-|
| Liu, Zhang, Liang: Evaluating Verifiability in Generative Search Engines | 2023 | Human audit of Bing Chat, NeevaAI, Perplexity, YouChat | 51.5% of sentences fully supported by citations; 74.5% of citations support their sentence | https://arxiv.org/abs/2304.09848 |
| Bhattacharyya et al.: High Rates of Fabricated and Inaccurate References in ChatGPT-Generated Medical Content (Cureus) | 2023 | ChatGPT-3.5 references in 30 short medical papers (115 references) | 47% fabricated, 46% authentic but inaccurate, 7% authentic and accurate; incorrect PMID in 93% of papers | https://pmc.ncbi.nlm.nih.gov/articles/PMC10277170/ |
| Venkit et al.: DeepTRACE | 2025 | Audit of web-search and deep-research configurations (GPT-4.5/5, Perplexity, Copilot, Gemini) | Citation accuracy 40–80%; large unsupported fractions; deep-research less overconfident but one-sided on debate queries | https://arxiv.org/abs/2509.04499 |
| OpenScholar (Ai2), Nature | 2025 (preprint 2024) | Citation accuracy on ScholarQABench; developer-run | GPT-4o fabricated citations 78–90% on recent-literature prompts; OpenScholar on par with human experts | https://www.nature.com/articles/s41586-025-10072-4 |
| Onweller et al.: Cited but Not Verified | 2026 (v1 May 7) | 14 LLMs; link works, relevance, factual check of cited claims | Link validity >94%, relevance >80% (strongest models); fact-check accuracy 39–77%, falling about 42% as tool calls rise from 2 to 150 | https://arxiv.org/abs/2605.06635 |
| Clinical LLM agent (Claude) evaluation | year not confirmed | 27 analysis runs, published ophthalmology dataset | 8 of 17 summaries fully satisfactory; 2 with clinically meaningful errors; SAP quality did not predict code correctness | https://pmc.ncbi.nlm.nih.gov/articles/PMC13552643/ |

## Recurring gaps across the market

- Cited sources that do not support the claim: DeepTRACE finds large unsupported fractions (https://arxiv.org/abs/2509.04499); Liu et al. report 51.5% fully supported sentences (https://arxiv.org/abs/2304.09848); Onweller et al. find 39–77% fact-check accuracy (https://arxiv.org/abs/2605.06635); OpenAI's launch post concedes hallucinations (https://openai.com/index/introducing-deep-research/).
- Working links do not mean correct claims: Onweller et al. find link validity above 94% alongside 39–77% fact-check accuracy (https://arxiv.org/abs/2605.06635).
- More retrieval did not raise citation accuracy: fact-check accuracy fell about 42% as tool calls scaled from 2 to 150 (https://arxiv.org/abs/2605.06635).
- Fabricated references: 47% in the 2023 ChatGPT-3.5 study (https://pmc.ncbi.nlm.nih.gov/articles/PMC10277170/); 78–90% for the GPT-4o baseline (https://www.nature.com/articles/s41586-025-10072-4). Older or baseline models, not current products.
- No calibrated confidence: OpenAI's launch post concedes weak calibration (https://openai.com/index/introducing-deep-research/); DeepTRACE finds highly confident one-sided answers on debate queries (https://arxiv.org/abs/2509.04499). Scite's per-statement label confidence is the only product-level confidence measure found (https://direct.mit.edu/qss/article/2/3/882/102990/scite-A-smart-citation-index-that-displays-the).
- Weak statistical execution and plausible summaries: the clinical agent study reports repeated execution errors and clinically meaningful summary errors (https://pmc.ncbi.nlm.nih.gov/articles/PMC13552643/).
- Abstention: only PaperQA2 documents it (21.9% "insufficient information", self-reported); no independent abstention evaluation found (https://arxiv.org/html/2409.13740v1).

## Sources

- https://help.openai.com/en/articles/10500283-deep-research-in-chatgpt
- https://openai.com/index/introducing-deep-research/
- https://blog.google/innovation-and-ai/models-and-research/gemini-models/next-generation-gemini-deep-research/
- https://aistudio.google.com/docs/deep-research
- https://docs.cloud.google.com/gemini-enterprise-agent-platform/agents/use-deep-research
- https://www.perplexity.ai/hub/blog/introducing-perplexity-deep-research
- https://www.perplexity.ai/help-center/en/articles/10352903-what-is-pro-search
- https://claude.com/resources/articles/research
- https://count.co/ai/claude-research
- https://x.com/AnthropicAI/status/1923062113741770905
- https://elicit.com/
- https://consensus.app/
- https://scite.ai/features
- https://direct.mit.edu/qss/article/2/3/882/102990/scite-A-smart-citation-index-that-displays-the
- https://journals.indianapolis.iu.edu/index.php/hypothesis/article/view/28018
- https://www.undermind.ai/
- https://casrai.org/guides/undermind-ai
- https://www.nature.com/articles/s41586-025-10072-4
- https://www.futurehouse.org/news/wikicrow
- https://github.com/future-house/paper-qa
- https://arxiv.org/html/2409.13740v1
- https://github.com/stanford-oval/storm
- https://arxiv.org/abs/2402.14207
- https://storm-project.stanford.edu/research/storm/
- https://storm.genie.stanford.edu/
- https://gptr.dev/
- https://smithery.ai/skills/assafelovic/gpt-researcher
- https://github.com/LearningCircuit/local-deep-research
- https://github.com/LearningCircuit/local-deep-research/blob/main/docs/features.md
- https://help.openai.com/en/articles/8437071-data-analysis-with-chatgpt
- https://pmc.ncbi.nlm.nih.gov/articles/PMC13552643/
- https://effortlessacademic.com/julius-ai-in-depth-review/
- https://julius.ai/
- https://arxiv.org/abs/2304.09848
- https://pmc.ncbi.nlm.nih.gov/articles/PMC10277170/
- https://arxiv.org/abs/2509.04499
- https://arxiv.org/abs/2605.06635

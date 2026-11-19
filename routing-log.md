# Routing log

| date | task type | route | result | note |
|---|---|---|---|---|
| 2026-11-19 | API client copying an existing pattern (arXiv, Semantic Scholar) | Haiku | pass | exact spec + test string; diff matched spec first time |

| 2026-11-19 | web research, conclusions + URLs (market survey) | Haiku | pass | 15 products; key studies re-checked against abstracts: accurate |
| 2026-11-19 | web research, conclusions + URLs (eval methods) | Haiku | partial | several items "not found"; numbers that were reported checked out |

| 2026-11-19 | parser copying an existing client pattern (arXiv HTML full text) | Haiku | pass | spec + fixture test; matched first time |

## Learned rules
- API clients that copy an existing in-repo pattern → Haiku is fine with a full spec and a fixture test.

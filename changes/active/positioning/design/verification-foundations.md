# Verification foundations guide

Status: agreed direction; implementation belongs to Task 2.

## Intent

The maintainer wants a dedicated starting guide teaching verification as a
discipline, independent of Wyrd adoption, grounded in citations. Its central
question is how to verify complex real systems reliably when behavior is
probabilistic, evidence is incomplete, requirements are contextual and a
plausible answer can conceal invalid process or data. Wyrd tutorials implement
these ideas; product mechanics are not the foundation's organizing structure.

Task 2 explicitly owns the page, comparisons, ML/agent failure cases, practical
method and source review. No new navigation section or runtime is authorized.

## Thesis and teaching boundaries

Infrastructure (capture, low-overhead execution, attribution, retention) and
adequate requirements are both hard. Agentic tools can help build infrastructure
and elicit/check requirements; those are opportunities, not proof that agents
made runtime verification possible for the first time or guarantee reliability.
Formal methods can model probability and runtime verification can use formal
properties. Avoid a false split between formal certainty and practical ambiguity.
State the scope of each claim and the assumptions/evidence on which it rests.

Use leakage, drift and skipped agent steps as three different lessons. Leakage
can invalidate apparently excellent evaluation and requires lineage/split/time
checks. Distribution drift is a signal to assess, not automatically proof of
wrong predictions; concept drift concerns changing input/outcome relationships.
A plausible agent answer does not prove required actions occurred. A missing
trace does not by itself establish a violation. Teach independent evidence,
negative cases, inconclusive outcomes and accountable application decisions.

## Maintainer-supplied references

Sources were opened and abstracts/selected relevant passages inspected during
design. This is a reading brief, not a systematic literature review or a claim
that every result has been independently reproduced.

- [Dennis et al., Practical verification of decision-making in agent-based autonomous systems](https://link.springer.com/article/10.1007/s10515-014-0168-9),
  published 2014, journal volume 2016: formal verification of agent decision-making;
  establishes historical depth predating contemporary LLM agents. Use its scope
  and assumptions when discussing formal methods, not as a universal guarantee.
- [Han et al., VerifiAgent: a Unified Verification Agent in Language Model Reasoning](https://aclanthology.org/2025.findings-emnlp.891/),
  EMNLP Findings 2025: model consistency/completeness plus adaptive external
  verification tools for reasoning tasks. Evidence for a particular approach,
  not proof that arbitrary systems/requirements can be reliably judged.
- [Shin, Automating epistemology: how AI reconfigures truth, authority, and verification](https://link.springer.com/article/10.1007/s00146-025-02560-y),
  published 2025, journal volume 2026: philosophical/sociotechnical framing of
  truth, assumptions and authority. It is not an engineering benchmark.
- [Koohestani, AgentGuard: Runtime Verification of AI Agents](https://arxiv.org/abs/2509.23864),
  2025 preprint: proof-of-concept combining runtime observations, learned models
  and probabilistic model checking. Useful counterexample to a hard division
  between formal and probabilistic verification; retain proof-of-concept limits.
- [Catalini, Hui and Wu, Some Simple Economics of AGI](https://arxiv.org/abs/2602.20946),
  2026 preprint (PDF inspected: v2): economic argument about verification
  bandwidth and correlated model-checker errors. Present as argument/model,
  not empirical proof that automated verification always fails or succeeds.

Before publication, check current versions, read relevant methods/limitations,
and add primary sources for definitions, runtime verification history, leakage,
drift and judge reliability. The supplied papers alone do not establish all
those lessons. Cite each source near the claim it supports and retain an
accessible bibliography; do not copy their vocabulary wholesale into the guide.

## Reader outcome

A newcomer can explain what is being verified, against which expectations, on
what evidence and with which limits; distinguish related disciplines; recognize
an attractive but invalid result; and design a small verification workflow
without needing a Wyrd account, API vocabulary or installation.

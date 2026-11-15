# Verification foundations guide

Status: locked by maintainer; implementation belongs to Task 2.

## Locked direction

The maintainer approved the dedicated tool-independent guide, core scholarly
reading set, teaching sequence and citation requirements. Task 2 owns delivery.
Prioritize peer-reviewed foundations, cite sources beside supported claims,
distinguish research from engineering guidance and preprints, and validate
methods/limitations for the claims actually published. The bibliography is a
research foundation, not a quota requiring every source on the starting page.
Product-specific procedures follow through links; the guide must be useful
without adopting Wyrd. No production guide or runtime change is implemented
by this approval.

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

## Core scholarly reading set — researched 13 November 2026

Prefer these foundations over making recent agent papers carry every claim.
This is a curated starting set, not a systematic review or a publication quota.
Published venue/year are distinguished from author-manuscript upload dates;
several accessible manuscripts retain placeholder issue/date metadata. Use the
confirmed journal/conference record in the eventual bibliography.

### Runtime verification and the limits of knowledge

**RV1 — Leucker and Schallhart (2009), A Brief Account of Runtime Verification.**
Journal of Logic and Algebraic Programming 78(5), 293–303, peer-reviewed journal.
[Author manuscript](https://christian.schallhart.net/publications/2009--jlap--a-brief-account-of-runtime-verification.pdf);
[author institution record](https://www.isp.uni-luebeck.de/research/publications/brief-account-runtime-verification).
Use for the definition of runtime verification and its complementary relation
to model checking and testing. Start here for terminology and history. It does
not establish low-latency performance of Wyrd or universal correctness.
Access: manuscript abstract/introduction and definition passages inspected.

**RV2 — Bauer, Leucker and Schallhart (2011), Runtime Verification for LTL and TLTL.**
ACM Transactions on Software Engineering and Methodology 20(4), article 14,
peer-reviewed journal. [Publication and DOI](https://research.uni-luebeck.de/de/publications/runtime-verification-for-ltl-and-tltl/);
[author manuscript](https://christian.schallhart.net/publications/2011--tosem--runtime-verification-for-ltl-and-tltl.pdf).
Use for formally specified properties, finite observations, monitorability,
and true/false/inconclusive semantics. Inconclusive outcomes have foundations
well before LLM agents. Do not equate its temporal-logic semantics with every
application's missing-evidence policy or an LLM judge's uncertainty.
Access: abstract/introduction inspected; venue/DOI corroborated by institution.

**RV3 — Agha and Palmskog (2018), A Survey of Statistical Model Checking.**
ACM Transactions on Modeling and Computer Simulation 28(1), article 6,
peer-reviewed journal. [Author laboratory record, abstract and DOI](https://osl.cs.illinois.edu/publications/journals/tomacs/AghaP18.html).
Use for precise stochastic properties, sampled executions and confidence/error
bounds; formal reasoning and probabilistic systems are compatible. Sampling
assumptions and statistical error matter. This does not mean an arbitrary
model-generated confidence score is a statistical guarantee.
Access: author abstract and publication metadata inspected; full methods need
claim-specific reading before publication.

**OR1 — Barr, Harman, McMinn, Shahbaz and Yoo (2015), The Oracle Problem in Software Testing: A Survey.**
IEEE Transactions on Software Engineering 41(5), 507–525, peer-reviewed journal.
[Author manuscript](https://philmcminn.com/publications/barr2015.pdf);
[institution publication record](https://discovery.ucl.ac.uk/id/eprint/1471263/).
This is the closest foundational match to the maintainer's question: deciding
correctness when expected behavior is hard to specify or checking is costly.
Teach partial oracles, specifications, domain knowledge and human judgment.
The oracle problem is not a new consequence of agentic AI and is not solved by
asking another model to agree. Access: abstract/introduction inspected.

**OR2 — Segura, Fraser, Sánchez and Ruiz-Cortés (2016), A Survey on Metamorphic Testing.**
IEEE Transactions on Software Engineering 42(9), 805–824, peer-reviewed journal.
[Author laboratory publication record](https://www.isa.us.es/publications/type/article-journal/2016/survey-metamorphic-testing);
[institution-hosted author manuscript](https://idus.us.es/server/api/core/bitstreams/d8fffc33-4eb5-4bb4-a3d6-c4ddab3f7cb6/content).
Use for checking necessary relationships across inputs/outputs when an exact
reference output is unavailable. Teach a concrete example such as invariance
under an irrelevant transformation, with domain justification. Passing a
metamorphic relation is not proof that either individual answer is correct.
Access: primary indexed manuscript abstract/introduction and author metadata
inspected; direct alternate PDF retrieval timed out.

### ML: outputs and scores can mislead

**ML1 — Kapoor and Narayanan (2023), Leakage and the reproducibility crisis in machine-learning-based science.**
Patterns 4(9), 100804, peer-reviewed journal.
[Published full text and DOI](https://pmc.ncbi.nlm.nih.gov/articles/PMC10499856/).
Use for the leakage taxonomy and the failure of apparently strong evaluations.
Separate evaluation-validity checks from per-request output checks: provenance,
training/test separation, preprocessing and time boundaries require their own
evidence. A fluent answer alone cannot establish these facts.
Access: published abstract, taxonomy opening and publication/acceptance metadata.

**ML2 — Gama, Žliobaitė, Bifet, Pechenizkiy and Bouchachia (2014), A Survey on Concept Drift Adaptation.**
ACM Computing Surveys 46(4), article 44, peer-reviewed journal.
[Publication record and DOI](https://research.aalto.fi/en/publications/a-survey-on-concept-drift-adaptation/);
[author accepted manuscript](https://mpechen.win.tue.nl/publications/pubs/Gama_ACMCS_AdaptationCD_accepted.pdf).
Use for changing distributions, real versus virtual drift, delayed feedback
and monitoring/adaptation. Input drift is not equivalent to changed predictive
relationships or proof that a particular prediction is wrong. The accepted
manuscript has placeholder 2013 citation metadata; cite the final 2014 record.
Access: manuscript introduction, definitions and drift distinctions inspected.

**ML3 — Ovadia et al. (2019), Can You Trust Your Model's Uncertainty? Evaluating Predictive Uncertainty Under Dataset Shift.**
NeurIPS 2019, peer-reviewed conference.
[Published paper](https://proceedings.neurips.cc/paper_files/paper/2019/file/8558cb408c1d76621371888657d2eb1d-Paper.pdf).
Use for uncertainty/calibration under distribution shift and why confidence
measured on one distribution may not transfer. Findings concern the evaluated
classification settings/methods, not a universal theorem about every AI judge.
Access: abstract/introduction and uncertainty distinctions inspected.

### Modern evaluation and practical requirements

**EV1 — Zheng et al. (2023), Judging LLM-as-a-Judge with MT-Bench and Chatbot Arena.**
NeurIPS 2023 Datasets and Benchmarks, peer-reviewed conference track.
[Published paper](https://proceedings.neurips.cc/paper_files/paper/2023/file/91f18a1287b398d378ef22505bf41832-Paper-Datasets_and_Benchmarks.pdf).
Use for both the usefulness and limitations of model judges: human-preference
agreement in the studied setting alongside position, verbosity, self-enhancement
and reasoning limitations. Human preference is not interchangeable with factual
correctness, process compliance or calibrated reliability on a new domain.
Access: abstract/introduction and an illustrative incorrect judgment inspected.

**EV2 — Liu et al. (2024), AgentBench: Evaluating LLMs as Agents.**
ICLR 2024, peer-reviewed conference.
[Official proceedings](https://proceedings.iclr.cc/paper_files/paper/2024/hash/e9df36b21ff4ee211a8b71ee8b7e9f57-Abstract-Conference.html).
Use to distinguish interactive, multi-step agent evaluation from a single final
response score; benchmarks expose reasoning/decision/instruction-following
failures in their environments. A benchmark evaluation is not by itself a live
production verification system. Access: official abstract and venue metadata;
full proceedings PDF exceeds the browser extraction size limit.

**PR1 — Breck, Cai, Nielsen, Salib and Sculley (2017), The ML Test Score: A Rubric for ML Production Readiness and Technical Debt Reduction.**
IEEE Big Data 2017, peer-reviewed conference.
[Author publication record](https://research.google/pubs/the-ml-test-score-a-rubric-for-ml-production-readiness-and-technical-debt-reduction/);
[paper](https://storage.googleapis.com/gweb-research2023-media/pubtools/4156.pdf).
Use for actionable data/model/infrastructure/monitoring requirements and the
difficulty of specifying production ML behavior. It is experience-based guidance,
not a universal standard or proof a passing rubric guarantees reliability.
Access: abstract/introduction and feature/data-test examples inspected.

**RE1 — Zave and Jackson (1997), Four Dark Corners of Requirements Engineering.**
ACM Transactions on Software Engineering and Methodology 6(1), 1–30,
peer-reviewed journal. [Institution-hosted original paper](https://cse.msu.edu/~chengb/RE-491/Papers/dark-corners-re-zave-jackson.pdf).
Use as a requirements companion: separate environmental assumptions, desired
requirements and machine specifications. Access is currently limited to indexed
original-paper excerpts; direct PDF retrieval failed. Read the full relevant
sections before relying on detailed claims. This is not evidence that LLMs
automatically discover adequate requirements.

### Authoritative engineering guidance (not a research paper)

**DEF1 — NASA Systems Engineering Handbook, section 5, Product Realization.**
[Official handbook](https://www.nasa.gov/reference/5-0-product-realization/).
Use its product verification/validation definitions and stakeholder/intended-use
distinction, labeled as engineering guidance rather than peer-reviewed research.
Explain that disciplines use terms differently. Avoid conflating ML's validation
dataset with the entire systems-engineering process of validation.

## Locked real-time-first educational sequence

The maintainer clarified that runtime/real-time verification must organize the
teaching rather than appear as an afterthought. Keep the foundation independent
of Wyrd while making decision timing prominent; use the canonical real-time
page as the explicit product bridge.

1. A plausible outcome or impressive score can conceal a failure (ML1/PR1).
2. Identify the requirement, evidence and consequential decision. Introduce
   checking actual running executions and the opportunity to judge before an
   action, during execution or afterward (RV1/RV2, DEF1).
3. Explain what can be established with finite observations and imperfect
   oracles; cover formal/runtime overlap and scoped inconclusive outcomes
   without opposing probability to formal methods (RV2/RV3/OR1).
4. Work without an exact reference answer using suitable invariants,
   constraints and metamorphic relationships (OR1/OR2); establish their limits.
5. Account for leakage, changing environments, uncertainty and delayed evidence
   (ML1/ML2/ML3), then contextual model/agent judgments and judge failure
   (EV1/EV2 and the supplied VerifiAgent paper).
6. Build a practical workflow: requirements, evidence, validated checks,
   counterexamples, response policy, provenance and revision (PR1 plus author
   synthesis). Treat adequate requirements and reliable low-overhead
   infrastructure as connected challenges, not interchangeable solutions.
7. Link to the real-time category page: Wyrd's engine is built for verdicts
   within the current application decision and also supports queued/background
   work. Show the declared/versioned contract, request/verify/verdict/decision/
   response flow and why evidence publication is separate. Explain engine,
   evidence and check/provider costs before queued/scheduled alternatives.

The product bridge does not turn the foundation into an installation guide.
Real-time is not a hard scheduling guarantee; model/evidence work and supported
admission/cache-miss waits remain part of an application's budget. Mechanics
must cite repository authority; numeric claims require applicable measurements.
No new backend work or performance experiment is authorized. This is a teaching
sequence, not a new taxonomy or claim that all model-based checks offer formal
monitor guarantees.

## Claims requiring restraint or further research

- Runtime verification, uncertain-system reasoning and requirements engineering
  existed before modern LLM agents. Agentic AI's contribution should be framed
  as an opportunity to reduce authoring/implementation/judgment costs, not a
  historical claim that it first made verification possible.
- The selected sources do not establish a general causal result that coding
  agents build faster and more reliable verification infrastructure or discover
  complete system requirements. Treat this as a thesis unless supported by
  narrower empirical work with relevant conditions and measures.
- No source here proves Wyrd's speed, reliability, superiority or formal
  verification guarantees. Product-specific claims require separate evidence.
- Independent requirements and evidence matter; judge agreement, confidence,
  repeatability, stakeholder preference and correctness are distinct measures.
- Peer review is a selection criterion, not a guarantee a result generalizes.
  Cite limits, venue/year and evidence type, not citation counts as authority.

## Publication proof

Task 2 should keep a short claim-to-source ledger: the proposed claim, source
and section, evidence type, applicability/limits and concrete teaching example.
Read methods/limitations for every empirical claim actually used, not only an
abstract; link to the version of record and an accessible author copy where
possible. Do not require every shortlisted paper on the introductory page.
Keep further reading available for depth without overwhelming a newcomer.

---
title: Foundations of verification
description: What AI verification is, how it differs from validation, evaluation, testing, observability, and formal proof, and how to verify uncertain systems from observed evidence.
pillar: wyrd
section: Understand
order: 1
---

Verification asks whether a system's behavior meets requirements stated before the behavior is judged, and what evidence supports the answer. For AI applications it matters because a fluent answer or a strong benchmark score can look right while a requirement nobody checked is broken.

This page explains verification as an engineering discipline. It needs no Wyrd account or vocabulary; product mechanics follow in [Real-time verification](/understand/real-time-verification/).

## A plausible result is not evidence

An assistant prepares a customer report for an outside auditor and replies:

```text
I confirmed the auditor is authorized and the report contains no account numbers. Ready to send.
```

The reply is fluent, specific, and possibly false. Three requirements decide whether the report may leave:

| Requirement | Evidence that can establish it | What the reply alone establishes |
| --- | --- | --- |
| The recipient is authorized to receive the report | The authorization record the application looked up | Nothing: the assistant says it checked |
| The report contains no bank account identifier | The report text itself | Nothing until someone reads the report |
| The company sharing policy permits this purpose | The policy text, the stated purpose, and a judgment that cites both | A claim, with no citation |

Judging the reply cannot settle any row. Judging the evidence in the middle column can. That shift, from judging what a system says to judging what it did and produced, is the core of verification.

## Requirement, evidence, judgment, decision

Every verification has four parts:

1. A **requirement** states what must be true, scoped to a context: "a report shared with an external auditor names no bank account identifier".
2. **Evidence** is what was observed: inputs, outputs, records the system read, actions it took, and the exact versions involved.
3. A **judgment** applies a check to the evidence and returns passed, failed, or inconclusive, with a reason.
4. A **decision** is what the application or team does with the judgment: send, hold, escalate, retrain, or revise the requirement.

The judgment is only as strong as the requirement and the evidence. A precise check over the wrong evidence answers a different question; a vague requirement cannot be checked at all.

## When the judgment happens

Runtime verification checks whether an observed execution of a system satisfies or violates a property, rather than proving the property for all executions ([Leucker and Schallhart, 2009](#sources)). The same check can run at three points, and the point determines which decisions it can inform:

| When | What it can change | Example |
| --- | --- | --- |
| Before an action is committed | Whether the action happens | Hold the report before it is sent |
| During execution | The next step of a running workflow | Stop an agent before it calls a payment tool twice |
| Afterward | Investigation, alerts, retraining, and the next release | Find every report sent last week that cited a superseded policy |

Checking during execution (online) and checking a recorded trace later (offline) are both runtime verification ([Leucker and Schallhart, 2009](#sources)). A check that runs online is not automatically fast enough for a given application: whether a judgment arrives in time depends on the check, its evidence, and the application's deadline.

## Related disciplines

Fields use these words differently, so this page states the definitions it uses. NASA's systems engineering guidance separates **verification**, which shows a product meets its specified requirements, from **validation**, which shows it fulfills its intended use in its intended environment ([NASA, 2016](#sources)). In machine learning, "validation set" means something narrower: held-out data for tuning a model.

| Discipline | Question | Typical evidence | Limit |
| --- | --- | --- | --- |
| Verification | Does this behavior meet the stated requirements? | Observed executions, outputs, records, versions | Only as good as the requirements |
| Validation | Do the requirements serve the real need? | Stakeholder review, field use | Can show the wrong thing was built, not that a run was correct |
| Testing | Does the system behave as expected on chosen cases? | Test inputs and an oracle that judges them | Covers the cases chosen |
| Statistical or model evaluation | How well does a model perform on a distribution? | Labeled datasets, metrics | An aggregate score says little about one request, and less after the distribution shifts |
| Agent evaluation | How well does an agent complete multi-step tasks? | Benchmark environments, trajectories ([Liu et al., 2024](#sources)) | A benchmark result is not a check on a live run |
| Observability | What happened inside the system? | Logs, metrics, traces | Records behavior without judging it against a requirement |
| Guardrails | Should this input or output be blocked? | The current input or output | Usually judges one message, not the process behind it |
| Formal verification | Does a model of the system satisfy a property in every execution? | A model, a specification, a proof or exhaustive search | Holds under the model's assumptions |
| Assurance | Is there a justified case that the system is acceptable for its use? | Arguments assembled from all of the above | As strong as the evidence it assembles |

These overlap. A test suite contains verifications; an evaluation can be run as a recurring check; a guardrail is a runtime check on a narrow kind of evidence.

## Proofs and observed executions

Formal verification and runtime verification are often drawn as opposites: certainty for deterministic software, approximation for probabilistic AI. Neither half of that picture is accurate.

- Formal methods reason about probabilistic systems. Statistical model checking samples executions of a stochastic system and bounds the probability that a property holds, with stated confidence and error ([Agha and Palmskog, 2018](#sources)).
- Runtime monitors check formally specified properties. Temporal-logic monitors judge a finite trace as satisfying, violating, or inconclusive when the trace so far cannot settle the property ([Bauer, Leucker and Schallhart, 2011](#sources)).
- Agent decision-making has been formally verified for years before language-model agents: [Dennis et al. (2016)](#sources) model-check the decision logic of autonomous agent programs. Recent proposals learn a probabilistic model of an LLM agent from observed runs and model-check it at runtime ([Koohestani, 2025](#sources), a proof-of-concept preprint).

The real distinction is what a result rests on. A proof holds for every execution of a model, under the model's assumptions; if the deployed system departs from the model, the proof says nothing about the departure. A runtime judgment rests on finite observed evidence; it says what this execution did, not what every execution will do. Neither establishes that a system is correct in general.

## Verifying without a single right answer

Many AI tasks have no reference output: two good summaries of a report can share no sentence. Deciding whether behavior is correct when the expected output is unknown or costly to compute is the **oracle problem** of software testing ([Barr et al., 2015](#sources)). It predates AI agents, and its known partial answers apply to them:

- **Scope the requirement.** "The summary is good" has no oracle. "The summary states the quarterly total that appears in the report" does.
- **Check invariants and prohibited actions.** No summary may contain an account number; no workflow may send before authorization is recorded.
- **Accept a range.** A forecast within an agreed tolerance of the actual, or a refund at or below the policy limit, passes.
- **Check relations between runs.** A metamorphic relation states how outputs must relate when inputs change in a known way ([Segura et al., 2016](#sources)). Rephrasing a customer's question without changing its facts must not change their refund eligibility. A passing relation shows consistency, not that either answer is right.

Uncertainty enters from three places, and each needs a different response:

| Source | Example | Response |
| --- | --- | --- |
| The system | A model gives different answers to the same request | Judge each answer; track the rate over time |
| The evidence | The authorization record was not captured | Return inconclusive instead of guessing |
| The judge | A model judge is unsure whether the policy covers this purpose | Return inconclusive, or route to a person |

An inconclusive judgment is a result, not a failure to produce one. Temporal-logic monitoring has formalized it for years ([Bauer, Leucker and Schallhart, 2011](#sources)), and an application has to decide what it does with one.

## Failures that look like success

Three failures produce attractive results, and each is revealed by different evidence.

**Leakage.** A fraud model scores 0.98 AUC on its test set because transactions from the same customers appear in both training and test data, or because a feature was computed after the fraud label was known. Leakage of this kind has undermined published results across many fields of machine-learning-based science ([Kapoor and Narayanan, 2023](#sources)). No output check catches it: the predictions look excellent. Detecting it needs evidence about the evaluation itself: how the split was made, where each feature came from, and when each value became available.

**Drift.** Six months later the mix of merchant categories has changed. That is a change in the input distribution, which may or may not change the relationship between inputs and fraud; the literature separates the two as virtual and real concept drift ([Gama et al., 2014](#sources)). A drift alarm is a signal to investigate, not proof that any prediction is wrong. The model's own confidence is a weak guide here: uncertainty estimates that were well calibrated on the original distribution can degrade under shift ([Ovadia et al., 2019](#sources)).

**A skipped step.** The assistant from the first example says it confirmed authorization. The evidence that settles the claim is a record of the lookup: a trace span or tool-call log showing the authorization service was called for this recipient. An output check cannot settle it.

The absence of that record is not, on its own, proof the step was skipped. Traces can be sampled away, delayed, or lost. Treat missing evidence as inconclusive unless the capture path guarantees that every executed step is recorded.

## Model judges

Some requirements need judgment: whether a purpose is "an external audit" under a policy written in prose. A language model can apply that judgment. In one large study, strong model judges agreed with human preferences about as often as humans agreed with each other, and also showed position bias, verbosity bias, a preference for their own answers, and weak grading of math and reasoning ([Zheng et al., 2023](#sources)). Combining a model's consistency checks with external tools, such as code execution, improved verification on reasoning benchmarks in [Han et al. (2025)](#sources). Neither result shows that a judge is reliable on a new domain or policy.

Treat a model judge as a check that needs its own verification:

- Build labeled cases: releases the policy clearly permits, clearly forbids, and leaves ambiguous. Run the judge on them before trusting it, and again after any change to its prompt, model, or policy.
- Require citations to the evidence it relied on, so a person can audit a judgment.
- Agreement between two models, or one model with itself, is not independent evidence. Models can share errors, and an economic argument holds that verification capacity, not generation, becomes the constraint as automated output grows ([Catalini, Hui and Wu, 2026](#sources), a preprint).
- Report a judge as calibrated only after measuring it against labeled outcomes in the domain where it runs.

## Two hard problems

Practical verification depends on two things, and solving one does not solve the other.

**Infrastructure.** Evidence has to be captured as the system runs, checks have to execute inside the application's time budget, every judgment has to name the exact versions it judged, and the evidence has to be retained for investigation. Capture that slows the application gets turned off; judgments without versions cannot be reproduced.

**Requirements.** Requirements describe the world the system acts in, not only the system. Requirements engineering separates assumptions about the environment, the requirements themselves, and the specification the machine implements ([Zave and Jackson, 1997](#sources)). Production ML adds data, model, and monitoring requirements that are hard to state completely ([Breck et al., 2017](#sources)).

Coding agents and model judges can help with both: drafting capture and check code, proposing requirements from a policy document, generating counterexamples, and judging contextual requirements. They do not remove the need for a domain owner who decides what the requirements are, for independent checks, and for labeled cases. Runtime verification is decades older than language-model agents ([Leucker and Schallhart, 2009](#sources)); whether agents make it substantially cheaper or more reliable is an open question, not an established result.

## A starting method

1. **Pick one consequential decision and its failures.** "Send a customer report to an external party" fails by leaking an identifier, reaching an unauthorized recipient, or violating policy.
2. **Write each failure as a scoped requirement.** One statement per failure, naming the context: "a report shared externally names no bank account identifier".
3. **Name the evidence each requirement needs.** The report text; the authorization record; the policy version and stated purpose. If the evidence is not captured today, capturing it is the first task.
4. **Choose the cheapest check that decides it, and test the check.** A field comparison for authorization; a pattern or code check for identifiers; a model judge with citations for the policy. Run each check on known passing, failing, and ambiguous cases before relying on it.
5. **Decide the response before you need it.** Passed sends. Failed holds. Inconclusive holds or goes to a person. An error in the check is not a pass.
6. **Keep the evidence and revise.** Retain each judgment with the evidence and versions it used. When the policy, model, or data changes, rerun the labeled cases, publish a new requirement version, and keep old judgments attributed to the versions they used.

## Sources

| Source | Type | Supports |
| --- | --- | --- |
| Leucker, M. and Schallhart, C. (2009). [A brief account of runtime verification](https://christian.schallhart.net/publications/2009--jlap--a-brief-account-of-runtime-verification.pdf). *Journal of Logic and Algebraic Programming* 78(5), 293–303. | Peer-reviewed journal | Definition of runtime verification; online and offline monitoring; relation to model checking and testing |
| Bauer, A., Leucker, M. and Schallhart, C. (2011). [Runtime verification for LTL and TLTL](https://christian.schallhart.net/publications/2011--tosem--runtime-verification-for-ltl-and-tltl.pdf). *ACM Transactions on Software Engineering and Methodology* 20(4), 14. | Peer-reviewed journal | Three-valued monitoring of finite traces: true, false, inconclusive |
| Agha, G. and Palmskog, K. (2018). [A survey of statistical model checking](https://osl.cs.illinois.edu/publications/journals/tomacs/AghaP18.html). *ACM Transactions on Modeling and Computer Simulation* 28(1), 6. | Peer-reviewed survey | Formal, sampled verification of stochastic systems with statistical bounds |
| Dennis, L., Fisher, M., Lincoln, N., Lisitsa, A. and Veres, S. (2016). [Practical verification of decision-making in agent-based autonomous systems](https://link.springer.com/article/10.1007/s10515-014-0168-9). *Automated Software Engineering* 23, 305–359. | Peer-reviewed journal | Formal verification of agent decision-making before LLM agents |
| Koohestani, R. (2025). [AgentGuard: Runtime verification of AI agents](https://arxiv.org/abs/2509.23864). arXiv:2509.23864. | Preprint, proof of concept | Probabilistic model checking of a learned model of agent behavior at runtime |
| Barr, E., Harman, M., McMinn, P., Shahbaz, M. and Yoo, S. (2015). [The oracle problem in software testing: a survey](https://philmcminn.com/publications/barr2015.pdf). *IEEE Transactions on Software Engineering* 41(5), 507–525. | Peer-reviewed survey | Judging correctness when expected behavior is unknown or costly |
| Segura, S., Fraser, G., Sánchez, A. and Ruiz-Cortés, A. (2016). [A survey on metamorphic testing](https://www.isa.us.es/publications/type/article-journal/2016/survey-metamorphic-testing). *IEEE Transactions on Software Engineering* 42(9), 805–824. | Peer-reviewed survey | Checking necessary relations between runs without a reference output |
| Kapoor, S. and Narayanan, A. (2023). [Leakage and the reproducibility crisis in machine-learning-based science](https://pmc.ncbi.nlm.nih.gov/articles/PMC10499856/). *Patterns* 4(9), 100804. | Peer-reviewed journal | Leakage taxonomy; strong scores undermined by leakage |
| Gama, J., Žliobaitė, I., Bifet, A., Pechenizkiy, M. and Bouchachia, A. (2014). [A survey on concept drift adaptation](https://research.aalto.fi/en/publications/a-survey-on-concept-drift-adaptation/). *ACM Computing Surveys* 46(4), 44. | Peer-reviewed survey | Real versus virtual drift; monitoring and delayed feedback |
| Ovadia, Y. et al. (2019). [Can you trust your model's uncertainty? Evaluating predictive uncertainty under dataset shift](https://proceedings.neurips.cc/paper_files/paper/2019/file/8558cb408c1d76621371888657d2eb1d-Paper.pdf). NeurIPS 2019. | Peer-reviewed conference, empirical | Calibration measured on one distribution degrading under shift, for the methods and tasks studied |
| Zheng, L. et al. (2023). [Judging LLM-as-a-judge with MT-Bench and Chatbot Arena](https://proceedings.neurips.cc/paper_files/paper/2023/file/91f18a1287b398d378ef22505bf41832-Paper-Datasets_and_Benchmarks.pdf). NeurIPS 2023 Datasets and Benchmarks. | Peer-reviewed conference, empirical | Model-judge agreement with human preference in the studied setting; position, verbosity, and self-enhancement biases |
| Han, Z. et al. (2025). [VerifiAgent: a unified verification agent in language model reasoning](https://aclanthology.org/2025.findings-emnlp.891/). Findings of EMNLP 2025. | Peer-reviewed conference, empirical | Consistency checks combined with tool-based verification on reasoning benchmarks |
| Liu, X. et al. (2024). [AgentBench: Evaluating LLMs as agents](https://proceedings.iclr.cc/paper_files/paper/2024/hash/e9df36b21ff4ee211a8b71ee8b7e9f57-Abstract-Conference.html). ICLR 2024. | Peer-reviewed conference, benchmark | Multi-step agent evaluation in interactive environments |
| Breck, E., Cai, S., Nielsen, E., Salib, M. and Sculley, D. (2017). [The ML test score: a rubric for ML production readiness and technical debt reduction](https://research.google/pubs/the-ml-test-score-a-rubric-for-ml-production-readiness-and-technical-debt-reduction/). IEEE Big Data 2017. | Peer-reviewed conference, experience-based guidance | Data, model, infrastructure, and monitoring requirements for production ML |
| Zave, P. and Jackson, M. (1997). [Four dark corners of requirements engineering](https://cse.msu.edu/~chengb/RE-491/Papers/dark-corners-re-zave-jackson.pdf). *ACM Transactions on Software Engineering and Methodology* 6(1), 1–30. | Peer-reviewed journal | Environment assumptions, requirements, and specifications as separate things |
| NASA (2016). [NASA Systems Engineering Handbook, section 5.0: Product realization](https://www.nasa.gov/reference/5-0-product-realization/). NASA/SP-2016-6105 Rev 2. | Engineering guidance, not research | Definitions of product verification and validation |
| Catalini, C., Hui, X. and Wu, J. (2026). [Some simple economics of AGI](https://arxiv.org/abs/2602.20946). arXiv:2602.20946 v2. | Preprint, economic argument | Verification capacity as a constraint; correlated checker errors |

Shin's [Automating epistemology](https://link.springer.com/article/10.1007/s00146-025-02560-y) (*AI & Society*, 2025) discusses, as philosophy rather than engineering evidence, how AI changes who holds the authority to declare something verified.

## Next

[Write verifiable requirements](/build-and-verify/declare/write-verifiable-requirements/), [Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/), and [Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) turn the starting method into practice. [Real-time verification](/understand/real-time-verification/) shows how Wyrd returns a judgment before the application acts.

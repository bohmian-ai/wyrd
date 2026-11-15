---
title: Foundations of verification
description: What AI verification is, how it differs from validation, evaluation, testing, observability, and formal proof, and how to verify uncertain systems from observed evidence.
pillar: wyrd
section: Understand
order: 1
---

Verification asks whether a system's behavior meets requirements stated before the behavior is judged, and what evidence supports the answer. For AI applications it matters because a fluent answer or a strong benchmark score can look right while a requirement nobody checked is broken.

It needs no Wyrd account or vocabulary; product mechanics follow in [Real-time verification](/understand/real-time-verification/).

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

Acting on a judgment, by holding or stopping, goes beyond detection; the literature calls it runtime enforcement or reflection. Checking during execution (online) and checking a recorded trace later (offline) are both runtime verification ([Leucker and Schallhart, 2009](#sources)). A check that runs online is not automatically fast enough for a given application: whether a judgment arrives in time depends on the check, its evidence, and the application's deadline.

## Related disciplines

Fields use these words differently, so this page states the definitions it uses. NASA's systems engineering guidance separates **verification**, which shows a product meets its specified requirements, from **validation**, which shows it fulfills its intended use in its intended environment ([NASA, 2024](#sources)). In machine learning, "validation set" means something narrower: held-out data for tuning a model.

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

Formal verification and runtime verification are often drawn as opposites: certainty for deterministic software, approximation for probabilistic AI. The overlap is larger than that picture suggests.

- Formal methods reason about probabilistic systems. Statistical model checking samples executions of a stochastic system and decides whether a probabilistic property holds, with quantified statistical confidence ([Agha and Palmskog, 2018](#sources)).
- Runtime monitors check formally specified properties. Temporal-logic monitors judge a finite trace as satisfying, violating, or inconclusive when the trace so far cannot settle the property ([Bauer, Leucker and Schallhart, 2011](#sources)).
- Formal verification of agent decision-making predates language-model agents: [Dennis et al. (2016)](#sources) model-check the decision logic of agent programs that control hybrid autonomous systems. Recent proposals learn a probabilistic model of an LLM agent from observed runs and model-check it at runtime ([Koohestani, 2025](#sources), a proof-of-concept workshop paper).

The real distinction is what a result rests on. A proof holds for every execution of what was verified, a model or the code itself, under stated assumptions about its environment, inputs, compiler, and hardware; where those assumptions fail, the proof says nothing. Statistical model checking gives quantified confidence, not a proof. A runtime judgment rests on finite observed evidence; it says what this execution did, not what every execution will do. Neither establishes that a system is correct in general.

## Verifying without a single right answer

Many AI tasks have no reference output: two good summaries of a report can share no sentence. Distinguishing correct from incorrect behavior for a given input is the **test oracle problem** of software testing; when no specification, derived oracle, or relation between runs is adequate, it falls back to costly human judgment ([Barr et al., 2015](#sources)). It predates AI agents, and its known partial answers apply to them:

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

An inconclusive judgment is a result, not a failure to produce one. Its reason should say which source caused it, so an operator can tell a capture gap from an unsure judge. Temporal-logic monitors formalize one case: a trace that so far cannot settle the property ([Bauer, Leucker and Schallhart, 2011](#sources)). Missing evidence and an unsure judge are further cases, and an application has to decide what it does with each.

## Failures that look like success

Three failures produce attractive results, and each is revealed by different evidence.

**Leakage.** A fraud model scores 0.98 AUC on its test set because transactions from the same customers appear in both training and test data, or because a feature was computed after the fraud label was known. A survey of machine-learning-based science compiled reports of leakage in 17 fields, affecting 294 papers ([Kapoor and Narayanan, 2023](#sources)). No output check catches it: the predictions look excellent. Detecting it needs evidence about the evaluation itself: how the split was made, where each feature came from, and when each value became available.

**Drift.** Six months later the mix of merchant categories has changed. That is a change in the input distribution, which may or may not change the relationship between inputs and fraud; the literature separates the two as virtual and real concept drift ([Gama et al., 2014](#sources)). Detecting a shift in inputs needs a baseline window to compare against; telling whether the input-outcome relationship changed needs outcomes, which often arrive late ([Gama et al., 2014](#sources)). A drift alarm is a signal to investigate, not proof that any prediction is wrong. The model's own confidence is a weak guide here: uncertainty estimates that were well calibrated on the original distribution can degrade under shift ([Ovadia et al., 2019](#sources)).

**A skipped step.** The assistant from the first example says it confirmed authorization. The evidence that settles the claim is a record of the lookup: a trace span or tool-call log showing the authorization service was called for this recipient. An output check cannot settle it.

The absence of that record is not, on its own, proof the step was skipped. Traces can be sampled away, delayed, or lost. Treat missing evidence as inconclusive unless the capture path guarantees that every executed step is recorded.

## Model judges

Some requirements need judgment: whether a purpose is "an external audit" under a policy written in prose. A language model can apply that judgment. In one large study, strong model judges agreed with human preferences about as often as humans agreed with each other, and also showed position bias, verbosity bias, and weak grading of math and reasoning; a preference for their own answers appeared for some judges, but the study could not confirm it ([Zheng et al., 2023](#sources)). Combining consistency checks with tool-based verification outperformed baseline verifiers on reasoning tasks in [Han, Buntine and Shareghi (2025)](#sources). Neither result shows that a judge is reliable on a new domain or policy.

A model judge is a check that needs its own verification:

- Labeled cases show how it behaves: releases the policy clearly permits, clearly forbids, and leaves ambiguous, run before the judge is trusted and again after any change to its prompt, model, or policy.
- Citations to the evidence it relied on let a person audit a judgment.
- Agreement between two models, or one model with itself, is not independent evidence: a shared blind spot produces agreement too. An economic argument holds that human verification bandwidth becomes the binding constraint as automated output grows, and that using AI to verify AI propagates those correlated blind spots ([Catalini, Hui and Wu, 2026](#sources), a preprint).
- A judge's accuracy, or its agreement with domain experts, is known only after it is measured against labeled outcomes in the domain where it runs. Calibration is a further claim: that a judge's stated confidence matches how often it is right ([Ovadia et al., 2019](#sources) measure it for classifiers), which needs a confidence to measure.

## Two hard problems

Practical verification depends on two things, and solving one does not solve the other.

**Infrastructure.** Evidence has to be captured as the system runs, checks have to execute inside the application's time budget, every judgment has to name the exact versions it judged, and the evidence has to be retained for investigation. Capture has to cost the application little enough to stay on.

**Requirements.** Requirements describe the world the system acts in, not only the system. Requirements engineering separates knowledge of the environment, the requirements themselves, and the specification the machine implements ([Zave and Jackson, 1997](#sources)). Production ML adds tests for data, models, infrastructure, and monitoring, because a model's prediction behavior is difficult to specify in advance ([Breck et al., 2017](#sources)).

Coding agents and model judges can help with both: drafting capture and check code, proposing requirements from a policy document, generating counterexamples, and judging contextual requirements. They do not remove the need for a domain owner who decides what the requirements are, for independent checks, and for labeled cases. Runtime verification was an established field more than a decade before language-model agents ([Leucker and Schallhart, 2009](#sources)); whether agents make it substantially cheaper or more reliable is an open question, not an established result.

## A starting method

A team can start with one decision and grow from there:

1. **One consequential decision and its failures.** "Send a customer report to an external party" fails by leaking an identifier, reaching an unauthorized recipient, or violating policy.
2. **One scoped requirement per failure,** naming its context: "a report shared externally names no bank account identifier".
3. **The evidence each requirement needs:** the report text, the authorization record, the policy version and stated purpose. Evidence that is not captured yet is the first gap to close.
4. **The cheapest check that decides each requirement,** tested on known passing, failing, and ambiguous cases: a field comparison for authorization, a pattern or code check for identifiers, a model judge with citations for the policy.
5. **A response chosen in advance.** Passed sends. Failed holds. Inconclusive holds or goes to a person. An error in the check is not a pass.
6. **Retained evidence and revision.** Each judgment is kept with the evidence and versions it used. When the policy, model, or data changes, the labeled cases run again, the requirement gets a new version, and old judgments stay attributed to the versions they used.

The practice guides below turn each step into concrete work.

## Sources

| Source | Type | Supports |
| --- | --- | --- |
| Leucker, M. and Schallhart, C. (2009). [A brief account of runtime verification](https://christian.schallhart.net/publications/2009--jlap--a-brief-account-of-runtime-verification.pdf). *Journal of Logic and Algebraic Programming* 78(5), 293–303. | Peer-reviewed journal | Definition of runtime verification; online and offline monitoring; relation to model checking and testing |
| Bauer, A., Leucker, M. and Schallhart, C. (2011). [Runtime verification for LTL and TLTL](https://christian.schallhart.net/publications/2011--tosem--runtime-verification-for-ltl-and-tltl.pdf). *ACM Transactions on Software Engineering and Methodology* 20(4), 14. | Peer-reviewed journal | Three-valued monitoring of finite traces: true, false, inconclusive |
| Agha, G. and Palmskog, K. (2018). [A survey of statistical model checking](https://osl.cs.illinois.edu/publications/journals/tomacs/AghaP18.html). *ACM Transactions on Modeling and Computer Simulation* 28(1), 6. | Peer-reviewed survey | Formal, sampled verification of stochastic systems with statistical bounds |
| Dennis, L., Fisher, M., Lincoln, N., Lisitsa, A. and Veres, S. (2016). [Practical verification of decision-making in agent-based autonomous systems](https://link.springer.com/article/10.1007/s10515-014-0168-9). *Automated Software Engineering* 23(3), 305–359 (published online 2014). | Peer-reviewed journal | Formal verification of agent decision-making before LLM agents |
| Koohestani, R. (2025). [AgentGuard: Runtime verification of AI agents](https://arxiv.org/abs/2509.23864). arXiv:2509.23864 v1; AgenticSE workshop at ASE 2025. | Workshop paper, proof of concept | Probabilistic model checking of a learned model of agent behavior at runtime |
| Barr, E., Harman, M., McMinn, P., Shahbaz, M. and Yoo, S. (2015). [The oracle problem in software testing: a survey](https://philmcminn.com/publications/barr2015.pdf). *IEEE Transactions on Software Engineering* 41(5), 507–525. | Peer-reviewed survey | Judging correctness when expected behavior is unknown or costly |
| Segura, S., Fraser, G., Sánchez, A. and Ruiz-Cortés, A. (2016). [A survey on metamorphic testing](https://www.isa.us.es/publications/type/article-journal/2016/survey-metamorphic-testing). *IEEE Transactions on Software Engineering* 42(9), 805–824. | Peer-reviewed survey | Checking necessary relations between runs without a reference output |
| Kapoor, S. and Narayanan, A. (2023). [Leakage and the reproducibility crisis in machine-learning-based science](https://pmc.ncbi.nlm.nih.gov/articles/PMC10499856/). *Patterns* 4(9), 100804. | Peer-reviewed journal | Leakage taxonomy; compiled reports of leakage in 294 papers across 17 fields |
| Gama, J., Žliobaitė, I., Bifet, A., Pechenizkiy, M. and Bouchachia, A. (2014). [A survey on concept drift adaptation](https://research.aalto.fi/en/publications/a-survey-on-concept-drift-adaptation/). *ACM Computing Surveys* 46(4), 44. | Peer-reviewed survey | Real versus virtual drift; monitoring and delayed feedback |
| Ovadia, Y. et al. (2019). [Can you trust your model's uncertainty? Evaluating predictive uncertainty under dataset shift](https://proceedings.neurips.cc/paper_files/paper/2019/file/8558cb408c1d76621371888657d2eb1d-Paper.pdf). NeurIPS 2019. | Peer-reviewed conference, empirical | Calibration measured on one distribution degrading under shift, for the methods and tasks studied |
| Zheng, L. et al. (2023). [Judging LLM-as-a-judge with MT-Bench and Chatbot Arena](https://proceedings.neurips.cc/paper_files/paper/2023/file/91f18a1287b398d378ef22505bf41832-Paper-Datasets_and_Benchmarks.pdf). NeurIPS 2023 Datasets and Benchmarks. | Peer-reviewed conference, empirical | Model-judge agreement with human preference in the studied setting; position and verbosity biases; self-enhancement examined but not established |
| Han, J., Buntine, W. and Shareghi, E. (2025). [VerifiAgent: a unified verification agent in language model reasoning](https://aclanthology.org/2025.findings-emnlp.891/). *Findings of the Association for Computational Linguistics: EMNLP 2025*, 16410–16431. | Peer-reviewed conference, empirical | Consistency checks combined with tool-based verification on reasoning benchmarks |
| Liu, X. et al. (2024). [AgentBench: Evaluating LLMs as agents](https://proceedings.iclr.cc/paper_files/paper/2024/hash/e9df36b21ff4ee211a8b71ee8b7e9f57-Abstract-Conference.html). ICLR 2024. | Peer-reviewed conference, benchmark | Multi-step agent evaluation in interactive environments |
| Breck, E., Cai, S., Nielsen, E., Salib, M. and Sculley, D. (2017). [The ML test score: a rubric for ML production readiness and technical debt reduction](https://research.google/pubs/the-ml-test-score-a-rubric-for-ml-production-readiness-and-technical-debt-reduction/). IEEE Big Data 2017. | Peer-reviewed conference, experience-based guidance | Data, model, infrastructure, and monitoring requirements for production ML |
| Zave, P. and Jackson, M. (1997). [Four dark corners of requirements engineering](https://cse.msu.edu/~chengb/RE-491/Papers/dark-corners-re-zave-jackson.pdf). *ACM Transactions on Software Engineering and Methodology* 6(1), 1–30. | Peer-reviewed journal | Environment assumptions, requirements, and specifications as separate things |
| NASA (2024). [NASA Systems Engineering Handbook, 5.0 Product realization](https://www.nasa.gov/reference/5-0-product-realization/), sections 5.3 Product verification and 5.4 Product validation. Online edition, updated 12 August 2024. | Engineering guidance, not research | Definitions of product verification and validation |
| Catalini, C., Hui, X. and Wu, J. (2026). [Some simple economics of AGI](https://arxiv.org/abs/2602.20946). arXiv:2602.20946 v2. | Preprint, economic argument | Verification capacity as a constraint; correlated checker errors |


## Next

[Write verifiable requirements](/build-and-verify/declare/write-verifiable-requirements/), [Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/), and [Act on verdicts in production](/build-and-verify/verify/act-on-verdicts-in-production/) turn the starting method into practice. [Real-time verification](/understand/real-time-verification/) shows how Wyrd returns a judgment before the application acts, and the [Quickstart](/get-started/quickstart/) runs that example.

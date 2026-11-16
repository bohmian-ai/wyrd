---
title: Foundations of verification
description: What verification establishes, how testing, validation, formal proof, and runtime checking differ, how to check outputs with no single right answer, and what an ML score or an agent's claim cannot show.
pillar: wyrd
section: Understand
order: 1
---

## What verification is

Verification is checking, with evidence, that something does what it was required to do. Systems engineering defines it the same way: verification shows that a product meets its specified requirements ([NASA, 2024](#sources)). The opposite of verified is not "broken"; it is "unknown". A system that seems to work has not been verified until someone has compared what it did with what it was required to do.

Every verification has three parts:

- A **requirement**: what must be true, stated before anything is judged. "The output is sorted" is a requirement.
- **Evidence**: a record of what actually happened, such as the input and output of one call.
- A **check**: a procedure that compares the evidence with the requirement and reports that the requirement is met, that it is violated, or that the evidence cannot decide.

The requirement comes first for a reason. A requirement written after seeing a result tends to describe that result. Stating it beforehand turns "this looks fine" into a claim the evidence can contradict.

## Why verification is important

Verification is how a team knows a system works instead of assuming it does. Without it, confidence rests on how results look, and a broken requirement is found by whoever is harmed by it: a customer who received the wrong refund, an auditor who received a report they were not entitled to see.

It matters for four reasons:

- **Failures are cheaper to catch before they act.** A check that runs before a result is used can stop it. The same failure found later can only be explained and cleaned up.
- **It makes claims inspectable.** "The model is accurate" or "the agent followed the policy" is an opinion until it names the requirement and the evidence. A verified claim can be checked again by a reviewer, an auditor, or a teammate who was not there.
- **Systems change.** Models are retrained, data shifts, prompts and dependencies are updated. Confidence earned last month says nothing about today unless the requirements are checked again.
- **Responsibility needs a record.** When something goes wrong, the question is what the system was required to do, what it did, and who knew. Verification leaves that record.

Machine-learning models and AI agents make each reason stronger. Their outputs are fluent by design: a language model is trained to produce likely text, so a plausible answer is weak evidence that it is correct. The same input can produce different outputs, so checking one run says little about the next. And an agent acts as well as answers, by sending a message, changing a record, or making a payment, so a wrong result has effects that cannot be taken back.

This page develops verification through examples from ordinary software, machine-learning models, and AI agents. None of it needs a particular tool.

## A result can look right and still be wrong

A sort function receives `[3, 1, 3]` and returns `[1, 3]`.

| Input | Expected output | Observed output |
| --- | --- | --- |
| `[3, 1, 3]` | `[1, 3, 3]` | `[1, 3]` |

The observed output is in ascending order, so a glance says the function works. It also lost an item. "Sorted" contains two separate requirements:

- **Ordered:** each element is no greater than the element after it.
- **Preserves items:** the output contains exactly the input's elements, each as many times as it appears in the input.

The output meets the first requirement and violates the second. Its plausible appearance established neither. Any function that drops or invents values can produce a tidy, ordered list, and a reader who checks only the order would pass this one. Verification starts by naming each requirement on its own, so that each can be checked against something.

## What is being checked, and against what

Applied to the sort, the three parts look like this:

| Requirement | Evidence it needs | Check | Result for this call |
| --- | --- | --- | --- |
| Ordered | The output | Compare each adjacent pair | Met |
| Preserves items | The input and the output | Count each value in both | Violated: the input has two `3`s, the output one |

The second requirement cannot be checked from the output alone. That is the general pattern: the requirement decides which evidence is relevant, and a precise check over the wrong evidence answers a different question.

Evidence also has a quality. Here the check reads the actual input and output values. If it read a log line someone wrote to summarize the output, its result could be no better than that log line.

Every check rests on assumptions. Counting values assumes that equal values are interchangeable. If the function sorts customer records by age, two records with age 3 differ in their other fields, and preserving items means preserving whole records. A requirement becomes precise once its assumptions are written down.

Not every check reaches a verdict. Suppose only the output `[1, 3]` was recorded. It is ordered, but whether it preserves items cannot be determined, because nothing records the input. A violated requirement and a requirement that the evidence cannot decide are different results. Reporting the second as a pass or as a fail makes a claim the evidence does not support.

What someone does with a result is a separate step. A team may block a release, fix the function, or record the finding and move on. Verification informs those decisions; it does not have to trigger one. A proof reviewed during design, or an audit of last year's records, may change no immediate action at all.

## When verification happens

The same requirement, "preserves items", can be checked at different times, and each choice establishes something different.

- **On chosen executions, usually before deployment.** A test calls the function with `[3, 1, 3]`, with an empty list, and with a million random values, then compares counts. It establishes the requirement for those inputs.
- **On an actual execution, while it runs.** A wrapper counts the input and output of every real call and reacts before the result is used.
- **On recorded executions, afterward.** A job reads last month's logged inputs and outputs and compares their counts.

The second and third are **runtime verification**: checking whether an observed execution satisfies or violates a property, rather than establishing the property for every possible execution ([Leucker and Schallhart, 2009](#sources)). Checking an execution as it happens is online monitoring; checking a recorded trace later is offline monitoring, and both are runtime verification. These times are not exclusive categories. A test is an execution observed by a check, and a property tested before deployment can be monitored in production too.

Timing decides what a result can change. The wrapper can stop the shortened list from reaching the next step. The monthly job finds the same violation, but by then it can only start an investigation. When a check must finish before a particular action, its result has a deadline. [Real-time verification](/understand/real-time-verification/) explains what changes under that deadline.

## Different methods establish different things

Several disciplines test, check, or prove behavior, and they support different claims. The sort function shows the differences.

**Testing.** Tests show that the function preserved items for `[3, 1, 3]` and the other inputs chosen. They show nothing about inputs nobody tried: a sort that drops duplicates only in lists longer than 16 elements passes every short test. Testing is one way to gather verification evidence, and its reach is the set of cases chosen.

**Verification and validation.** NASA's systems engineering guidance separates **verification**, which shows a product meets its specified requirements, from **validation**, which shows it fulfills its intended use in its intended environment ([NASA, 2024](#sources)). The sort can be verified against "ordered and preserves items" and still fail validation. If spreadsheet users expect names in their language's alphabetical order and the function orders them by character code, it meets its specification and serves its users badly. Machine learning uses "validation set" for something narrower: held-out data used to tune a model.

**Formal proof.** A proof can show that a sorting algorithm is ordered and preserves items for every input. It holds for what was proved, whether an abstract model of the algorithm or the code itself, under stated assumptions about the environment, compiler, and hardware. Where an assumption fails, the proof says nothing about the running program. Within its assumptions it is stronger than any number of tests; it is not unconditional.

**Runtime checking.** The wrapper judges each actual call. It establishes what this execution did, including when a proof's assumptions are in doubt, and nothing about the next call.

Two common simplifications are wrong. Formal methods are not limited to deterministic software: statistical model checking samples executions of a random system and decides whether a property about probabilities holds, with quantified statistical confidence rather than certainty ([Agha and Palmskog, 2018](#sources)). And runtime monitors can check formally specified properties: a monitor for a temporal-logic property judges a finite trace as satisfying it, violating it, or inconclusive when the trace so far cannot settle it ([Bauer, Leucker and Schallhart, 2011](#sources)). Formal verification of agent decision-making also predates language-model agents ([Dennis et al., 2016](#sources)).

Neighboring practices answer related questions:

| Practice | Question it answers | What it leaves open |
| --- | --- | --- |
| Evaluation | How well does a model or agent perform on a dataset or benchmark? ([Liu et al., 2024](#sources) for agents) | How one particular request went, or how the system performs once its inputs change |
| Observability | What happened inside a system, from logs, metrics, and traces? | Whether what happened met a requirement; it records without judging |
| Guardrails | Should this input or output be blocked now? | Usually judges one message, not the process that produced it |
| Assurance | Is there a justified argument that the system is acceptable for its use? | Anything beyond the evidence the argument assembles |

These overlap. An evaluation run on a schedule against stated thresholds is a verification, and a guardrail is a runtime check on narrow evidence. The distinctions say which claim a method supports, not which teams may call their work verification.

## When there is no single expected answer

The sort has one correct output for each input. Many tasks have none. Here are two summaries of the same incident report:

```text
A: The outage began at 09:12, affected the EU region, and ended at 09:47 after a configuration rollback.
B: A configuration change took the EU region down from 09:12 until it was rolled back at 09:47.
```

They share little wording and state the same facts. Comparing either one with a stored reference summary would reject the other. Software testing calls this the **test oracle problem**: deciding whether behavior is correct for a given input when no expected output is available. Where no specification, derived oracle, or relation between runs is adequate, the fallback is costly human judgment ([Barr et al., 2015](#sources)).

Scoped properties make much of it checkable without a reference string:

- **Required facts.** The summary states the start time, end time, and region that appear in the report. A and B both pass; a summary that says 09:21 fails.
- **Prohibited content.** The summary names no customer. A list of customer names decides it.
- **Acceptable ranges.** If the summary states a duration, it is within one minute of the report's 35 minutes.
- **Relations between runs.** Reordering the report's paragraphs without changing its facts must not change the facts the summary states. A **metamorphic relation** states how outputs must relate when inputs change in a known way ([Segura et al., 2016](#sources)). Passing one shows the two outputs are consistent, not that either is correct.

The judgment can be deterministic, such as comparing a time string; statistical, such as a failure rate across many outputs; human, such as a reviewer; or model-assisted, where a language model reads the summary and the report and decides whether the facts match. Uncertainty enters from three places, and each calls for a different response:

| Source of uncertainty | Example | Response |
| --- | --- | --- |
| The system | The summarizer writes a different summary each time | Judge each output, and track the failure rate |
| The evidence | The report the summary came from was not kept | Report that the evidence cannot decide, instead of guessing |
| The judge | A model judge is unsure whether "09:12" and "just after nine" agree | Report the judgment as undetermined, or ask a person |

A model judge is a check that needs its own verification. In one large study, strong model judges agreed with human preferences about as often as humans agreed with each other, and also showed position bias, verbosity bias, and weak grading of math and reasoning ([Zheng et al., 2023](#sources)). That is agreement with human preference on the questions studied. It is not evidence of factual accuracy, or of compliance with a particular policy. Combining consistency checks with tool-based verification outperformed baseline verifiers on the reasoning benchmarks in [Han, Buntine and Shareghi (2025)](#sources), which also says little about a new domain.

Agreement and repeatability are often mistaken for correctness. Two models, or one model rerun, can agree because they share a blind spot; a repeatable judge is consistent, not necessarily right. A judge's accuracy is known only after measuring it against labeled cases from the domain where it will run: cases that clearly pass, clearly fail, and are ambiguous. The measurement holds until the judge's prompt, its model, or the requirement changes. Calibration, whether a judge's stated confidence matches how often it is right, is a further claim that needs its own measurement ([Ovadia et al., 2019](#sources) measure it for classifiers).

## What the output cannot establish

Some requirements are about the output itself, such as a sort's order or a summary's facts, and the output is valid evidence for them. Other requirements are about how the output was produced. The output can look excellent while those are broken. Three cases show which evidence has to be inspected instead.

**An optimistic evaluation.** A fraud model scores 0.98 AUC on its test set. Transactions from the same customers appear in both training and test data, and one feature was computed after the fraud label was known. That is **leakage**: information that would not be available at prediction time reaches the model during evaluation. A survey of machine-learning-based science compiled reports of leakage in 17 fields, affecting 294 papers ([Kapoor and Narayanan, 2023](#sources)). The score cannot reveal it. Detecting it means inspecting the evaluation: how the split was made, where each feature came from, and when each value became available. Without those records, whether the score is valid remains unknown.

**Changed conditions.** Six months later, the mix of merchant categories in incoming transactions has shifted. That is a change in the input distribution, which may or may not change the relationship between inputs and fraud; the drift literature calls these virtual and real concept drift ([Gama et al., 2014](#sources)). Comparing a recent window of inputs with a baseline window detects the first. Whether the relationship changed needs outcomes, which often arrive weeks later ([Gama et al., 2014](#sources)). A drift alarm is a reason to investigate. It does not show that any individual prediction is wrong. The model's own confidence is a weak guide here: for the methods studied by [Ovadia et al. (2019)](#sources), uncertainty estimates calibrated on the original distribution degraded under shift.

**An agent's claim about its own work.** A coding agent finishes and reports:

```text
Ran the test suite: 214 passed. Looked up the customer's account tier before applying the discount.
```

The message is evidence that the agent wrote this message. The first claim needs a record of the test run, such as the CI job's exit status and output. The second sentence holds two claims that need different evidence. A tool-call log or trace span showing the account lookup establishes that the call happened. It does not establish what the call returned, or that the discount matched the tier it returned. That needs the call's result, the customer it was made for, and the discount the agent then applied.

The absence of a record establishes less still. Traces can be sampled away, delayed, or dropped, and a step can run in code nobody instrumented. A missing log shows that the step may have been skipped, not that it was. It supports that conclusion only when the capture path is known to record every executed step; otherwise the honest result is that the evidence cannot decide.

In each case the output looked right and was not the evidence that decided the requirement.

## A repeatable verification practice

The examples combine into a practice with six parts:

1. **One requirement per failure that matters,** scoped to its context, with its assumptions stated: ordered and preserves items; states the report's times; the discount matches the tier the lookup returned.
2. **Evidence adequate to each requirement.** The output, when the requirement is about the output. Records of the process, such as data splits, feature origins, and tool calls with their results, when it is about the process. Evidence nobody captures yet is the first gap to close.
3. **The cheapest method that decides each requirement,** with the check itself tested on cases that should pass, fail, and stay undecided before anyone trusts it. This matters most for model judges.
4. **A response decided in advance** for each result an action depends on: on a violation, on evidence that cannot decide, and when the check itself fails to run. A check that crashed has not passed.
5. **Provenance.** Each result is recorded with the requirement, the evidence, and the versions of the system and the check it used, so a later reader can tell exactly what was verified.
6. **Revision as the system changes.** A new model, policy, or data source reruns the labeled cases. A changed requirement gets a new version, and old results stay attached to the version they used.

Two difficulties remain once a team has a method. The first is requirements. Requirements describe the world the system acts in, not only the system: requirements engineering separates knowledge of the environment, the requirements, and the specification the machine implements ([Zave and Jackson, 1997](#sources)). Production machine learning adds tests for data, models, infrastructure, and monitoring, because a model's behavior is difficult to specify in advance ([Breck et al., 2017](#sources)).

The second is infrastructure. Evidence has to be captured while the system runs, checks have to finish in time, every result has to name exact versions, and the evidence has to be kept for later investigation. Capture also has to cost little enough that it stays on.

Coding agents and model judges can help with both, by drafting capture code, proposing requirements from a policy document, generating counterexamples, and judging requirements that need context. They do not remove the need for a domain owner who decides what the requirements are, or for checks independent of the system they check. An economic argument holds that human verification capacity becomes the binding constraint as automated output grows, and that using AI to check AI spreads correlated errors ([Catalini, Hui and Wu, 2026](#sources), a preprint). Runtime verification was an established field long before language-model agents ([Leucker and Schallhart, 2009](#sources)); whether agents make it substantially cheaper or more reliable is an open question.

### Applying the ideas to a new case

A checkout service converts a cart total from US dollars to euros at the current exchange rate. A dashboard shows the average converted total for the day looks normal. What would establish that one conversion was wrong, and what would remain unknown?

The requirement applies to each conversion: the euro total equals the dollar total multiplied by the rate in effect at that moment, rounded by the stated rule. Its evidence is that conversion's dollar total, the rate it used, the published rate for that moment, and its euro total. Recomputing the total and finding a mismatch establishes a violation. A normal daily average establishes nothing about one conversion, in the same way a strong test score hid leakage. If the service never recorded when each conversion ran, the rate in effect cannot be identified, and the evidence cannot decide whether that conversion was right.

## Sources

The sources below support the claims cited beside them. For further reading, Leucker and Schallhart (2009) is a short introduction to runtime verification; Barr et al. (2015) and Segura et al. (2016) cover checking without a reference output; Kapoor and Narayanan (2023) and Gama et al. (2014) cover evaluation leakage and drift; Zheng et al. (2023) is the starting point for model judges. Koohestani (2025) is a proof-of-concept workshop paper that learns a probabilistic model of an LLM agent from its runs and model-checks it at runtime; it shows a direction, not an established method.

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

[Real-time verification](/understand/real-time-verification/) explains what changes when checks run while a system executes. [Write verifiable requirements](/build-and-verify/declare/write-verifiable-requirements/) and [Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/) turn this practice into concrete work. To implement it with a tool, [How verification works in Wyrd](/understand/how-verification-works/) shows one implementation.

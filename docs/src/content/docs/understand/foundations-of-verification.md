---
title: Foundations of verification
browserTitle: Foundations of AI Verification | Wyrd
description: What verification establishes, how testing, validation, formal proof, and runtime checking differ, how to check outputs with no single right answer, and what an ML score or an agent's claim cannot show.
pillar: wyrd
section: Understand
order: 1
---

## What verification is

Verification means checking, with evidence, that something does what it was required to do. Systems engineering uses the same definition: verification shows that a product meets its specified requirements ([NASA, 2024](#sources)).

A system that hasn't been verified isn't necessarily broken. It may work, but nobody knows yet. It is known to work only once someone compares what it did with what it was required to do.

Every verification has three parts:

- A **requirement**: a statement of what must be true. "The output is sorted" is a requirement.
- **Evidence**: a record of what actually happened, such as the input and output of one call.
- A **check**: a procedure that compares the evidence with the requirement. It reports one of three results: the requirement is met, the requirement is violated, or the check can't tell.

The requirement has to be written down before anyone looks at the result. A requirement written afterward tends to describe whatever the system happened to do, so the result can't fail it. A requirement written beforehand is fixed, so the result can fail it.

## Why verification is important

Verification is how a team knows a system works instead of assuming it does. Without verification, the team trusts results because they look right, and problems are found by the people they affect. A customer receives the wrong refund. A confidential report reaches someone who wasn't allowed to see it.

Verification matters for four reasons:

- **A problem caught early does less damage.** A check that runs before a result is used can stop a wrong result from being used. A wrong result found later has already been used, so the team can only explain the damage and clean it up.
- **Others can check the claim.** The claim "the model is accurate" is only an opinion until the claim says what "accurate" means and what evidence supports it. Once it does, a reviewer, an auditor, or a teammate can check the claim for themselves.
- **Systems change.** Models are retrained, data changes, and prompts and libraries are updated. A system that passed its checks last month may not pass them today, so the checks have to run again.
- **Someone has to be accountable.** When something goes wrong, people ask what the system was supposed to do, what it actually did, and who knew about the problem. Verification keeps a record that answers those questions.

AI models and agents make verification more pressing, for three reasons:

- A language model is trained to produce text that sounds right. So an answer that sounds right is weak evidence that it is right.
- The same input can produce a different output each time. So checking one run says little about the next.
- An agent takes actions, such as sending a message, changing a record, or making a payment. So a wrong result can cause harm that can't be undone.

The rest of this page uses examples from ordinary software, AI models, and AI agents. The ideas apply whatever tools a team uses.

## A result can look right and still be wrong

A sort function receives `[3, 1, 3]` and returns `[1, 3]`.

| Input | Expected output | Observed output |
| --- | --- | --- |
| `[3, 1, 3]` | `[1, 3, 3]` | `[1, 3]` |

The output is in order, so at a glance the function seems to work. But it lost one of the `3`s. "Sorted" is really two separate requirements:

- **Ordered:** each item is no larger than the item after it.
- **Keeps every item:** the output contains the same items as the input, each the same number of times.

The output meets the first requirement and fails the second. Someone who checks only the order would pass this output, because a function can drop items and still return an ordered list. So verification starts by writing each requirement down separately. Each requirement then gets its own check.

## What is being checked, and against what

Here are the requirement, the evidence, and the check for each half of "sorted":

| Requirement | Evidence it needs | Check | Result for this call |
| --- | --- | --- | --- |
| Ordered | The output | Compare each item with the next one | Met |
| Keeps every item | The input and the output | Count each value in both | Violated: the input has two `3`s, the output has one |

The second requirement can't be checked from the output alone, because the check needs to know what went in. This is true of every requirement: its check needs particular evidence, and without that evidence the check can't decide. A check that reads only the output can confirm the order, but it can't tell whether items were lost, however carefully it is done.

The evidence must be the actual input and output, not a description of them. Here, the check reads the actual input and output. If it read a log message someone wrote to describe the output, the result could only be as accurate as that message.

A requirement can depend on an assumption. Suppose the function sorts customer records by age, and two customers are both 30. They are two different people, so "keeps every item" means keeping both records. Counting how many records have age 30 isn't enough. A broken function could drop one of the two customers and output the other twice, and the count would still be two. Counting works only when equal values are interchangeable, the way two copies of the number 3 are. So the requirement has to say which kind of item it is about: here, each customer record must appear exactly once.

Sometimes a check can't decide whether a requirement is met. Suppose only the output `[1, 3]` was recorded. It is in order, but nobody can tell whether an item was lost, because the input wasn't kept. The result is "can't tell", which is different from "violated". Reporting it as a pass, or as a fail, claims something the evidence doesn't show.

Deciding what to do about a result is separate from checking. A team might block a release, fix the function, or note the problem and move on. Verification tells the team what happened, and the team decides what to do.

## When verification happens

The same requirement, "keeps every item", can be checked at three different times:

- **Before release, a test checks chosen inputs.** It calls the function with `[3, 1, 3]`, an empty list, and a million random numbers, and compares the counts. If the test passes, the requirement holds for those inputs.
- **While the system runs, a wrapper checks each real call.** A wrapper is a small piece of code around the function. It compares the counts on every real call before the result is passed on.
- **Afterward, a job checks recorded calls.** It reads last month's recorded inputs and outputs and compares their counts.

Checking real runs, as they happen or afterward, is called **runtime verification** ([Leucker and Schallhart, 2009](#sources)). It shows what happened in the runs that were checked, and nothing about the runs that weren't. The three times aren't fully separate. A test is also a run that gets checked, and a requirement tested before release can be checked again in production.

The time a check runs decides whether its result can prevent a problem or only report it. The wrapper can stop a shortened list before the next step uses it. The monthly job finds the same problem a month later, when all anyone can do is investigate. A check meant to prevent an action must finish before that action happens, so it has a deadline. The page [Real-time verification](/understand/real-time-verification/) explains checking under a deadline.

## Different methods establish different things

Testing, validation, proof, and runtime verification each answer a different question.

**Testing checks the cases someone chose.** Tests can show that the sort kept every item for `[3, 1, 3]` and the other lists tried. They say nothing about lists nobody tried. For example, a sort that drops duplicates only in lists longer than 16 items passes every test that uses short lists.

**Validation checks that the requirements are the right ones.** Verification asks whether a product meets its requirements. Validation asks whether the product does the job it is meant for ([NASA, 2024](#sources)). A sort can meet both of its requirements and still be wrong for its users. Suppose users expect names in the alphabetical order of their language. The sort instead orders names by the number the computer stores for each letter. Names that start with an accented letter, such as "Émile", then end up after "Zoe".

**A proof covers every input.** A proof can show that a sorting algorithm puts every list in order and keeps every item, for any list, not only the lists tested. But a proof is about the code as written. It assumes that the tools that turn the code into a program, and the computer that runs it, work correctly. If one of those assumptions is false, the proof says nothing about how the program actually runs.

**Runtime verification covers the runs that actually happen.** A runtime check looks at each real call, so it catches a failure even when one of a proof's assumptions turned out to be false. But it covers only calls that have already happened, not the next one.

These methods work best together. Take the rule "every payment comes after an approval." Once the rule is written down precisely, it can be checked three ways. A proof can show that the system's design never allows a payment without an approval. Tests can try the cases the team thought of. A check on the live system can catch a run nobody predicted. Each gives a different kind of confidence, and none replaces the others.

Other terms describe related work:

- An **evaluation** scores a model or agent on a set of test examples. It says little about how any single real request went.
- **Observability** records what happened, in logs and traces. It doesn't judge whether what happened was acceptable.
- A **guardrail** blocks a single unacceptable message, such as a reply that contains a card number. It judges only the message, not whether the steps that produced it were correct.
- An **assurance case** is a written argument, built from the results of all these methods, that a system is fit for its purpose.

Evaluation, observability, and guardrails become verification when they check a written requirement against evidence. An assurance case collects those results.

## When there is no single expected answer

Sorting a list has one right answer. Many tasks have more than one. Here are two summaries of the same incident report:

```text
A: The outage began at 09:12, affected the EU region, and ended at 09:47 after a configuration rollback.
B: A configuration change took the EU region down from 09:12 until it was rolled back at 09:47.
```

Both summaries are correct. They use different words to say the same thing.

Having more than one correct answer breaks the usual way of testing. A normal test stores one correct answer and checks that the output matches it. If the stored answer is summary A, the test fails summary B, even though B is also correct. Testing researchers call this the **test oracle problem**: for many tasks, there is no single correct answer to compare against ([Barr et al., 2015](#sources)).

The way around it is to stop asking "does this match the correct answer?" and ask "does this have the properties every correct summary must have?" Each property can be checked on its own:

- **It includes the key facts.** Any correct summary must give the start time (09:12), the end time (09:47), and the region (EU). A and B both do, so both pass. A summary that says the outage began at 09:21 fails.
- **It leaves out forbidden content.** For example, a summary must not name a customer. A program can check this by searching the summary for every name on the customer list.
- **Its numbers are close enough.** The outage lasted 35 minutes. A summary may round the length, so if a summary mentions it, a check can accept anything from 34 to 36 minutes.
- **It gives the same facts when the input is rearranged.** Reordering the report's paragraphs doesn't change what happened, so the summarizer should still report the same times and region. If the two runs report different facts, the summarizer is unreliable. If they report the same facts, the summarizer is consistent but not necessarily correct, because both runs could make the same mistake ([Segura et al., 2016](#sources)).

Something has to run each check and decide pass or fail. This page calls that the **judge**. A judge can be a program that compares values, a person, or an AI model asked whether the summary matches the report.

Sometimes a check can't reach a clear result. The doubt can come from three places, and each needs a different response:

| Where the doubt comes from | Example | Response |
| --- | --- | --- |
| The system | The summarizer writes a different summary each time it runs | Check every summary, not one, and track how often checks fail |
| The evidence | The original report wasn't kept, so there is nothing to compare the summary with | Report "can't tell" instead of guessing |
| The judge | An AI judge can't decide whether "09:12" and "just after nine" mean the same time | Report "can't tell", or ask a person |

## Can an AI judge be trusted?

An AI model used as a judge is a check like any other, and it can be wrong.

One large study compared AI judges with human raters ([Zheng et al., 2023](#sources)). The strongest AI judges agreed with the humans about as often as the humans agreed with each other. But the AI judges often preferred an answer because of the order the answers were shown in, or because it was longer, rather than because it was better. They also graded math and reasoning poorly.

Agreeing with people is also not the same as being right. A judge can match what people prefer and still get a fact wrong or misread a company's policy.

Two common shortcuts don't show that a judge can be trusted:

- **Using a second AI to check the first.** Both models can make the same mistake, so the second one may agree with an error.
- **Getting the same answer every time.** A judge that always gives the same answer can be wrong every time.

The only way to know whether an AI judge can be trusted is to test it on examples from the real task where the right answer is already known. The examples should include cases that clearly pass, cases that clearly fail, and unclear cases. The test has to be repeated whenever the judge's instructions (its prompt), the model behind it, or the requirement changes.

## What the output cannot establish

Some requirements are about the output itself. Whether a list is sorted can be checked by looking at the list. Other requirements are about how the output was produced. For those, the output can look perfect and still prove nothing. The three examples below show what to look at instead.

**Example 1: a test score that's too good.** A fraud-detection model scores very well in testing, but the test was built wrong in two ways:

- The same customers appear in the data the model learned from and in the data it was tested on. The model can recognize those customers instead of learning what fraud looks like.
- One of the model's inputs was calculated after each fraud had been discovered. In real use, that information doesn't exist yet when a purchase happens.

Both mistakes give the model information it won't have in real use, so the test score is higher than its real performance. Both are called **leakage**. It is common: one survey found it reported in 294 published studies across 17 fields ([Kapoor and Narayanan, 2023](#sources)).

Leakage can't be seen in the score itself. Finding it means checking how the test was built: how the data was split, where each input came from, and when each value became known.

**Example 2: conditions that changed.** Six months after launch, customers are buying from different kinds of shops. That change can mean one of two things ([Gama et al., 2014](#sources)):

- **Only the inputs changed.** Customers shop differently, but the signs of fraud are the same. Comparing this month's transactions with an earlier month shows this kind of change.
- **The signs of fraud changed.** Detecting this requires knowing which transactions turned out to be fraud, and that often takes weeks.

An alert that the data has changed, called a **drift alert**, is a reason to look closer. It doesn't prove that any prediction is wrong.

Many models also report a confidence score with each prediction. When the data changes, that score can't be relied on: one study found that confidence scores become less accurate after the data changes ([Ovadia et al., 2019](#sources)).

**Example 3: an agent describing its own work.** A coding agent finishes a task and reports:

```text
Ran the test suite: 214 passed. Looked up the customer's account tier before applying the discount.
```

The message proves only that the agent wrote it. It makes three claims, and each needs its own evidence:

- **The tests passed.** This needs the record of the test run, such as the result and output of the automated test run.
- **The lookup happened.** This needs a log showing the agent called the lookup tool.
- **The discount matched the tier.** The log of the call isn't enough. This needs what the lookup returned, which customer it was for, and the discount the agent applied.

Sometimes there is no record at all. Some systems log only some runs. Records can arrive late or get lost. Some code writes no records at all. So when there is no record of a step, the step may or may not have happened. A missing record proves that a step was skipped only if every step is always recorded. Otherwise, the check can't tell.

In all three examples, the output looked right. The evidence that mattered was somewhere else: in how the test was built, in what happened later, and in the records of what the agent did.

## A repeatable verification practice

Taken together, the examples on this page come down to six things. Any system can be verified with them:

1. **A requirement for each failure that matters.** For example: the list keeps every item, the summary gives the report's times, and the discount matches the customer's account tier. Any assumption the requirement depends on is written down with it.
2. **Evidence that can show whether the requirement is met.** When the requirement is about the output, the output is the evidence. When it is about how the output was produced, the evidence is a record of how it was produced, such as how the data was split, or which tools an agent called and what they returned. If that record doesn't exist yet, creating it comes first.
3. **The simplest check that works.** Before anyone relies on the check, it is tried on cases where the right result is known: some that should pass, some that should fail, and some where it should report "can't tell". This matters most when the check is an AI judge.
4. **A plan for each result, decided in advance.** The team decides beforehand what happens when a check fails, when it can't tell, and when the check itself breaks and returns no result. A check that breaks never counts as a pass.
5. **A record of each check.** The record names the requirement, the evidence, and the versions of the system and the check, so anyone can later see exactly what was verified.
6. **Updates when the system changes.** When the model, policy, or data source changes, the check is run again on the cases where the right result is known. A changed requirement gets a new version, and old results stay linked to the version they were checked against.

Two things are hard to do well: writing the requirements, and building the system that runs the checks.

**Writing good requirements.** A requirement often depends on facts outside the system. "Only send reports to authorized recipients" depends on who is authorized, and people outside the system decide that ([Zave and Jackson, 1997](#sources)). AI models make requirements harder to write, because it is hard to say in advance exactly how a model should behave. Because a model's behavior is hard to pin down, teams that run models in production also write requirements for things they can pin down: the data the model receives and how it is monitored ([Breck et al., 2017](#sources)).

**Building the system that runs the checks.** It has to capture evidence while the system runs, finish each check before the result is needed, and store each result with the exact versions involved. It also has to be cheap enough to run on every call.

AI tools help with both, but only in part. They can write logging code, suggest requirements from a policy document, find inputs that break a requirement, and act as the judge for requirements that need understanding, such as whether a reply follows a policy. They don't replace a person who owns the requirements. They also don't replace independent checks, because an AI tool can share the blind spots of the AI system it checks. One economics paper argues that using AI to check AI repeats those blind spots instead of catching them ([Catalini, Hui and Wu, 2026](#sources)).

### Applying the ideas to a new case

A checkout service converts each cart total from US dollars to euros, using the exchange rate at the moment of purchase. A dashboard shows today's average euro total, and it looks normal. Does that mean every conversion was right?

No. An average across thousands of purchases can look normal while a few purchases are wrong, because a handful of bad conversions barely moves it.

To catch a bad conversion, each purchase is checked on its own. The euro total must equal the dollar total times the exchange rate at the time of purchase, rounded to the cent. That check needs four records for the purchase: the dollar total, the euro total, the rate the service used, and the official rate at that time. The check multiplies the dollar total by the official rate and compares the answer with the euro total. It also catches a service that used an out-of-date rate, because that rate won't match the official one.

If the service never recorded when each purchase happened, nobody can look up which official rate applied. The check can't say whether that conversion was right or wrong. It can only report "can't tell".

## Sources

The sources below support the claims cited beside them. For going deeper:

- Leucker and Schallhart (2009): a short introduction to checking real runs.
- Barr et al. (2015) and Segura et al. (2016): checking software when there is no single right answer.
- Kapoor and Narayanan (2023) and Gama et al. (2014): test scores that mislead, and data that changes.
- Zheng et al. (2023): how far AI judges can be trusted.
- More specialized: Agha and Palmskog (2018) and Bauer, Leucker and Schallhart (2011) on proof-style checks for systems with randomness and for live runs; Dennis et al. (2016) on proving autonomous agents' decisions; Koohestani (2025), an early proof of concept for checking LLM agents while they run.

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

[Real-time verification](/understand/real-time-verification/) explains what changes when checks run while a system executes. The guides [Write verifiable requirements](/build-and-verify/declare/write-verifiable-requirements/) and [Establish sufficient evidence](/build-and-verify/observe/establish-sufficient-evidence/) show how to apply this practice. [How verification works in Wyrd](/understand/how-verification-works/) shows one tool that implements it.

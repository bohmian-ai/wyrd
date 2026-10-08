# TASK-009 concurrency and context-isolation review

## Subject and independence

Fresh reviewer `concurrency_resume`; returned report preserved by orchestrator under reviewer file-edit prohibition. Base `7d96c30066425e0cde2290842d5801307843283d`; candidate `c47761decff8db95768d2c05f0b85b8ba62a021a`. Original TASK-009, approved spec/mapped revision-35 obligations and R1 remediation govern. Full cumulative production/declaration/dependency/test diff inspected; candidate stayed HEAD; no peer R2 reports read; no CodeGraph index.

## Authority and boundary coverage

Read AGENTS §§2–8,11–12,15–16, agent rules, spec-driven-development, maintainer style, Python/PyO3/testing/telemetry references, design client/observation identity and Run API.

| Invariant | Source and assessment |
|---|---|
| Immutable Card/Run identity | native alias resolves before construction; Python/TS delegate |
| One concurrent private key | otel.py:201–212 lazy second check under existing lock; producer and processor share key |
| Execution-local stack | otel.py:189–198,279–297 immutable ContextVar tuple; one entry per attempt, no token on Run |
| Nested/async restoration | exit:300–326 pops local stack, exact detach and prior fallback |
| Serialized registration | install:246–276 lock protects registry/install; failed install removes entry; stock SDK add does not invoke callbacks/reenter key |
| Copied task context | native contextvars copy; child entries own tokens; inherited pair after parent exit is approved |
| Explicit sibling observations | PyO3 best effort; Drift/record native Run; Eval active IDs separately best effort |
| Lifecycle | no exporter/queue/flush/shutdown/network/provider ownership added |

## Producer-to-consumer and recovery

PyRun entry passes exact CardRef/Run ID into entry, which obtains one key, records prior, attaches pair and token. Processor reads supplied parent context. Lock protects process metadata, while values and stack remain local; published key reads need no lock. Exit attempts exact detach and restores recorded prior when detach raises or silently leaves inner installed. Inner recovery retains outer Card; outer recovery leaves no pair. Optional fallback failure remains contained as required.

Installed OTel source confirms UUID-backed distinct keys, copied set_value contexts and ContextVar attach/reset tokens. Public detach swallows reset exceptions, justifying value check. No raw-thread propagation or arbitrary hanging-provider availability guarantee is inferred.

## Prior closure

IDs 4 and 3 close via serialized key and deterministic thread proof, and same-context nested/outer failed-detach restoration. Domain-relevant ID 1 attach/enrichment/detach cases reach ordinary Drift; overall category completeness belongs to task reviewers. ID 2 corrected documentation agrees with constructors and has no concurrency effect.

## Verification and result

Independently ran from SDK `mise exec -- uv run python -m pytest -q tests/unit/state/test_observe_surface.py`: **33 passed in 0.85s**, exit 0. Tests include active/child, nesting, await/concurrent/copy, provider idempotency, first-use and detach recovery. Real journey/broad gates inspected as source and supplied evidence, not rerun. No material findings or open questions.

**PASS**.

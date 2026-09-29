---
id: SPEC-gateway-benchmarks
revision: 5
status: draft
---

# Gateway benchmarks against LiteLLM

## Objective

Publish a reproducible comparison of Wyrd and LiteLLM on inference tasks both products can perform, plus separate Wyrd evidence for credential control, security, and accountability. Compare client-visible behavior without claiming that LiteLLM virtual keys and Wyrd principals have the same implementation. This backlog draft does not authorize implementation.

## Fixed comparison setup

- **REQ-001 — Products.** The headline comparison uses a pinned Wyrd build and a pinned stable LiteLLM Python proxy. LiteLLM's Rust gateway may be a separately named result only when the measured endpoint and behavior are supported. Never blend LiteLLM Python and Rust data.
- **REQ-002 — Common task.** Both gateways receive the same OpenAI-compatible `/v1/chat/completions` request from the same load driver, use one authorized client key with access to one named model, route to the same deterministic local mock provider, and return semantically equivalent responses. Wyrd uses its service-principal mechanism; LiteLLM uses a virtual key. The comparison concerns the same client task, not equivalent internal identity models.
- **REQ-003 — Reference environment.** Run on a dedicated Linux host with 16 vCPU and 64 GiB RAM. Run only one gateway at a time, each limited to 4 vCPU and 8 GiB RAM. Put the driver, mock, and PostgreSQL in separate containers on the same private container network, each with published resource limits. Give both gateways the same network path, connection reuse, mock behavior, timeout, and Postgres capacity. Use a pinned digest of the published [`zerob13/mock-openai-api`](https://hub.docker.com/r/zerob13/mock-openai-api) image as the initial shared mock. Record image digests, exact config, host model, kernel, container limits, CPU throttling, and any extra service a product requires.
- **REQ-004 — Normal product paths.** Keep each gateway's required authentication, routing, accounting, and audit behavior enabled. Publish an enabled-feature matrix beside every result. Differences in mandatory work must be stated; never call the numbers an isolated forwarding-cost or feature-parity result. Do not replace a networked mock with an in-process mock for either product.
- **REQ-005 — Direct control.** For each measured workload, run the same client directly against the mock at the same offered rate. Report direct latency and mock/driver saturation alongside gateway results. Direct-to-mock is a control, not a competing product.

## The four benchmark runs

| Run | Load and mock setup | Report | What the comparison establishes |
| --- | --- | --- | --- |
| **1. Request overhead** | 10 requests/second. Fixed short and large chat payloads, published in bytes and tokens. Mock waits 20 ms and returns a fixed response. Warm up 60 seconds; measure 180 seconds. | Client p50/p95/p99 completion latency, direct-control latency, CPU, peak memory. | Client-observed cost of the same short and large chat task at low load. |
| **2. Streaming** | 10 requests/second. Same chat request; mock emits 10 fixed SSE chunks at 50 ms intervals. Warm up 60 seconds; measure 180 seconds. | Time to first valid event, completion time, inter-chunk p95/p99, CPU, peak memory. | Delay added to a valid stream. Ordered chunks and complete content are correctness gates. |
| **3. Capacity** | Same authorized short chat task and 20 ms mock response. Offer 50, 100, 200, 400, then 800 requests/second, for 180 seconds at each step after a 60-second warmup. The driver schedules by clock independently of completions. | Offered, started, and successful RPS; p95/p99; errors, timeouts, missed starts; CPU and peak memory. | The load each product sustains for this task. Show the full curve, including overload; do not report only peak RPS. |
| **4. Failover, conditional** | Same authorized request at 50 requests/second for 180 seconds: primary mock healthy for 60 seconds, returns 503 for 60 seconds, then recovers; second mock is fallback. | Complete success/error rate, fallback latency, upstream attempts, recovery time, selected upstream. | Comparable outage behavior only when both products are configured and verified with the same primary/fallback, retry conditions, timeouts, and attempt limits. Otherwise publish Wyrd-only results and mark LiteLLM comparison `N/A`. |

- **REQ-006 — Measurement boundary.** Use client monotonic timestamps for all comparable latency measurements. A valid non-streaming response must be complete; an aborted stream is a failure. Report first valid SSE event, not just HTTP headers. Do not subtract unrelated p99 values or treat a vendor overhead header as a cross-product metric.
- **REQ-007 — Qualification.** Run each product sequentially and alternate product order across at least three trials. Pair each trial with a direct-control run. Use a fixed, repeatable mock scenario rather than ordered recording replay, whose shared cursor consumes responses by arrival order under concurrency. Verify driver and mock headroom, 20 ms response delay, ten-chunk 50 ms streaming cadence, response equivalence, and intended policy before accepting a trial. Publish warmup, measured duration, raw observations or lossless histograms, exclusions and their reasons, and trial spread. A shifted direct control, missed offered load, saturated driver/mock, unstable mock timing, or invalid response disqualifies a headline result. If the published image cannot meet a run's controlled behavior, use one small pinned local fixture for that run and use that same fixture for both gateways and the direct control.
- **REQ-008 — Capacity claim.** State a client-visible latency/error objective before testing and define sustained capacity as the highest tested offered rate that meets it. Publish successful RPS as well as offered RPS. Any cost-per-request estimate must state resource prices, utilization, formula, and excluded costs.
- **REQ-009 — Comparison discipline.** Only runs from the same campaign and common task may appear in a Wyrd-versus-LiteLLM chart. Publish exact product versions, configs, enabled-feature matrix, raw data, and reproduction commands. Label mandatory feature differences and do not call the result a matched-internals or pure-forwarding comparison. LiteLLM's published benchmark figures are context, not data points in Wyrd's chart.

## Wyrd-only correctness evidence

- **REQ-010 — Governance checks.** Separately report pass/fail evidence for authorized access, forbidden model/deployment access, tenant isolation, upstream credential selection, secret redaction, accounting, audit completion, and fallback attribution. Use two synthetic teams and credentials where the shipped Wyrd contract supports them. These are correctness and security checks, not additional speed benchmarks; they do not imply a full security audit.
- **REQ-011 — Current capability.** Mark deployment-scoped authorization and explicit/automatic deployment selection `N/A` until delivered. Do not include priority queuing in this change.

## Invariants

- **INV-001:** No real provider credential, billable call, sensitive prompt, or live tenant data is required. Public artifacts contain no secret values.
- **INV-002:** Do not disable Wyrd's required auth, accounting, or audit to improve a comparison. Report product configuration differences openly.
- **INV-003:** Full load runs are opt-in on a dedicated host, outside ordinary test and pull-request gates. A bounded, credential-free harness smoke check may run in verification without a performance assertion.
- **INV-004:** A single-host result supports only the published workload and configuration, not a universal latency, security, availability, or cloud-cost claim.

## Acceptance

- **AC-001:** Another contributor can reproduce the four defined runs from published commands, fixed payloads, config, mock behavior, and pinned image digests without provider credentials. The report identifies which runs use the published mock image and which, if any, use a local fixture.
- **AC-002:** The report contains side-by-side results for runs 1–3 and run 4 only when failover policies match; Wyrd-only governance checks appear in a separate pass/fail section.
- **AC-003:** Every chart can be recomputed from published raw data and identifies the direct control, load offered, successful requests, failures, resource limits, and enabled features.
- **AC-004:** Driver/mock saturation, invalid response content, or incomplete required accounting/audit evidence cannot appear as a qualified headline result.

## Open material decisions

None for this backlog draft. Exact prompt bytes, token-count method, capacity objective, and tool choices must be fixed in the published run manifest before headline measurements.

## Authority and revision

- `AGENTS.md` §10–11; `architecture/agent-rules.md`; `architecture/wyrd-design.md`; `architecture/wyrd-doctrine.mdx`; `architecture/operations/reliability-and-recovery.md`; `architecture/references/languages/testing-workflows.md`.
- External context: [LiteLLM proxy benchmarks](https://docs.litellm.ai/docs/benchmarks), [LiteLLM virtual keys](https://docs.litellm.ai/docs/proxy/virtual_keys), [LiteLLM Rust gateway](https://docs.litellm.ai/docs/proxy/rust_gateway), [AIGatewayBench](https://github.com/BerriAI/ai-gateway-bench), and [mock-openai-api](https://github.com/zerob13/mock-openai-api).
- Revision 1 — draft, 2026-09-25: initial backlog specification.
- Revision 2 — draft, 2026-09-25: expanded comparison and evidence scope.
- Revision 3 — draft, 2026-09-25: separated correctness checks and removed priority queuing.
- Revision 4 — draft, 2026-09-25: fixed four benchmark runs and limited competitor charts to comparable client tasks.
- Revision 5 — draft, 2026-09-25: selected the published mock image for qualification, with one shared local fixture only when a required run cannot be controlled with it.

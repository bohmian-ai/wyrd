# Wave 2 findings validation — TASK-003-R4

## Immutable subject and scope

Base `861f8d86cc3f9d7e70fb59489e80f8be62afddbf`; candidate `24b8e78d7420321eb8fe936b3045fcbd3bc1d26b` (tree `6a3ec967ab094a42256b44ce908abb080360e466`). The last source commit is `d6de890d83f269eab834fa323f3ebe31f1adc573` (tree `d8a727b5846f24bac7b55bbc786bcb391c55cb03`); the candidate's later commit adds only R3 review reports and the R4 evidence packet. `HEAD` remained the candidate during validation, `git diff --check` passed for the cumulative base-to-candidate range, and no `.codegraph/` directory exists.

I read all six Wave 1 reports, the approved revision-9 spec, original TASK-002/004/003, prior R3 verdict and validation, the R4 task and evidence, `AGENTS.md`, `architecture/agent-rules.md`, the spec-driven authority, the cumulative path inventory, and the exact R4 source diff. I inspected the current forwarding/peer/test call path, the relevant Forge and storage owners, and the gate dependency definition. This is validation of task acceptance, not a second implementation pass.

## Validation of the Wave 1 empty finding union

| Report | Decision and independent basis |
|---|---|
| `task-review.md` | **VALIDATED EMPTY.** The R4 implementation delta is only import/type spelling and rustdoc in `oracle/forwarding.rs` and `state.rs`; no runtime branch, public contract, feature, dependency, or test-selection change is present. The cumulative change has the prior task/review chain and the final-source nonzero lane record. No missing or widened R4 behavior was found. |
| `standards-review.md` | **VALIDATED EMPTY.** `forwarding.rs:121-125,1050-1077` now documents both tuple fields and the assertion-bearing test's panics. Its module-top, feature-gated imports and `state.rs:41-42,1734-1741` use bare type names at the previously flagged positions, as required by `AGENTS.md` §16 and `architecture/agent-rules.md`. |
| `domain-review-forwarding.md` | **VALIDATED EMPTY.** `ReadyOracleForwarder::forward` is the production caller of `route_remote_once`; its `forward_remote` delivery includes credential acquisition and waiting for the peer's first response. `route_remote_once:592-618` uses the ingress-captured monotonic deadline for selected delivery, returns typed `QueryTimeout` on expiry, drops the pending future, and has no successor-delivery path. The exact unit test and the role-separated HTTP journey exercise that path. R4 did not alter it. |
| `domain-review-peer-security.md` | **VALIDATED EMPTY.** The private handler authenticates the peer, requires an envelope and open Gate, and checks the Oracle role before calling the test-only hold; ordinary ticket verification remains in `accept`. `SilentForwardPeer`, its export/accessor and its sole handler call are `test-support`-gated. The hook has a concrete journey caller and was needed to park an authenticated peer before its first response, so deletion would lose the required cancellation proof. No production trust bypass was added. |
| `domain-review-forge.md` | **VALIDATED EMPTY.** The cumulative Forge correction reuses `ForgeTablePolicy::extract`, the existing settlement owner, and a tenant-qualified partial unique index instead of adding a second policy or task authority. R4 changes none of those source paths; their final-tree SQL, journey, recovery and geometry results are recorded. No new complexity or reopened durability gap was identified. |
| `domain-review-storage-ci.md` | **VALIDATED EMPTY.** The selected `WYRD_STORAGE_URL` backend remains the single signer/operator source; the cloud workflow triggers on push to `main` and invokes the three provider `mise` tasks. The approved spec accepts final-tree local `mise.local.toml` S3/GCS/Azure proof before merge, recorded as two passing tests per provider. R4 touches neither storage nor workflow code. |

## Prior-finding closure and minimality

| Stable prior finding | Decision |
|---|---|
| `FIND-TASK-003-R3-1` | **CLOSED.** The R4 packet records `CARGO_BUILD_JOBS=4 mise run -j 1 gate` exit 0 on the final source tree, with nonzero Bifrost and Rust selections. `mise.toml` defines `gate` through the existing dependency list; `CARGO_BUILD_JOBS=4` only limits compiler parallelism. `check:bifrost` passed separately, completing the declared `verify:bifrost` children. The current user expressly accepts complete same-tree child execution in place of a single gate invocation; this candidate additionally records a completed gate. One killed *separate* non-gate runner is not counted as passing, and its unfinished lanes are recorded as successfully rerun on that same tree. |
| `FIND-TASK-003-R3-2` | **CLOSED.** The exact missing `Abandon.0`, `DropProbe.0`, and `# Panics` documentation is present. No extra wrapper, helper, or test was added. |
| `FIND-TASK-003-R3-3` | **CLOSED.** The previously qualified field, return and bound types now use module-top imports and bare names, with test-only imports gated. The source diff preserves runtime behavior and feature ownership. |

Earlier R1/R2 findings remain closed by the cumulative candidate and were not reopened by the R4 two-file documentation/import correction. The Ponytail ladder yields no new deletion or refactor obligation: the R4 edits add no abstraction, configuration knob or dependency; the earlier test-only silent peer has a required real journey caller; existing timeout, policy, storage and gate owners are reused. Removing any of these to save lines would discard required proof or behavior.

## Verification limits and final ledger

The final-tree gate, focused tests, six owner lanes, language/storage journeys, and credentialed local cloud tasks are recorded in the committed R4 packet with commands and nonzero selections. I checked the source/tree identity, task mapping, source paths and diff, but did not rerun heavy or credentialed suites or obtain their raw runner logs. This is a stated evidence limit, not a new finding or a reason to require redundant checks. The later evidence-only commit does not invalidate source-tree test results.

**Deduplicated validated finding ledger: empty.** No `FIND-TASK-003-R4-*` ID is assigned; no correction or specification revision is recommended.

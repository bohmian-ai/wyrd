# Approval resolution and remediation readiness — TASK-001

**Decision resolved. Recommended task verdict: FIX_REQUIRED. Remediation packet: READY.**

This is a narrow independent review of the human-approved authority amendment and `TASK-001-R1-deployment-addressing-remediation.md`. It does not rerun discovery, alter the original finding ledger, or close any implementation finding. The reviewed production/test source remains candidate `ef2fe8bda2071f7b32cb6803b7f9f4599f0d4183` in `/tmp/wyrd-task-review-ef2fe8bd`, against base `1d3b5ffa0d34f75fa5b22c9b68f6ec0fc8d0cc70`. Live revision 6 and the delegation authority passages were read only as explicitly authorized post-review authority. Later implementation/test changes remain unreviewed.

## Decision closure

The user's explicit choice, **“Reject unrepresentable exchanges (recommended)”**, together with the requirement that both actor and subject have permission, settles the previously material choice in FIND-001-1. This is human approval, not a new reviewer finding.

The approved revision 6 now requires exact behavioral intersection for successful issuance; when the complete intersection cannot be represented by current token scopes, exchange refuses before signing and emits no token. It forbids amplification, silent attenuation, new conditional scopes, and rejection merely because the inputs mix broad and explicit grants. Explicit deployment/model grants survive normalization beside All. Its REQ-002 and AC-002 amendment, approval record, and revision-history entry agree.

The corresponding passages in `architecture/wyrd-design.md` (identity and cross-service delegation), `architecture/wyrd-security-posture.md` (delegation effective permissions), and `architecture/v1/00-foundations/permission-model.md` now state the same rule. Successful delegated tokens retain exact shared authority; attribution supplies none. There is no remaining authority conflict requiring SPEC_REVISION_REQUIRED for this established defect.

The packet correctly distinguishes an unrepresentable intersection from a representable empty intersection. It preserves representable complete mixed results rather than banning each All×scoped pair independently. It also avoids freezing current Open deployment configuration into explicit grants. These boundaries are necessary to implement the approved refusal policy without weakening existing successful exchanges.

The selected derive-backed authorization error, HTTP 403 and stable `WYRD_AUTH_403_UNREPRESENTABLE_DELEGATION` code are a concrete ordinary projection of this approved refusal through the existing catalog, issuer, exchange failure and audit owners. They add no permission, route, scope, new refusal category beyond the approved one, or separate issuance lifecycle. The packet prohibits secret-bearing details and permission-widening remediation text. No further product decision is required for that projection.

## Packet contract assessment

The packet is self-contained for implementation: approved r6 and original task references; immutable base/candidate; existing-owner map; diagnosis, source anchors, concrete consequence and selected correction for every retained finding; bounded documentation inventory; preserved behavior/non-goals; acceptance criteria covering all thirteen IDs; focused closure commands and exact-selector discipline; and return to cumulative immutable task review. It does not claim implementation or verification completion.

| Findings | Readiness assessment |
|---|---|
| FIND-001-1 | Exact representability/normalization stays in PermissionSet; refusal propagates through existing issuer/exchange before signing, with ordinary stable error and nonblocking denial audit. Both directions, mixed representable sets, broad axes, empty result, no-token and no-upstream proof are explicit. |
| FIND-001-2 | Agent retains declared state and resolves invocation-local route through its existing gateway/registry execution seam. Load/save, direct Python lifetime, tool loop, missing dependencies and isolation are covered; native/server and sub-agent boundaries survive. |
| FIND-001-3 | Corrects the current public protocol producer and earlier Jev single-call rejection together. Actual ingress proof, slash suffix, deployment named vertex and actual Vertex refusal are distinguished. No new general Jev loop/tool capability is required. |
| FIND-001-4/5/8/11 | Shared pure Prompt/model/schema/media owners prevent invalid inputs before mutation/storage/IO; runtime builders, Card decoding, binding and language consumers reuse the invariants. Existing media budget/source restrictions and native options survive. No duplicate SDK validation, catalog, fetch or upload subsystem. |
| FIND-001-6 | Enforces the already approved initial Agent provider/adapter matrix at existing admission while preserving direct-client translation and explicit fallback conversion. Private carriage remains implementation-owned. |
| FIND-001-7 | Eligibility is applied before LIMIT with bounded existing SQL/snapshot ownership, tenant isolation, usable continuation and mixed-grant proof. No unbounded scan or hidden-row exposure. |
| FIND-001-9 | Actual admitted per-attempt provider is retained through existing engine/capture facts, separately from deployment/model, including fallback. No mutable post-call lookup. |
| FIND-001-10 | Dispatch authorization is tied to its actual routing snapshot through existing admission; early claim protection, pre-dispatch release, replay, uncertain-dispatch fencing and already admitted calls are preserved. Deterministic barrier proof avoids timing guesses. |
| FIND-001-12 | Changed-item rustdoc is batched with behavioral corrections; explicit bounded inventory covers changed bodies/tests without an unrelated documentation campaign or new checker. |
| FIND-001-13 | Existing three-language register/GET/hydrate owners prove the missing form/context, authored hash/JSON and introspection cases. It remains a proof gap, with no invented production accessor or storage-corruption claim. |

The focused P1–P10 commands target established owners and are explicitly planned, not passes. Exact Python/TypeScript preservation and route selectors are supplied. Added runtime regressions must record their actual selectors, and existing passing owners must gain the missing assertions before they establish closure. Required format/lint/type/schema/SQL checks depend on the actual write set. The packet reserves full journey/family aggregates and `mise run gate` for integrated change review; it does not substitute supporting unit tests for cross-boundary proof.

No concrete packet correction is required. The original thirteen stable finding IDs remain unchanged and open. This authority resolution changes the verdict recommendation from SPEC_REVISION_REQUIRED to FIX_REQUIRED; it does not make the candidate pass. Implement the packet under `$wyrd-implement`, then reassess the cumulative change against the original task, r6 and prior findings at a new immutable candidate.

Skipped tests/builds, new discovery and source edits; later concurrent changes and all implementation closure remain unverified.

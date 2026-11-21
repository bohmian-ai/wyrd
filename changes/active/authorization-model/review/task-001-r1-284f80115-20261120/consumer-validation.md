# Focused consumer validation

Candidate: `284f801159a391e92493c3072878fd644c9d173b`; remediation base: `3139c08cb0b7c0194583a66af3ab31bef28dc14f`. Independent source validation of proposals from `r1_closure`; no implementation changes or runtime tests. HEAD remained the candidate. CodeGraph preceded source searches. Read the applicable documentation and SDK instructions, approved specification revision 2, original task, and relevant testing/authorization authority.

## FIND-TASK-001-8: CONFIRMED incomplete closure

Classification: INCORRECT / mandatory documentation rule violation. REQ-002 permits local password sign-in without an IdP; REQ-008 includes invitations and explicit scoped assignment; REQ-010 requires updating superseded callers/fixtures together.

`docs/src/content/docs/operate/add-people.md:9` still says Wyrd stores no passwords and has no invite step. Its `Without an identity provider` section at line 182 still tells administrators to give people Service keys rather than explaining supported local User enrollment. A maintainer following this guide receives incorrect account/lifecycle guidance despite the revised system. The remediation changed other sections of this same page but left this contradiction. Correct these existing passages to distinguish optional provider sign-in and local User/invitation enrollment; do not invent a new onboarding subsystem.

`docs/src/content/docs/operate/grant-and-inspect-permissions.md:46`, `:60`, and `:78` introduce manually authored Python, Rust and TypeScript bind/access/unbind examples. `docs/AGENTS.md` under Examples explicitly requires `<CodeFromFile file="examples/...">` from a journey-executed file and says not to hand-write copyable snippets. This is an explicit repository obligation, not a subjective prose preference. Reuse a tested example for these current calls; do not add aliases or duplicate lifecycle logic. Content review and the affected documentation/example check provide closure.

## FIND-TASK-001-11: CONFIRMED stale language journey consumers

Classification: REGRESSION in fixtures/assertions, not a production authorization defect. REQ-006 says approval requires readable parent; REQ-008 explicitly says an approver on a Verifier needs separate Repository read when approving that Repository-owned execution; REQ-010 requires updating fixtures/callers with the revised contract.

Python `sdks/wyrd-sdk-python/tests/integration/test_deferred_evidence.py:108-111` bootstraps exactly the supplied role. Its approval test at `:152-159` creates an `approver`-only client and expects successful Repository execution approval and replay. TypeScript `sdks/wyrd-sdk-ts/wyrd/tests/integration/deferred-evidence.test.ts:134-135` likewise bootstraps exactly one role, and `:148-153` expects approver-only success. Neither test's setup seeds a replacement approver role or adds parent read.

The shared builtin `APPROVER` in `crates/shared/wyrd-runtime/src/builtin_roles.rs:85-89` contains only `cards:read` and `verifier:approve`, not `executions:read`. Fixture bootstrap delegates to the common bootstrap owner (`wyrd-testing/src/server.rs:3433-3439`); role binding uses `seeded_role_bindings` (`:6086-6095`, `:6121-6128`) with the builtin's tenant scope and adds no viewer. Production `verification/service/approvals.rs:81` calls `retained` for approval; `inspection.rs:562-570` independently requires `Permission::executions_read(parent)`. Therefore these consumers expect success for an operation the settled contract refuses.

Smallest correction: update each language's existing approval journey to first observe refusal without parent visibility, then assign the necessary Repository read through the public Principals contract and prove successful approval/replay with the same execution. Preserve actor attribution and viewer/editor decision refusal. Reuse the existing corrected Rust journey as behavior, without weakening production checks. Run each exact affected Python/TypeScript test under its repository-managed Postgres setup. This report establishes the source contradiction; it does not claim an observed runtime failure.

## FIND-TASK-001-12: CONFIRMED bounded CLI evidence gap

Classification: EVIDENCE_GAP. REQ-008 exposes common HTTP/CLI administration contracts. AC-003 explicitly includes granting through HTTP/CLI, and TASK-001 scenario 3 assigns that programmatic behavior. Root AGENTS.md Testing requires a journey for each shipped capability; unit tests cannot substitute.

`crates/wyrd/wyrd-cli/src/principal/access.rs:30-57` defines the new access show/grant/revoke, role and group command family; `:244-374` dispatches them to real Principals operations. Searches of CLI journeys and Rust SDK/test-harness journeys found no execution of `principal access` or `AccessCommand`. `principal_journey.rs` covers account creation using `--grant`, workload restriction, and principal revocation; these do not execute the distinct access command dispatch, its arguments, or inspection output. The recorded CLI family-read journey likewise proves read enforcement, not administration through this adapter.

Smallest correction: one focused real-server CLI journey that grants explicit family access, inspects it, removes it, and demonstrates the resulting data access/refusal; include a non-admin administration refusal without mutation. Exercise the related role/group adapter flow within that journey as needed to cover its shipped command family; do not require one new top-level scenario per verb or replay the full suite. Run the exact new selector with the setup owned by `test:cli:journey`. This is missing required proof, not a claim that dispatch is incorrect.

## FIND-TASK-001-13: CONFIRMED new journey fixture rule violation

Classification: VIOLATION. `TESTING.md:319-321` explicitly requires checked-in Card YAML and forbids test code building/editing it. This is a mandatory client-journey rule.

The new Rust SDK journey `sdks/wyrd-sdk-rust/tests/integration/principal_roles.rs:1328-1343` reads an existing Model YAML file, replaces its version text, writes a temporary YAML, copies its artifact and declares the generated file. The user outcome can use a checked-in second-release Card fixture instead, preserving the same public declaration/upload/deletion assertions. No production change or generic fixture builder is justified. Run the exact existing scoped-editor journey after replacing the generated input.

The new peer journey at `crates/wyrd/wyrd-testing/tests/bifrost/oracle/distributed.rs:2090-2099` also rewrites example YAML to author another Service name. It drives the public WyrdState SDK; if grouped under this finding, replace that input with checked-in second-parent fixtures too and rerun that exact peer journey. This is the same fixture-source correction, not a separate product defect. The older same-family version journey also synthesizes YAML, but expanding remediation to unrelated older test construction is unnecessary for closure of this newly introduced violation.

## Exact peer same-family variant: REJECTED as a separate blocker

The original FIND-TASK-001-10 closure asks for actual peer participation with family/exact readers over two same-tenant parents, visible identities/counts and hidden-parent exclusion. `distributed.rs:2037-2205` implements that shape: a family reader of `support-reply` and an exact reader of `other-reply`, each denied the other parent's rows/counts, with actual peer execution asserted through `peer_counts`. The fact that the exact reader uses a different Service does not violate that criterion. The existing exact-versus-family version behavior has separate source/assertion-backed proof at `principal_roles.rs:1051-1214`, including the same `code-review` family at versions 1.0.0 and 2.0.0. Requiring another same-family second-version peer composition would expand this settled remediation without a demonstrated coverage failure. Carry forward the exact-containment proof and assess the new peer test's actual execution evidence separately.

## Limits

Only source/content and existing assertions were inspected here; runtime execution belongs to the primary remediation reviewer. No code, architecture, benchmark, or TDD assessment was performed. The source-backed stale consumer expectations and explicit fixture/documentation rules are sufficient for these dispositions without claiming live reproduction.

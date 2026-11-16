# Maintainer remediation review

Result: **FAIL** — one bounded configuration-precedence proposal, MR2-1. The remaining assigned surfaces pass this maintainer review.

## Subject and scope

Repository: `/home/thorrester/Documents/GitHub/wyrd-factory-worktrees/code-review-workflow-github-verifier-pipelines`.
Prior candidate: `34fb45bb387582d2aedfee77fb4e09fa2753b80d`.
Candidate: `ea4460c3c97adb55395acd50f062658bc8d0d4cd` (HEAD checked).
Original base: `395274d58bf76a2075f245ca3a5c1bd044925027`.

Reviewed the incremental range, original task/spec revision 3, remediation including both approved human revisions, and prior maintainer/validated scope. Authority: root AGENTS, `agent_docs/index.md`, `rust.md`, `maintainer-style.md`, `changes/spec-driven-development.md`, and `architecture/wyrd-design.md` Agents/tools/workspaces. CodeGraph preceded code searches; its misses for the newly relocated child module were followed by direct source inspection. No source or Cargo changes, no checks run, and no report other than this assigned report written.

Carry forward r1's accepted single-engine/lane ownership, selected execution modes, unchanged accepted authority/evidence/budgets, Sandbox/Workspaces lifetimes, and live-log rationale. This round does not reopen those unchanged owners. The prior maintainer report's placement opinion is superseded by validated FIND-001-4 and the explicit runtime-only testing authority; the moved test is assessed at its server owner.

## Coverage and existing alternatives

| Changed surface | Full changed symbols, callers and proof inspected | Assessment |
|---|---|---|
| Typed file configuration | `VerificationConfig`, `VerificationLaneConfig`, defaults, `resolve_limits`, `set_file`, existing `set_scaled`, `set_millis`, `env_positive`; `WyrdServerConfig::load`/`apply_env_overrides`/validation; complete added file-resolution test and affected env test | New file fields belong at the existing server owner and avoid exposing internal lease/maintenance timings. Existing `LimitsConfig` uses typed file defaults and projection; Bifrost resource/Scribe/Forge settings use the same loader and `parse_optional_env`, with final validation. The candidate preserves that loader and projection but differs in when file positivity/unit overflow is checked: MR2-1. |
| Startup/resource projection | `boot/mod.rs` owners block and `engine_workers`; `HostedLimits::lanes_fit`, defaults and complete memory test; `RuntimeLimits` default test | Resolved `limits` still go to `HostedBlocks::start`, and resolved `runtime` still goes through the existing runtime builder/drain clipping. No second resource owner. Checked addition fixes the shared predicate at its source. Exact approved defaults, overflow, per-worker and total-memory assertions remain clear and local. |
| Relocated capacity workflow | Complete `verification_runtime/isolated_capacity.rs`, parent registration/native client, old-to-new move, removed SDK registration, existing server YAML/pull/hydration precedents around parent lines 5381–5452 and 7851–7910 | `CapacityDesk` owns only this story's retained server/client/mock/bundle/state. Existing `WyrdTestServer::start_bound`, `native_client`, public Gateway administration/CredentialWriter, Cards declaration/pull and WyrdState hydration cover the complete workflow. The child module composes them without a reusable parallel harness, SDK dependency, target or fixture format. |
| Preserved capacity assertions | Entire relocated test body, `hold`, `verify`, `submit`, `call_subjects`, `passed`, `logged`, `settled`; TESTING public-surface checklist and server test-controls contract | Actual wire calls remain. Four distinct saturations, queued pending/defer/reclaim, reverse no-borrowing, subject accounting, cancellation/release and direct execution ceiling assertions survive. Runtime-only holds/log polling now live in the explicitly permitted server target. Local mock response/result helpers were formerly supplied by SDK support and are minimum local replacements, not competing owners. |
| Gateway wait/budget | Complete `ServerWyrdGatewayCaller` fields/methods/call, new full `lane_wait_spends_the_agent_call_budget`, adjacent compatible-adapter test, unchanged invocation owner boundary | A timeout around the existing owned-semaphore wait plus remaining duration is the small source correction; no deadline service/scheduler/provider owner is introduced. `GatewayInvocation` retains governance/settlement. Tests assert no dispatch, remaining provider deadline, accounting and recovery, not merely helper output. System-resilience specialist owns deeper failure/cleanup acceptance. |
| Recovery ordering | Complete `speculative_recovery_cannot_resurrect_content`, `reached`, existing related disposition test and HeldUpstream use | Reuses the existing response gate and public invocation milestone. Admitted work is succeeded before release; excluded work reaches its publish-result second turn and is still Started. Takeover, no-replay counts, mounted content, original attempt and retained-publication assertions remain unchanged. No production recovery owner was added or altered. |

## Proposed finding

### MR2-1 — Validate the effective setting after source precedence

Classification: **INCORRECT**. Location: `crates/wyrd/wyrd-server/src/config.rs:1854–1901,2079–2083` (`VerificationConfig::resolve_limits` / `set_file`).

Approved revision 1 requires “defaults, configuration-file values, environment overrides, then startup validation,” standard environment-over-file precedence, and validation of “the final merged values.” Nearby Bifrost/Forge settings implement this ordering through `parse_optional_env` (`config.rs:3177–3238,3915–3929`) before validating or projecting the effective values. For example, `forge.target_file_size_bytes` is overridden at line 3201 and refused if the resulting value is zero by `validate` at line 3513.

The new verification path validates each file value before its environment override can be considered. A parseable file `[verification]\nqueued_executions = 0` together with `WYRD_VERIFICATION_QUEUED_EXECUTIONS=9` returns `ConfigError::Invalid` from `set_file` at line 2081, never reaching the override at lines 1930–1934. The effective operator setting should be 9 under the approved precedence. The same cause affects all newly exposed capacities/timeout; the pool also multiplies the file MiB value at lines 1892–1900 before checking its valid overriding environment value. An oversized but parseable file pool value therefore similarly prevents a valid environment pool override.

Concrete consequence and maintenance cost: an operator cannot correct a stale shared TOML value through the documented environment layer, and verification settings now have a separate source-precedence rule from the normal server/Bifrost owner they were explicitly required to follow. Startup fails despite valid final settings. This is a bounded violation of the new explicit merge contract, not a request to accept an invalid active limit or to harden an exotic runtime count.

The minimum correction is to select the effective raw setting using the existing source-precedence mechanism before applying positivity and unit conversion, and keep the existing cross-field/HostedLimits predicates on the resulting resolved values. Reuse the existing server override pattern, preserving all environment names and existing named refusal paths. No loader, resource owner, public option, or new validation abstraction is needed. Merely deleting file validation is insufficient because an unoverridden zero/overflow must still refuse. Merely adding downstream guards cannot make an override reachable after the early return.

Focused closure proof: extend the existing configuration test with a zero parseable file capacity and a valid override, and an overflowing parseable file pool value and a valid pool override; both must resolve to the override. Preserve existing refusal assertions with the overrides absent, plus invalid environment refusal and cross-field checks. The existing test currently proves only valid-file/valid-env precedence (`config.rs:4536–4556`) and does not exercise shadowed invalid file values. No runtime journey or new harness is required for this correction.

## Verification limits

The remediation packet records 4/4 focused config/default units, 2/2 gateway units, the orchestration pair, relocated journey, fmt/lints/deps/docs checks green. I inspected their source assertions; this review did not rerun Cargo, per the orchestrator-owned sequential-check rule. No additional runtime checks are requested for the accepted maintenance outcomes. MR2-1 is source-proven and requires independent validation before becoming a final finding.

Skipped unrelated refactors and unchanged-owner rediscovery. Risk: the proposed merge-precedence correction must retain rejection of invalid effective values; acceptance of a shadowed value does not authorize accepting it when active.

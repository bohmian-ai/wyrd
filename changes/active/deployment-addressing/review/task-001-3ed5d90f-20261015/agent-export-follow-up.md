> Superseded by the owner's subsequent instruction to include this work in the
> same remediation as **FIND-001-17**. See
> [TASK-001-R3-portable-media-remediation.md](TASK-001-R3-portable-media-remediation.md).
> The analysis below records the earlier separate-tracking recommendation only.

# Separate follow-up: faithful runtime Agent export

**Recommendation: track separately from deployment-addressing remediation.**
This records the owner's question and independently validated scope decision;
it is not a blocking FIND ID, approved new specification or ready implementation
task. No fix has been made by review.

At candidate `3ed5d90ff53a2ed082decd66bf7aab168a5afdcb`, loading and saving a
runtime Agent can change its authored spec/hash. The issue is broader than the
reported concurrency cap:

- `run_config_from_agent_run_config_spec` in
  `crates/skald/skald-agent/src/agent.rs:844` resolves omitted values using
  runtime defaults; the reverse conversion at833 emits concrete values,
  including omitted `max_iterations` becoming10 and cap becomingSome(8).
- `Agent::to_spec` at784 also emits an empty `verified_by`, losing nonempty
  declared verification bindings.
- `to_card`, `to_wire`, YAML/save and Python runtime projections share this
  conversion boundary; hydrated registered Card holders separately retain
  their stored authored spec.

The original exact authored save/load/hash requirement (REQ-008/AC-004) covers
the two Prompt forms. The Agent obligations preserve its route, pinned Prompt
identity, invocation isolation and stored Card/hash during overrides. Those
paths now pass. These generic conversion bodies are unchanged in the cumulative
executable diff, and the required loader test executes the actual restored
Agent rather than saving/re-registering it. This is therefore recorded as
separate lifecycle debt, not excluded merely because R2 omitted a criterion.
The initial whole-spec test assertion was narrowed with the actual discrepancy
recorded; it did not remove required route/identity/execution proof.

A future bounded repair should cover faithful Agent re-emission at the existing
Agent/spec conversion owner: preserve omitted versus explicit run configuration
and verification bindings while runtime defaults still behave normally, and
reflect intentional edits correctly. Prove load → save/reload/spec/hash
preservation plus explicit configuration/binding changes through existing Rust
and Python owners. Do not fix only cap8, change global execution defaults,
set every cap toNone, weaken hashing or add another loader.

Until repaired, do not describe runtime `to_card`/`to_yaml` as a lossless export
of the registered Agent. Use the retained registered Card/bundle when exact
authored fidelity is required. The full source/scope analysis is in
`findings-validation.md`, section “Owner-raised Agent export question.”

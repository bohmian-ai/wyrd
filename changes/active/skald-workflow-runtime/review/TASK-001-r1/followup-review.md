# Focused follow-up review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd`
- Base: `a51af030b6039eea4b2914f3ebf2c31925d08721`
- Candidate: `eb22b03f2bb766886d839bda23aafbd4ba130ab3`
- Approved specification: `changes/active/skald-workflow-runtime/spec.md`, revision 9
- Original task: `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md`

The candidate remained unchanged while this follow-up was performed. This pass
investigated only the three conflicts assigned by the orchestrator and does not
vote on the discovery findings.

## 1. External-gateway header and secret containment

### Paths inspected

- `changes/active/skald-workflow-runtime/spec.md:1244-1313,1912-1917,1961-1972,2028-2041`
- `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:68-164,341-356`
- `architecture/wyrd-security-posture.md:15-26`
- `crates/wyrd-spec/src/card/workflow.rs:513-608`
- `crates/skald/skald-workflow/src/route.rs:95-187,258-319,452-491`
- `crates/skald/skald-providers/src/clients/external.rs:21-149`
- `crates/skald/skald-providers/src/clients/mod.rs:30-76,132-158`
- `crates/skald/skald-providers/src/error.rs:10-87,100-149`
- `crates/skald/skald-runtime/src/dispatch.rs:14-44`
- `crates/skald/skald-runtime/src/error.rs:11-75`
- `crates/skald/skald-agent/src/error.rs:16-52,145-175`
- `crates/skald/skald-agent/src/loop_runtime.rs:330-356,604-650`
- `crates/skald/skald-workflow/src/attempt.rs:128-193`
- `crates/skald/skald-observer/src/otel.rs:261-268`

### Resolution of NET-001 versus the invariant PASS

The invariant PASS is too broad. `ExternalGatewayBindings::insert` validates a
bare origin and each secret value but never classifies the secret header name.
`merged_headers` then inserts every binding header into the request
`HeaderMap`. Unlike Card-authored headers, this path does not apply the
transport/routing restrictions in `is_forbidden_route_header`.

The path is reachable through the public programmatic binding seam expressly
allowed by the specification. An inserted `Host` survives request construction:
the HTTP client only synthesizes `Host` when the request does not already have
one. The screened URL still selects the connection and TLS name, but an HTTP/1
shared reverse proxy can route the request by the caller-supplied `Host`; bound
credentials in the same request can therefore reach a different virtual
backend. Hop-by-hop, framing, forwarding, proxy, and Wyrd-internal headers are
likewise currently admitted.

The authority requires forbidden-header checks before dispatch and exact
origin/header binding behavior. Credential headers such as `authorization`,
`*-api-key`, and `*-token` must remain legal only in the secret binding, so the
whole Card-authored classifier cannot be applied unchanged. The smallest
correction belongs in `ExternalGatewayBindings::insert`: reject the
transport/routing/internal subset while preserving the credential-bearing
subset. NET-001 is therefore a reachable task-local defect, not speculative
hardening.

### Resolution of NET-002 versus the invariant PASS

NET-002 is also reachable, with one important narrowing. The producer is an
external gateway that has already received the bound credential and returns a
non-success body containing it. `send_bytes_with_retry` reads that bounded body
and constructs `ProviderError::Status`; the derived `Debug` representation
contains the `body` field. `ExternalGatewayClient::send` exposes that error
unchanged through its public API.

The current Agent and Workflow consumers do not themselves log or observe the
body. `ProviderError::Display` omits it; `SkaldRuntimeError` and `AgentError`
use that display; terminal journal and observer events call
`error.to_string()`; and `project_agent_error` emits only the stable provider
code. Thus the canary does not reach those current message/log sinks through
the inspected path. The discovery report's claim should not be read as proof
that an existing runtime log statement formats this value with `Debug`.

That narrowing does not remove the violation. REQ-042 and the repository
security posture prohibit secret values in an error, not merely in its
`Display`, and the public error value retains the reflected credential where
ordinary debug inspection can expose it. Sanitization must occur at the
`ExternalGatewayClient` boundary before the shared native-provider error path
stores the body; changing shared native-provider behavior would exceed the
approved scope. NET-002 is therefore a reachable secret-containment defect.

## 2. Ownership of newly added PyO3 Workflow behavior

### Paths inspected

- `AGENTS.md:99-114,286-327`
- `architecture/references/languages/pyo3-boundaries.md:1-32,156-190`
- `changes/active/skald-workflow-runtime/tasks/TASK-001-explicit-local-runtime.md:15-40,380-415`
- `crates/skald/skald-workflow/src/python.rs` in the complete base-to-candidate diff, especially new conversion helpers, `py_with_inputs`, `py_with_step_inputs`, `py_with_outputs`, `py_validate`, and `PyWorkflowRun`
- `sdks/wyrd-sdk-python/src/lib.rs:40-50`

### Resolution of MNT-001 versus the standards PASS

MNT-001 follows the controlling authority. Existing owner-crate `python`
features may remain as migration state, but both `AGENTS.md` and the PyO3
reference assign new or materially relocated wrappers to
`sdks/wyrd-sdk-python/src`. The original task repeats that ownership explicitly
and says to leave untouched legacy Python migration state untouched.

The candidate materially adds Python-only conversion, three authoring methods,
validation, changed run conversion, and the new `PyWorkflowRun` pyclass in
`crates/skald/skald-workflow/src/python.rs`. The SDK remains an aggregator that
only calls `skald_workflow::python_register`. These are not retained untouched
wrappers; they are the task's new Python contract. A green
`check:pyo3-scope` establishes feature containment, not compliance with the
stricter placement rule. The standards PASS is therefore incorrect for these
changed symbols, and MNT-001 is an authority-backed ownership violation.

## 3. Packet-local Rust seams

### Paths inspected

- `changes/active/skald-workflow-runtime/spec.md:965-1033,1244-1263,1961-1988`
- Original task at base commit, `TASK-001-explicit-local-runtime.md:49-164`
- Candidate implementation record at `TASK-001-explicit-local-runtime.md:468-484`
- `crates/skald/skald-workflow/src/route.rs:95-187`
- `crates/skald/skald-workflow/src/lib.rs:45-52`
- `crates/skald/skald-providers/src/error.rs:10-87,151-174`
- `crates/skald/skald-providers/src/lib.rs:18-24`
- All `ProviderError::RemoteProblem` constructors and matches under
  `crates/skald`

### Resolution of BEH-003

The `secret_headers` type is a true public-contract deviation. The approved
specification calls the shape fixed, and the original task identifies the
packet stubs as cross-task/public contract shapes. The candidate exposes
`HashMap<HeaderName, SecretString>` instead of the specified
`BTreeMap<HeaderName, SecretString>`. Its implementation-record deviation is
lower authority and cannot amend that seam.

The reason for the deviation is real: the installed `http::HeaderName` derives
`Eq`, `PartialEq`, and `Hash`, but not `Ord`, so the exact approved `BTreeMap`
key type is not implementable directly. Choosing `HashMap`, `HeaderMap`, a
string key, or a local ordered newtype changes the public contract and requires
an authority decision. This is not permissible private mechanics; it requires
specification revision before implementation can close the seam.

No `FromIterator` implementation for `ExternalGatewayBindings` exists in the
candidate. Its derived `Default` is a public trait implementation, but it
produces the same valid empty collection as the approved `new()` and is used
only to implement that constructor. The packet stub does not enumerate derive
traits, so `Default` is not independently a material contract deviation.

### Resolution of BEH-004

`ProviderError::RemoteProblem(Box<RemoteProblem>)` is also a true public seam
deviation. `ProviderError` and `RemoteProblem` are publicly re-exported, and
Workflow code constructs and matches the tuple variant, so boxing is observable
to downstream constructors and exhaustive matches. Runtime projection of the
fields is equivalent and safe, but equivalence does not satisfy the fixed
struct-variant contract.

The implementation record says the box was introduced to keep the mandatory
`clippy::result_large_err` lane green. That records a genuine conflict; it does
not authorize a different public variant. The approved authority must choose
the public representation or another compliant size strategy. BEH-004 is
therefore not permissible private mechanics and requires correction under
revised authority.

## New findings

None. The inspected evidence resolves or narrows the assigned discovery claims
without revealing a separate violated obligation.

## Result

**RESOLVED**

- NET-001 and NET-002 are reachable; NET-002 is narrowed to retained
  error/debug exposure because the inspected journal, observer, and Workflow
  projections use redacted display/code data.
- MNT-001 is required by the explicit new-PyO3 placement authority.
- BEH-003 and BEH-004 are public contract deviations. The impossible ordered
  key and lint-driven boxing conflicts require approved authority changes;
  `ExternalGatewayBindings::Default` is not independently material, and no
  `FromIterator` implementation exists.

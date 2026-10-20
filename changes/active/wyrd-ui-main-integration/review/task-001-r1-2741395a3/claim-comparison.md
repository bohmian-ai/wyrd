# Discovery claim comparison

All five required general discovery reports and both sensitive-domain reports
are present. This compares claims; it does not independently validate them.

| Cause / obligation | Discovery source IDs | Validation question |
|---|---|---|
| Native auth forwarder removes tenant-session authority from the new public metadata/logout endpoints | BEH-R1-001 | Verify public native caller→hook→forwarder→Rust enumeration/logout and preserve existing trust/header handling; Node internal calls are distinct. |
| Metadata HTTP failure or successful enumeration without selected tenant falls back to prior selected metadata | BEH-R1-002, INV-R1-001, SYS-R1-001, SEC-1, REP-002 | Consolidate one Node metadata consumer cause; require current accepted view and preserve cookie/error behavior, without changing Rust session state. |
| Browser callback adapter converts backend unavailable into ordinary sign-in refusal | SEC-2 | Unique claim; verify post-consume errors, reachable follow-on directory success and proper response boundary. |
| Container test EXIT trap aborts before common cleanup on nonzero status | SYS-R1-002 | Verify inherited errexit, original status preservation and existing shared cleanup owner; source reviewer ran only a nonmutating shell reproduction. |
| New nginx-only test fixture environment variable despite explicit prohibition | INV-R1-002 | Verify test-only producer/consumer and whether established fixture transport covers the same result without a new setting. |
| Selected tenant logout can revoke relabeled cookie's different tenant session | DATA-001 | Verify possessed-cookie provenance, selected route/label/value binding and minimal lifecycle mutation correction, preserving idempotent logout and key/sibling sessions. |
| Mandatory new-item/durable-operation rustdoc | REP-001 versus maintainer scope PASS | Fresh focused followup required to resolve exact source/rule conflict and narrow any unsupported pieces. |

Follow-up decision: a fresh followup-rev is assigned only the material rustdoc
coverage conflict. No extra origin validator is requested: independent discovery
traced pinned SvelteKit's Origin enforcement plus form-only action dispatch and
found no reachable production bypass. A rejected suspicion is not a finding.
Unique source claims still require independent Ponytail validation; agreement is
not proof. General scope PASS reports do not erase independently source-backed
behavior claims.

Prior FIND-TASK-001-1: discovery agrees the shared fresh/cache/lookup eligibility
cause is corrected in source; signed source-free runtime evidence remains
separately missing. Prior FIND-TASK-001-2: missing executable nginx provider path
is added, but its real run is explicitly unverified. Preserve prior IDs and
classify execution evidence honestly, not as proof of a runtime bug. REQ-033 is
the approved human-directed architecture and does not regain the superseded
confidential-Node/encrypted-cookie prerequisite.

Remaining acceptance limits: source-free signed native runtimes/digests, native
Linux, official Debian amd64/arm64 and Rocky amd64 image runs, nginx provider/
restart run, and human inspection. Do not invent new configuration/infrastructure
or repeat broad matrices in a bounded remediation task.

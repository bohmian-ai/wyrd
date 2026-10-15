# Default authored Card API version

- Change ID: `SPEC-authoring-api-version-default`, approved revision 2.
- Completed: 2026-10-14.
- Reviewed target: not supplied; no final-change-review PASS is claimed.
- Completion target: `06a89286943a64c83eb588562b308c3d35663d66`, including `a18c6768e`.
- Delivery reference: not supplied.
- Completion authority: human-approved closeout extending the explicit completion override on 2026-10-14.

## Intent and behavior

Card authors may omit `apiVersion` in YAML or JSON. Canonical Card
deserialization and the shared loader's raw authored-envelope deserializer
default that omission to `wyrd/v1`. Serialization and registration submission
construction include the explicit value. CLI, SDK and per-kind consumers
inherit the behavior from those shared seams.

## Lasting decisions and constraints

Omission permanently means `wyrd/v1`, not a configured or latest version.
The two deserializers reuse the existing API-version default; no
consumer-specific defaults or new error contracts are introduced.
`CardSubmission` and raw HTTP/gRPC registration remain strict and require an
explicit version. Missing kind, metadata or spec remains invalid, and existing
unsupported-version validation is unchanged.

Revision 1's consumer-by-consumer default and duplicated proof were superseded
by approved revision 2, which places behavior at the two shared parsing seams.
Completion changes no behavioral decision; the owner approves retirement
without a new final review.

## Acceptance evidence and limits

| Obligation | Recorded evidence |
|---|---|
| REQ-001/003, AC-001: canonical omission and explicit serialization | Existing `api_version_defaults_to_v1` regression passed. |
| REQ-002, AC-002: shared-loader omission | Multi-document loader test passed; obsolete missing-version refusal was removed while other-field diagnostics remained covered. |
| INV-002/003/004, AC-003/004: strict wire and unchanged validation | Implementation record reports unchanged submission/server/generated contracts, 908 specification tests and 762 shared-family tests passing. |
| REQ-004, AC-005: authored-file documentation | Current design documents optional authored apiVersion and explicit registration wire. |
| Owning checks | Recorded formatting, lints and whitespace checks passed. The stale Forge documentation-parity test was retired after its referenced page was removed; adjacent emitted-family coverage remained. |

These are retained implementation results, not a new independent final-review
verdict. Closeout reruns only record/link and diff validation; it claims no
release or new production qualification.

## Current authority, owners and tests

- [Card authoring authority](../../../architecture/wyrd-design.md) and [doctrine](../../../architecture/wyrd-doctrine.mdx).
- [Canonical envelope](../../../crates/wyrd-spec/src/envelope.rs), [shared loader and tests](../../../crates/shared/wyrd-loader/src/parse.rs), and [Card regression tests](../../../crates/wyrd-spec/src/card/prompt/mod.rs).
- [Strict registration contract](../../../crates/wyrd-spec/src/registry/mod.rs).

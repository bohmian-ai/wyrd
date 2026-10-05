# Operator verifier fields

- Change: `operator-verifier-fields`
- Specification: `SPEC-operator-verifier-fields`, approved revision 1
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Operator delivery templates now expose bounded, aggregate fields derived from the bound Verifier kind. Drift bindings expose drifted and scored feature counts. Eval bindings expose passed, failed, and total task counts plus the defined pass-rate projection. The same frozen context contract serves Notify, HTTP, and future delivery actions.

Registration validates referenced fields against the exact bound Verifier kind and rejects unknown or cross-kind fields before runtime delivery. Persisted and wire representations carry the fields through generated schemas and SDK types. Retry rendering uses the same frozen values, preserving deterministic delivery behavior without embedding feature-level or task-level payloads.

Acceptance closed through Drift notification rendering, Eval HTTP rendering, registration refusal cases, frozen retry evidence, schema generation, SDK projection, and operator journey coverage. The aggregate-only boundary and existing retry semantics remain unchanged; no material deviation remains.

Current owners and evidence:

- [Wyrd design](../../../architecture/wyrd-design.md) and [protocol doctrine](../../../architecture/wyrd-doctrine.mdx).
- [Operator contract and field resolution](../../../crates/wyrd-spec/src/card/operator.rs), [Verifier activation](../../../crates/wyrd/wyrd-server/src/components/cards/resolve.rs), and [verification engines](../../../crates/wyrd/wyrd-server/src/verification/engines.rs).
- [Operator journey coverage](../../../crates/wyrd/wyrd-cli/tests/operator_journey.rs).


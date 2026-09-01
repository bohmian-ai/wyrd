# Wyrd Changes

Wyrd keeps a complete tracked packet while a change is active and one compact
record after it is complete. The normative workflow is
[`spec-driven-development.md`](../architecture/references/languages/spec-driven-development.md).

```text
changes/
|-- active/
|   `-- <slug>/
|       |-- spec.md
|       `-- tasks/
`-- completed/
    `-- <year>/
        `-- <slug>.md
```

`$wyrd-spec` creates and revises the active specification. `$wyrd-plan` creates
implementation and remediation tasks. Implementors append focused execution
evidence to their tasks. Final `$wyrd-change-review` keeps the complete packet
available while it performs read-only verification. Task files carry their
execution evidence; the final reviewer builds the integrated evidence matrix.

An `APPROVE` verdict automatically invokes `$wyrd-complete`. Completion writes
the compact record, verifies that lasting behavior is already represented by
the owning architecture or product documentation, and removes the active
packet. The completed record is historical context; current architecture,
contracts, code, and tests remain authoritative.

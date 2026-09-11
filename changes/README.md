# Wyrd Changes

Wyrd keeps a complete tracked packet while a change is active and one compact
record after it is complete. The normative workflow is
[`spec-driven-development.md`](../architecture/references/languages/spec-driven-development.md).

```text
changes/
|-- active/
|   `-- <slug>/
|       |-- spec.md
|       |-- tasks/
|       `-- review/
|           `-- <review-name>/
|               |-- verdict.md
|               `-- TASK-001-R1-<name>.md
`-- completed/
    `-- <year>/
        `-- <slug>.md
```

`$wyrd-spec` creates and revises the active specification. `$wyrd-plan` creates
small outcome-complete implementation tasks. Implementors append focused
acceptance evidence. `$wyrd-task-review` writes its verdict and, when needed, a
self-contained remediation task under the named review directory for a fresh
`$wyrd-implement` agent. Final `$wyrd-change-review` keeps the complete packet
available while it performs read-only integrated acceptance review.

A final `PASS` verdict automatically invokes `$wyrd-complete`. Completion writes
the compact record, verifies that lasting behavior is already represented by
the owning architecture or product documentation, and removes the active
packet. The completed record is historical context; current architecture,
contracts, code, and tests remain authoritative.

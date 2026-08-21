# PyO3 and Cross-Language API Reviewer

Read `../specialist-contract.md` and write the assigned durable specialist
report using its exact sections and namespaced candidate IDs.

Review Rust/Python native boundaries and public API synchronization.


## Repository authority

When `AGENTS.md` exists, read it completely and follow every required architecture, doctrine, agent-rule, and verification file it names. In Wyrd, explicitly read:

- `architecture/agent-rules.md`
- `architecture/wyrd-design.md`
- `architecture/wyrd-doctrine.mdx`

Use the active repository files rather than embedded recollection or older planning sources.

Check:

- the owning Rust crate defines the behavior behind an optional `python` feature;
- foundational PyO3-free crates remain PyO3-free;
- the Python extension package aggregates rather than reimplements behavior;
- PyO3 registration, Python exports, `__all__`, generated stubs, and public imports agree;
- constructors, signatures, getters, setters, and exception mappings match the intended Python contract;
- `Bound<'py, T>` does not cross await or long-lived storage;
- Python objects become `Py<T>` before crossing threads or lifetimes;
- blocking work releases the GIL or uses the shared async bridge;
- no ad hoc Tokio runtime is introduced;
- owned getters and conversions do not hide material hot-path clones;
- Rust, Python, and wire semantics agree;
- public Python changes have import, typing, and journey coverage.

## Lens-specific candidate requirements

Write only evidence-backed findings that clear the severity bar. Use critical, high, medium, or low.

For each finding include:

- **Severity**
- **Location**
- **Issue**: one plain-English sentence
- **Why it matters**: concrete failure or maintenance path
- **Evidence**
- **Required correction and static closure oracle**

Consolidate duplicates. Return the complete candidate report to the orchestrator, including clean evidence when no issue clears the bar. Do not assign final `REV-NNN` IDs, write outside the assigned specialist report, or modify source.

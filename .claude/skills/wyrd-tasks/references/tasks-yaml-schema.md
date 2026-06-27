# tasks.yaml Schema

The machine-readable commit graph. Humans read `plan.md` and `tasks/NN-*.md`;
`wyrd-implement` reads this. The plan stage emits the skeleton (id + depends_on);
`wyrd-tasks` fills the rest.

## Shape

```yaml
feature: identity                       # matches .dev/plan/<feature>/
base_branch: main
created_from: plan.md                   # provenance

tasks:
  - id: "01"
    title: spec-contracts-and-errors
    file: tasks/01-spec-contracts-and-errors.md
    crates: [wyrd-spec]
    depends_on: []
    seams: []                           # symbols this commit reuses/avoids (names only)
    model: sonnet                       # sonnet | opus  (per-task routing)
    status: pending                     # pending | in_progress | done | blocked
    verify:                             # exact gate commands (mirror the task's Verify block)
      - "mise run codegen:check"

  - id: "04"
    title: refresh-grant-and-rotation
    file: tasks/04-refresh-grant-and-rotation.md
    crates: [wyrd-server, wyrd-sql]
    depends_on: ["01"]
    seams: [token_hash, consume_active_refresh, insert_refresh_token, issue_for_subject, revoke_refresh_family]
    model: opus                         # concurrency + race correctness -> opus
    status: pending
    verify:
      - "cargo test -p wyrd-sql --all-features"
      - "cargo test -p wyrd-server auth::refresh --all-features"
```

## Field rules

- `id` — string, stable, matches the `NN` in the filename and `plan.md`'s table.
- `depends_on` — list of `id`s. Must match `plan.md`'s DAG exactly. No cycles.
  This is the only input `wyrd-implement` uses to compute execution waves.
- `seams` — symbol names only. Never `file.rs:line`. Used by the executor to
  pre-hydrate CodeGraph context and by the arch gate to verify invariants.
- `model` — `sonnet` default. Set `opus` for concurrency, trait/object-safety
  design, the PyO3 boundary, migrations, or novel algorithms. The orchestrator
  routes the executor on this field.
- `status` — `wyrd-implement` updates this as waves complete. Resumable: a
  re-run skips `done` nodes.
- `verify` — exact commands, copied from the task's `## Verify` block, so the
  executor and the test stage run the same gate.

## Aggregate gate (feature-level, optional)

```yaml
final_gate:
  - "mise run test:unit"
  - "mise run lints"
  - "mise run pre-pr"
```

Run by the test stage on the assembled branch after all tasks are `done`.

## Validation

`wyrd-tasks` must confirm before hand-off: unique ids; acyclic `depends_on`;
edges identical to `plan.md`; every node has `crates`, `seams`, `model`,
`status: pending`, and ≥1 `verify` command.

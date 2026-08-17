# Execution manifest contract

The manifest is scheduling and proof identity, never a second product spec.
Normative requirements, decisions, contracts, lifecycle semantics, acceptance
text, and commands live in `plan.md` and task packets. The manifest references
their IDs and binds their files.

```json
{
  "schema_version": 3,
  "plan_id": "plan-example",
  "repository": {"origin": "https://github.com/bohmian-ai/wyrd.git", "revision": "<40 lowercase hex>"},
  "artifacts": {"plan.md": "sha256:<64 hex>", "tasks/01-example.md": "sha256:<64 hex>"},
  "delegation": {"model": "gpt-5.6-sol", "reasoning_effort": "low", "cargo_execution_lanes": 1},
  "requirements": {"R1": ["T1"]},
  "decisions": ["D1"],
  "locks": [
    {"category": "semantic", "key": "owner", "owners": ["T1"]},
    {"category": "contracts", "key": "Owner::run", "owners": ["T1"]}
  ],
  "tasks": [{
    "id": "T1", "packet": "tasks/01-example.md", "depends_on": [],
    "write_set": [{"path": "src/lib.rs", "kind": "existing", "coverage": ["owner-lib"], "symbols": [{"name": "Owner::run", "kind": "new"}]}],
    "requirements": ["R1"], "decisions": ["D1"], "acceptance": ["AC1"],
    "lifecycle": {
      "construction": {"applicable": true, "behavior": "task section reference", "proof": "AC1"},
      "shutdown": {"applicable": false, "behavior": "task section explains no owned resources", "proof": "AC1"},
      "cancellation": {"applicable": false, "behavior": "task section explains synchronous boundary", "proof": "AC1"},
      "concurrency": {"applicable": true, "behavior": "task section reference", "proof": "AC2"},
      "recovery": {"applicable": false, "behavior": "task section explains no partial progress", "proof": "AC1"}
    },
    "proofs": [{
      "acceptance": "AC1", "coverage": ["owner-lib"], "package": "owner", "target": "lib",
      "features": [], "selector": "tests::owner_runs", "test_kind": "new",
      "test_path": "src/lib.rs", "test_name": "tests::owner_runs",
      "expected_selected_count": 1, "setup": "none",
      "lane": "cargo-1", "expected_result": "one named test passes",
      "preflight_command": "mise exec -- cargo test --locked -p owner --lib --no-run",
      "acceptance_command": "mise exec -- cargo test --locked -p owner --lib tests::owner_runs -- --exact",
      "preflight": {"path": "evidence/T1-AC1.json", "digest": "sha256:<64 hex>"}
    }]
  }]
}
```

Lock categories are exactly `semantic`, `contracts`, `manifests`, `migrations`,
`generated`, `fixtures`, or `config`. A lock key has one owner unless all owners
are dependency-ordered. Source paths and symbols declare `existing` or `new`.
Existing identities are proven from the accepted commit; new file parents must
exist there. Preflight evidence is captured JSON containing `exit_code`,
`selected_count`, `command`, and non-empty `output`; its digest is verified.
Evidence JSON also carries exact repository `origin`, `revision`, `package`, and
`target`. Existing tests require an executed positive selected count. New tests instead
require a source-bound planned path/name, a compilable existing package/target,
a successful `--no-run` preflight, and a positive expected post-implementation
selected count; they never fabricate a pre-implementation positive count.
Every written path declares coverage identities consumed by one or more proofs.
Multiple proofs may cover one AC, but every AC has proof and every affected
package/target, manifest, mise task, binary registration, construction path,
and entrypoint is covered. `acceptance_command` always selects the named test
and preserves required wrappers, setup, features, and `--ignored` flags.
The only script/shell acceptance wrapper is the audited Postgres form:
`scripts/postgres/with-test-postgres.sh -- bash -lc 'mise run db:migrate:inner && mise exec -- cargo test ... <selector> ...'`.
Arbitrary scripts, `bash -lc` payloads, or wrapper forms are invalid.

`rehearsal.json` is not listed under `artifacts`. It binds a PASS/FAIL/STALE
verdict to the manifest digest, artifact digests, origin, and revision.

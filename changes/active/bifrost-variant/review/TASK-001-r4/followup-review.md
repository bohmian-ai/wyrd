# TASK-001 r4 focused follow-up and conflict resolution

## Immutable subject

- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `a6429060fb011aafa4335f2f736c70adab231739`
- Candidate tree: `64177d67141993ec18e63b43dc227dbc31d70950`
- Approved specification: `changes/active/bifrost-variant/spec.md`, revision 11
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`

The candidate object was available and resolved to the supplied tree before
and after this pass. Candidate source was read through explicit Git objects;
the later checkout tip was not treated as part of the subject. CodeGraph was
used first for the Variant, admission, Oracle, and permission paths.

## Conflict resolutions

### 1. Repository task-review policy commits are unrelated candidate scope

**Resolution:** `BVR-R4-BEH-001` and `STD-R4-001` describe the same confirmed
scope defect.

Commits `a6f1326f6` and `a6429060f` change only the mirrored
`wyrd-task-review/SKILL.md` files. They make a reuse reviewer mandatory, require
a fresh root-cause follow-up on repeat reviews, and expand validation and
remediation rules for every future task. No TASK-001 runtime, generated
contract, test, or evidence command consumes those rules. TASK-001's expected
write set names Variant contracts, Bifrost runtime owners, SDK/query surfaces,
tests, generated artifacts, docs, manifests, and pinned forks; it does not own
repository-wide review topology. Review artifacts produced under the process
belong in `changes/active/.../review/`; changing the process definition itself
is not required evidence. The two skill commits must therefore be separated
from this candidate.

### 2. Null verification Struct children do leak placeholders

**Resolution:** `INV-R4-001` is confirmed; the Oracle/DataFusion PASS covered a
different case.

Producer-to-result trace:

1. `verification/results.rs:700-742` builds an absent `drift_report` with
   children `""`, encoded JSON null, `""`, and an absent `eval_summary` with
   numeric zero children, then marks only the parent Struct null.
2. Arrow permits those values under the null parent. Parquet returns the same
   child buffers without intersecting their validity with the parent.
3. DataFusion 55 `GetFieldFunc::extract_single_field` returns
   `StructArray::column_by_name` directly. Its return field becomes nullable,
   but execution does not apply the parent validity.
4. `mask_placeholders` repairs only a Variant cell whose two child byte buffers
   are empty. The producer's encoded JSON-null Variant is a valid, non-empty
   Variant and the primitive String/numeric children never enter that helper.
5. The Oracle unit test filters primitive access with `WHERE s IS NOT NULL`, and
   the server journey projects children only from the scored row. Neither test
   reaches the reported absent-parent result.

Consequently `drift_report['method']`/`['verdict']` can yield empty strings,
`to_json(drift_report['features'])` can yield the text `null`, and
`eval_summary[...]` can yield zero for a null parent. The smallest correction
boundary is the shared Struct materialization/read normalization that can reuse
Arrow's native parent-validity flattening while leaving the public Struct
schema and DataFusion `get_field` syntax unchanged. Proof must cover null and
present summaries on both hot and published reads.

### 3. Predeclared built-ins violate locked competing-error precedence

**Resolution:** `INV-R4-002` is confirmed; the behavior PASS and AC-005 tests
exercise isolated errors, not one request containing competing failures.

`decode_rows` calls `enforce_builtin_source_contract` before the source
fingerprint. For a predeclared built-in, `validate_predeclared` immediately
calls `validate_declared_variants`; that function checks only declared fields
that hold Variant and then traverses their values. Complete undeclared/missing/
non-Variant wire identity is left to the later fingerprint. A request with an
undeclared or non-Variant type mismatch plus malformed/deep/oversized Variant
bytes therefore returns the later Variant error. Canonical signal tables do not
have the defect because `validate_canonical_user_batch` checks undeclared and
all declared field identities before its Variant walk. The predeclared path
must establish the same locked schema phases before value traversal, with a
compound pre-ACK/no-durable-row test.

### 4. Shared Variant/Arrow findings

#### Recursive validation before the Wyrd cap

**Resolution:** `VARIANT-ARROW-R4-001` is confirmed.

`EncodedVariant::from_bytes` checks encoded size, calls
`Variant::try_new`, and only then runs Wyrd's `check_depth`. In
`parquet-variant` 59.3, `try_new_with_metadata` invokes recursive
`with_full_validation`; lists and objects recursively construct each child.
A one-child nested container grows slowly enough to remain well below the
8,388,608-byte ceiling at a depth that can exhaust a worker stack. The Scribe
path is reachable through the built-in validator before ACK/WAL. Existing
depth-65 and malformed-byte tests do not test hostile depth or process
survival. The cap must be enforced by a bounded fallible path before upstream
recursive full validation, while upstream remains the encoding-validity owner.

#### Correct extension name with foreign metadata

**Resolution:** `VARIANT-ARROW-R4-002` is confirmed.

`is_variant` checks only `ARROW:extension:name`. `field_to_spec` then classifies
the field as Variant and removes both extension keys; `spec_to_field`
reconstructs canonical empty metadata. The admission comparison therefore
accepts a correct name with a foreign metadata value and erases the evidence.
The pinned `VariantType` declares empty metadata but its deserializer accepts
and ignores the supplied value. Existing coverage tests missing/foreign names,
not foreign metadata under the correct name. The shared predicate must require
the exact canonical name and empty metadata at every nesting level.

#### JSON conversion precedence

**Resolution:** `VARIANT-ARROW-R4-003` is confirmed.

`from_json_text` returns immediately from recursive `append_raw` for depth or
numeric failures and checks encoded size only after the builder finishes.
Object traversal is sorted-key order. Thus size+depth returns depth, and
numeric+depth can change with key order, contrary to the locked size, JSON,
numeric, depth precedence. Isolated error tests do not prove compound
precedence. One shared conversion owner must retain sufficient failure state
to choose the locked result, with direct, SQL `parse_json`, and pre-ACK compound
proof.

These three findings are related to the prior Variant admission/conversion
work, but they do not share one correction site: byte-validation recursion,
extension identity, and JSON failure selection are separate mechanisms.

### 5. Payload permission is after prohibited provider/peer IO

**Resolution:** the security review's unlabeled medium finding is confirmed as
`SEC-R4-001`; the behavior and Oracle/DataFusion PASS interpretations are too
narrow.

The specification requires refusal, while TASK-001 states it more precisely as
"before provider IO" and "enforces sensitivity before IO." Current order is:

1. acquire the active cut and authorize table identities;
2. materialize metadata/manifests in `planner.rs:257-263`;
3. list live Scribe routes in `oracle/mod.rs:3089`;
4. register cut providers in `oracle/mod.rs:3121-3122`;
5. build the optimized logical plan and only then call
   `authorize_payload_columns` at `oracle/mod.rs:3321`.

The earlier table-scope check correctly precedes object reads, but it is not
the gateway payload permission. `build_physical_root`'s statement that it reads
no row does not make route discovery and provider setup non-IO. The existing
journey observes follower leases; it does not prove zero metadata, route-list,
or provider work. The payload-sensitive logical gate must run after enough
schema planning to identify projected leaves but before cut materialization,
route discovery, and runtime provider setup, with those counters asserted zero
on denial.

### 6. CLI journey closure is required

**Resolution:** `SDK-PARITY-R4-001` is confirmed.

REQ-018 covers any JSON-rendering row surface, and the candidate materially
changes the compiled CLI's JSONL writer to install the Variant encoder. The
repository rule requires a journey for every shipped user-facing capability.
The existing compiled-CLI journey queries primitive columns only; adjacent
Rust/SDK/MCP tests cannot detect CLI-specific metadata propagation, binary
wiring, or JSONL writer regressions. Extending the existing CLI journey with a
Variant containing `3.0` and `u64::MAX` is the required focused proof. This is
task-required verification, not TASK-002 Variant authoring.

### 7. `EncodedVariant::to_json` is an unnecessary parallel surface

**Resolution:** `REUSE-R4-001` is confirmed, narrowed from duplicate
implementation to a dead wrapper/API surface.

The method delegates in one line to `variant_bytes_to_json` and has only two
unit-test callers. Production Rust/Python/TypeScript boundaries already call
the byte renderer, which is the necessary owner because those boundaries hold
separate metadata/value buffers. Keeping a public per-instance method adds no
capability and violates the rule against an unearned helper. Delete it and
route the two tests through the existing byte owner; no replacement abstraction
or new test is needed.

### 8. Final task evidence is stale but the compaction behavior is independently proven

**Resolution:** `MNT-001` is confirmed as an artifact-integrity finding, with
its runtime impact narrowed.

The tracked task binds revision 11 in front matter but its authority link says
revision 10; it records compaction SHA `94db7b94...` while the manifest/lock pin
`2b65fa18...`; and it names nonexistent `is_placeholder` instead of the actual
inline logic in `mask_placeholders`. Those statements cannot accurately
reproduce or navigate the final candidate. The independent r4 durability
review did run both required compaction tests at `2b65fa18...`, so the stale SHA
does not create an unresolved durability defect. It remains a material
acceptance-record defect: the candidate's own claim that its recorded command
ran against the final pin is false. Update only the existing task evidence to
the actual authority, pin/test result, and symbol; do not add another evidence
artifact.

## New proposed findings

No new finding beyond the discovery union is proposed. The deduplicated union
for independent validation is:

- task-review skill scope drift (`BVR-R4-BEH-001` / `STD-R4-001`);
- `INV-R4-001` null Struct child fabrication;
- `INV-R4-002` predeclared admission precedence;
- `VARIANT-ARROW-R4-001` pre-cap recursive validation;
- `VARIANT-ARROW-R4-002` foreign extension metadata acceptance;
- `VARIANT-ARROW-R4-003` JSON compound-error precedence;
- `SEC-R4-001` payload authorization after provider/peer IO;
- `SDK-PARITY-R4-001` missing compiled-CLI Variant journey;
- `REUSE-R4-001` dead public JSON wrapper; and
- `MNT-001` stale final task evidence.

## Result

**RESOLVED**

Every reported conflict was resolved against candidate source and applicable
authority. No candidate-source or authority gap remains for independent
validation.

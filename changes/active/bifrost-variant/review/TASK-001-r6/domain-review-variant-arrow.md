# Variant / Arrow Domain Review

## Immutable Subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-bifrost-variant`
- Base: `80b33286e7dcaab8ed04d06b63eb0ca329b0c2a2`
- Candidate: `ef92074f057b0a240c54b4407d8f32cac1310921`
- Candidate tree: `e3c59991994125dac41187f17de12935e2b45424`
- Approved authority: `changes/active/bifrost-variant/spec.md`, revision 13
- Original task: `changes/active/bifrost-variant/tasks/TASK-001-variant-storage-and-query.md`
- Latest remediation: `changes/active/bifrost-variant/review/TASK-001-r5/TASK-001-R5-close-remaining-variant-contract-gaps.md`

The candidate commit and tree matched the supplied immutable subject before
source inspection. They were rechecked immediately before this report was
written and remained unchanged.

## Reviewed Boundary

This pass reviewed the complete cumulative Variant/Arrow change, with emphasis
on the r1-r5 remediations and the final shared-object-byte commit. It traced:

- every `EncodedVariant` constructor and the `VariantViolations` selection
  order;
- raw JSON parsing, below-depth numeric scanning, exact integer and decimal
  classification, duplicate-key behavior, and recursion/size bounds;
- the iterative raw Variant walker, malformed-byte containment, object field
  names and offsets, Decimal variants, and the final node-count guard;
- `variant_bytes_to_json`, `variant_cell_to_json`, `mask_placeholders`, and
  `VariantJsonEncoderFactory`, including their Rust, Python, TypeScript, CLI,
  MCP, server, Oracle, and test-server consumers; and
- the pinned Arrow 59.3 `parquet-variant`, `parquet-variant-compute`,
  `parquet-variant-json`, and `serde_json` 1.0.150 implementations selected by
  `Cargo.lock`.

The governing sources were `AGENTS.md`, `architecture/agent-rules.md`,
`architecture/bifrost-design.md`,
`architecture/references/domain/arrow-analytical-interop.md`, revision-13
`spec.md`, TASK-001, the five prior Variant/Arrow reviews, their remediation
packets, and the r5 verdict/evidence packet. CodeGraph was used first to map
the owner and callers; the map was then expanded against current source and
the pinned dependency source.

## Authority and Source Coverage

| Boundary | Source/dependency evidence | Result |
|---|---|---|
| Failure precedence | `VariantViolations::{record,finish}`, `append_raw`, `scan_numbers`, `scan_encoded` | **PASS** for the r5 numeric-below-depth and malformed-below-depth cases: selection is malformed, numeric, then depth, and both walks continue below depth without recursive Variant validation. |
| Canonical Decimal16 | `scan_encoded:903-950`; r5 unit/journey evidence | **PASS** for the tested Decimal16 domain: scale zero and `i64::MAX + 1..=u64::MAX` only. **FAIL** for sibling numeric encodings (`VARIANT-ARROW-R6-003`). |
| Raw-walk stack safety | `scan_encoded:896-957`; pinned shallow accessors | **PASS** for admission depth and malformed panics: the explicit stack and `catch_unwind` avoid the former pre-validation recursive call. |
| Shared object bytes | final commit `ef92074f0`; node-count guard; pinned object validation | **FAIL** (`VARIANT-ARROW-R6-001`): the guard stops the demonstrated exponential-node chain, but neither rejects all shared ranges nor bounds rendered output, and renderers bypass it. |
| Object-key uniqueness | `scan_encoded:921-937`; pinned `variant/object.rs:256-277` | **FAIL** (`VARIANT-ARROW-R6-002`): duplicate names are accepted when metadata is not marked sorted. |
| JSON depth handling | `from_json:180-182`, `from_json_text:199-207`, `scan_numbers`; pinned serde recursion behavior | **FAIL** (`VARIANT-ARROW-R6-004`): recursive serde work happens before Wyrd's 64-container decision. |
| Renderer safety | `variant_bytes_to_json`, `variant_cell_to_json`, `mask_placeholders`, encoder factory; pinned `Variant::try_new`, `VariantArray::try_value`, `VariantToJson` | **FAIL** (`VARIANT-ARROW-R6-001`): query/result consumers do not share the bounded validator. |
| Size ordering | `from_bytes:230-246`, `sized:257-263` | Size wins as a returned violation, but both input slices are cloned before `sized` checks them. This is bounded by the admitted Arrow envelope on the server path; it is recorded as a proof limit rather than a separate material finding. |

## Material Proposed Findings

### VARIANT-ARROW-R6-001 — INCORRECT — shared object bytes remain an unbounded rendering path

**Violated obligation.** Revision-13 INV-007 requires Variant processing to be
bounded by value size/depth. TASK-001 requires malformed stored values to fail
without panic and all native/JSON result terminals to preserve the accepted
Variant value. The Arrow interop authority requires malformed analytical
buffers to fail closed within their admitted envelope.

**Exact locations.** `crates/shared/wyrd-queue/src/variant.rs:230-246`,
`:296-327`, `:354-385`, `:405-431`, and `:886-956`.

**Evidence.** The final commit counts visited nodes and rejects only after
`visited > value.len()`. That closes the recorded two-way shared-child chain,
but it does not reject sharing itself. A shallow object can retain many field
offsets into one large child while visiting no more nodes than there are input
bytes; rendering then copies the shared child once per field, so accepted input
of size `n` can materialize quadratic JSON output. The same mechanism can be
combined below containers up to the approved depth.

More directly, every result renderer bypasses `scan_encoded` entirely:
`variant_bytes_to_json` and `mask_placeholders` call recursive
`Variant::try_new`; the cell and encoder paths then call the explicitly shallow
`VariantArray::try_value` followed by recursive `to_json_value`. Pinned
`parquet-variant` object validation at `variant/object.rs:280-296` validates
every supplied offset independently from `offset..` and does not reject shared
ranges. Pinned `VariantArray::try_value` explicitly says it performs no deep
validation. Thus the exact shared chain refused at write admission can still
drive exponential work if it appears in stored/query IPC, and a compact very
deep stored value reaches upstream recursion before Wyrd's depth bound.

This is reachable through Python and TypeScript native conversion
(`sdks/wyrd-sdk-{python,ts}`), Rust row deserialization, CLI/MCP/HTTP Arrow JSON
rendering, Oracle's placeholder masking, and server verification result
decoding. The claim in the r5 remediation that renderers accept only
`EncodedVariant` is not true: their public signatures accept raw byte slices or
arbitrary Arrow arrays.

**Observable consequence.** A hostile or corrupted query cell can consume
superlinear/exponential CPU and output memory, or exhaust the process stack,
instead of returning the promised typed error. Even a value admitted by the
new node-count heuristic can expand far beyond the 8 MiB value envelope during
JSON rendering.

**Required testable correction.** Keep `EncodedVariant`/the existing iterative
scanner as the single owner. Make accepted raw object values prove that field
value byte regions do not alias in a way that permits repeated expansion, and
make every raw-byte/Arrow renderer apply that same bounded validation before
recursive upstream conversion. Do not add a second Variant parser or model.
Prove closure with (1) the existing exponential shared chain through each
renderer class, (2) a shallow many-fields/one-large-child case whose node count
is below `value.len()`, and (3) a deeply nested stored cell; each must return a
typed error promptly and a following ordinary render must still succeed.

### VARIANT-ARROW-R6-002 — INCORRECT — raw objects can retain duplicate keys

**Violated obligation.** Revision-13 REQ-004 and the active Bifrost design
require object keys in every stored Variant to be unique. Final-occurrence-wins
is the normalization rule for JSON and OTel input, not permission for raw Arrow
Variant bytes to retain duplicate fields.

**Exact location.** `crates/shared/wyrd-queue/src/variant.rs:921-937`.

**Evidence.** `scan_encoded` rejects `name <= previous` only when the metadata
dictionary reports itself sorted. For non-sorted metadata it rejects only
`name < previous`, so equal adjacent names pass. The subsequent pinned
`Variant::try_new` has the same gap at
`parquet-variant-59.3.0/src/variant/object.rs:256-277`: its non-sorted-metadata
branch also tests `<`, not `<=`. Repeating one valid field id therefore passes
both Wyrd and upstream validation. The current tests cover repeated JSON text
keys (correctly collapsed by `BTreeMap`) but no already-encoded duplicate
object key.

**Observable consequence.** Raw Arrow admission can acknowledge an object with
two values for one logical key. Object lookup and serde-map rendering can then
select one occurrence and silently discard the other, so the stored value does
not have the unique-key meaning promised across Arrow and JSON terminals.

**Required testable correction.** In the existing raw walker, require strict
lexical increase of object field names regardless of the metadata dictionary's
sorted flag. Add one focused constructor case and one pre-ACK raw Arrow journey
using valid non-sorted metadata with a repeated field id; require
`VARIANT_INVALID_JSON`, no retained row, and a successful following valid
write. Retain JSON/OTel final-occurrence normalization unchanged.

### VARIANT-ARROW-R6-003 — INCORRECT — raw numeric variants outside the JSON domain remain constructible

**Violated obligation.** Revision 13 applies the exact JSON numeric domain to
canonical Arrow Variant input: signed integer primitives, only the approved
scale-zero Decimal16 unsigned range, and finite doubles. Other decimals are
refused before acknowledgement with the numeric-range error.

**Exact location.** `crates/shared/wyrd-queue/src/variant.rs:903-950`.

**Evidence.** The raw scan has one numeric match arm, for disallowed
`Variant::Decimal16`; every `Decimal4`, every `Decimal8`, and every
`Float`/`Double` falls through `_ => {}`. Consequently fractional or
scale-zero Decimal4/8 values are accepted even though JSON would produce a
Double or integer, and NaN/infinite Float/Double values are accepted even
though `raw_number_variant:834-839` explicitly refuses non-finite doubles.
Upstream full validation checks encoding, not Wyrd's numeric domain, so the
later `Variant::try_new` does not close this gap. The renderer documentation at
`:292-301` already concedes that non-finite values cannot render as JSON.

**Observable consequence.** `EncodedVariant::from_bytes` can successfully
construct values that do not render in the required native/JSON terminals,
breaking its invariant and allowing a write to be acknowledged before query or
result conversion fails.

**Required testable correction.** Extend the existing raw numeric-domain arm,
not a new validator: reject Decimal4/Decimal8 and non-finite Float/Double with
the existing numeric-range violation and the applicable numeric kind, while
retaining finite Float/Double and the one canonical Decimal16 range. Prove each
numeric family through the focused constructor test and one raw-IPC no-ACK/no-
row journey followed by a valid write.

### VARIANT-ARROW-R6-004 — INCORRECT — recursive serde work precedes the 64-container JSON bound

**Violated obligation.** The fixed Variant depth is 64 and INV-007 requires
conversion to be bounded by that depth/size. Valid JSON past the bound must be
`VARIANT_TOO_DEEP`; it is not invalid JSON.

**Exact locations.** `crates/shared/wyrd-queue/src/variant.rs:180-207` and
`:677-806`.

**Evidence.** `from_json_text` first calls ordinary
`serde_json::from_str::<&RawValue>`. Pinned serde_json 1.0.150 initializes
`remaining_depth` to 128 (`src/de.rs:59-68`) and returns
`RecursionLimitExceeded` before deserializing a deeper but syntactically valid
document (`:1372-1378`). Wyrd maps every such error to `InvalidJson` at the
root, so its iterative `scan_numbers` never sees the value. The crate does not
enable serde_json's `unbounded_depth` feature. Separately, `from_json` invokes
recursive `Value::to_string()` before any Wyrd depth check, so a deeply
programmatically constructed `Value` can consume stack before the shared raw
walker runs.

**Observable consequence.** Two inputs past the same public depth ceiling can
return different stable errors based only on whether they cross serde's
unrelated internal ceiling; a sufficiently deep `Value` can also terminate the
process instead of returning `TooDeep`.

**Required testable correction.** Enforce the 64-container decision before any
recursive serialization/deserialization can outrun that public bound, while
retaining one JSON conversion owner and no new JSON model. The correction must
not simply disable serde's recursion guard and expose the stack. Prove depths
64, 65, 128, and a much deeper compact value through both public constructors;
64 succeeds and every valid deeper value returns `TooDeep` without panic.
Retain malformed and numeric-below-depth precedence cases.

## Verified Prior Closure

- The r5 iterative raw walk no longer returns at depth 65; it can discover a
  malformed child or non-canonical Decimal16 below/after depth and
  `VariantViolations::finish` selects the locked class order.
- JSON `scan_numbers` now finds an out-of-range integer below the first
  over-depth container and restores the JSON pointer after its iterative walk.
- `is_variant` requires the canonical extension name and absent/empty metadata.
- JSON text and OTel duplicate keys retain the final occurrence before the
  canonical builder writes them.
- The final candidate's node-count guard is a real bounded improvement for the
  exact exponential shared-child fixture; it is incomplete for the renderer
  and shared-range cases described above.

## Verification Evidence and Limits

- The r5 packet records green focused Variant tests, Oracle tests, the two
  server journeys, cross-language journeys, formatting, lints, codegen, docs,
  and `git diff --check`. The final addendum records 12 focused Variant tests,
  the affected server journeys, Oracle tests, formatting, lints, and diff
  check after `ef92074f0`'s shared-child remediation.
- Those recorded tests cover the exact 64-level two-way alias chain, canonical
  and refused Decimal16, malformed/depth precedence, JSON numeric/depth
  precedence, extension metadata, placeholders, and ordinary rendering.
- They do not cover non-sorted-metadata duplicate object names, Decimal4/8,
  non-finite raw Float/Double, serde's depth-128 boundary, a deep
  programmatically built `Value`, shared-byte quadratic rendering, or the
  hostile shared/deep fixtures through result renderers.
- Per review coordination, no Cargo, mise, codegen, or Postgres command was
  started in this pass. Dependency conclusions above come from the exact
  `Cargo.lock` versions and their local source. Recorded r5 evidence is not
  treated as proof for uncovered cases.
- TASK-003 shredded physical layouts and pruning remain outside this logical
  TASK-001 Variant/Arrow review.

## Overall Result

**FAIL** — admission is substantially safer after r5 and the final node guard,
but four reachable gaps remain in the one Variant owner: shared-byte renderer
amplification/recursive validation, duplicate raw object keys, incomplete raw
numeric canonicality, and recursive JSON work before the locked depth bound.

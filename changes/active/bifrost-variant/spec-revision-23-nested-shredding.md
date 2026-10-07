# SPEC-bifrost-variant revision 23: Variants inside Structs and Lists

Status: approved by the user on 2026-10-07 ("Also include unnest").

Kept apart from `spec.md` for the same reason as revisions 20–22. It is
applied after them (frontmatter `revision: 23`).

## Why

Span event and link attributes (`events[].attributes`, `links[].attributes`)
are Variants inside a List of Structs. Today they are never shredded, and a
query such as `events[1]['attributes'] ->> 'gen_ai.finish_reason'` decodes the
whole `events` column: every event's name, time and full attribute value.

The writer, Forge's footer combine, and both readers' unshred step only look
at top-level Variant columns. Shredding on write alone would make no read
faster, so this revision covers write and read together.

## Changes to `spec.md`

### REQ-006 — Spans

Replace "(inside their lists, never shredded)" with:

> (inside their lists, shredded like any other Variant field; REQ-020)

### REQ-020 — Per-file layout

Replace "For each top-level Variant column" with:

> For each Variant field at any depth — a table column, a Struct child, or a
> child of a List element's Struct —

Append:

> For a Variant inside a List, every non-null element value of a sampled row
> counts as one root observation, in that row's stratum. One layout is chosen
> per field and applies to every element.

### REQ-021 — Choosing the layout without a second pass

Append:

> Forge's footer combine counts the leaves of nested Variant fields the same
> way as top-level ones.

### REQ-022 — Shredded files are read correctly everywhere

Append:

> Both readers unshred a Variant at any Struct or List depth before results
> leave the scan.

### REQ-023 — Leaf projection

Append:

> A path through a List element, `l[i]['f'] ->> 'k'` or `l[i]['f']`, declares
> the same leaves of every element, whatever the index: the Struct child `f`
> and, for a Variant, the leaves REQ-023 lists for `k`. Other children of the
> element Struct, and other shredded keys, are not decoded. The same holds
> for a path on an `unnest(l)` output, `u['f'] ->> 'k'`: leaf demand passes
> through unnest to the List column's element.

### Not in scope

- Filter pushdown and row-group skipping on List element paths ("any
  element matches"). Such predicates are evaluated by the query engine with
  the same result (REQ-024's existing fallback).
- JSON arrays inside a Variant value stay residual (unchanged).

### AC-006 — Shredding equivalence

Append:

> Span event and link attributes are shredded in hot and published files and
> read back unchanged before and after compaction.

### AC-007 — Leaf reads and pruning

Append:

> For `events[1]['attributes'] ->> 'k'`, a test shows that on hot and
> published files the decoder reads only `events`' `attributes` leaves for
> `k` (plus `metadata` and the path-local `value`), not the event `name`,
> time, or other shredded keys, and that the result equals an all-residual
> read. The same holds for `unnest(events)` followed by
> `u['attributes'] ->> 'k'`. The existing span journey query in
> `pg_bifrost_e2e.rs` reads shredded event attributes.

### Revision history

Prepend:

> - **Revision 23 (2026-10-07, approved):** Shreds Variant fields inside
>   Structs and Lists (span event and link attributes), unshreds them in both
>   readers, and extends leaf projection through List element access and
>   `unnest`. List predicate pushdown stays out of scope.

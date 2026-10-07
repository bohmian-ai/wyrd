# SPEC-bifrost-variant revision 22: output leaf projection

Status: approved by the user on 2026-10-07 ("agree. make the spec revision").

Kept apart from `spec.md` for the same reason as revisions 20 and 21. It is
applied after them (frontmatter `revision: 22`).

## Why

Both Oracle readers use leaf projection for filters only. For returned
values, they decode the whole Variant root:

- `metadata`;
- the root residual `value`;
- every shredded child.

The readers rebuild the full value and only then extract the requested path.
A query that returns one attribute of a 300-key shredded column therefore
reads all 300 keys. REQ-023 already requires the selected leaves only, but it
does not say this covers output expressions. This revision states it and adds
the proof.

## Changes to `spec.md`

### REQ-023 — Leaf projection

Append:

> This applies to every expression the query reads, not only filters. When
> every use of a Struct or Variant column in the projection and the filter is
> a literal path, each Oracle reader decodes only the leaves those paths
> declare. A shredded Variant path needs:
>
> - its `typed_value` subtree;
> - its path-local residual `value`;
> - the top-level `metadata`.
>
> Other shredded children are not decoded. A query that uses the whole column
> (for example `SELECT v`), or a path the file did not shred, reads the root
> as today. Results are identical either way.

### AC-007 — Leaf reads and pruning

Append:

> For a query that only returns one shredded Variant path, a test shows that:
>
> - on both hot and published files, the decoder reads that path's leaves
>   and not its sibling shredded children;
> - the result equals an all-residual read.
>
> The same holds for a Struct field. The nested-field benchmark measures this
> projection-only case alongside the filtered case.

### Revision history

Prepend:

> - **Revision 22 (2026-10-07, approved):** By explicit human direction,
>   states that REQ-023 leaf projection covers returned expressions as well
>   as filters, on both Oracle readers. AC-007 adds the projection-only proof
>   and benchmark case.

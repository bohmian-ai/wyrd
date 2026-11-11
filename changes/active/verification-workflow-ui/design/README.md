# Verification UI design references

The sole approved mock is [the complete verification mock](mock/dist/index.html),
with [source and instructions](mock/README.md) in `mock/`.
It contains Verify, execution pages, PR review sets, Verifier and Repository Cards,
and GitHub Settings in one shell. The user approved this centralized UI.

The [correction brief](mock-brief.md) records the source comparison, causes of
the rejected consolidation, retained designs and agreed additions. See
[current agreement](notes.md) and [composition](consolidation.md).

| Reference | Purpose | Current coverage |
|---|---|---|
| [Original Verify dashboard](../references/original-canvas/png/Verify.png) | Landing dashboard, timelines, facets, table and drawer | Approved original visual reference |
| [Original Verifier Card](../references/original-canvas/png/Verifier.png) | Exact definition graph and YAML | Approved original visual reference |
| [Original drift inspection](../references/original-canvas/png/VerifyDrift.png) | Statistical requirement detail and windows | Approved original visual reference |
| [Original session inspection](../references/original-canvas/png/VerifySession.png) | Session grouping, members and judgments | Approved original visual reference |
| [Complete verification mock](mock/dist/index.html) · [build](mock/README.md) | All verification and Repository/Settings routes in one folder | Approved centralized UI |

All four original canvas HTML sources and images are preserved in
[original-canvas](../references/original-canvas/README.md). Mock approval does not
make their simulated data or illustrative wire fields a backend contract.

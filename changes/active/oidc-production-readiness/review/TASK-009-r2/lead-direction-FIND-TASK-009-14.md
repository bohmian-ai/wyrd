# Lead direction: FIND-TASK-009-14 withdrawn

Standing human direction: Wyrd does what everyone else does, and reviewer
findings may not require mechanisms that the standard practice does not have.

`architecture/operations/deployment-and-release.md` requires
expand-and-contract only "when old and new replicas overlap", meaning across
releases. Wyrd has no release: the repository has no release tag, so there is
no deployed old replica to overlap with. That matches the reasoning that
withdrew FIND-TASK-004-11 (`review/TASK-004-r2/lead-direction-FIND-TASK-004-11.md`).

Even for a released column, the conventional removal is "stop reading and
writing it, drop it in a later release". A mismatch preflight, a constrained
derived overlap column and dual-writing are not part of that convention.

Audience semantics follow OpenID Connect Core 1.0 §3.1.3.7: the audience is
the client ID. A stored separate audience was never a supported trust choice
under REQ-004, so deriving it from `client_id` is the specified behavior, not
a silent reinterpretation.

Correction: none. Keep `20261002000001_platform_oidc_client_audience.sql` as
shipped. The FIND-TASK-009-14 section of `TASK-009-R2-relying-party-corrections.md`
and its closure proof are void. FIND-TASK-009-11, -12 and -13 stand.

# Thin Task Format

One file per commit: `.dev/plan/<feature>/tasks/NN-<slug>.md`. Target 40–70 lines.
It carries decisions, seams, invariants, acceptance, and verify — **not** rendered
code. The executor (`wyrd-implement`) hydrates seam source from CodeGraph.

## Template

```markdown
# Commit NN — <Title>

> One commit. <crates touched>. Depends on: <NN, NN | —>.

## Goal
What this commit delivers and why, in 2–4 sentences. State the behavior change,
not the implementation.

## Binding decisions
The subset of spec.md / plan.md decisions this commit must honor. Reference, do
not re-derive. One line each.
- <decision> — <one-line consequence for this commit>

## Seams & invariants
For every existing symbol reused or deliberately avoided — named as a symbol,
never file:line — with the invariant the commit depends on.
- `<symbol>` — <invariant the commit relies on>
- Do NOT use `<symbol>` — <why it would break an invariant>

## Approach
The shape of the change in plain steps (not code): what gets added, where the
seam attaches, the order of operations, the one or two correctness arguments that
matter (e.g. a race, a transaction boundary, a tenant-isolation point). Keep it to
the reasoning the executor cannot re-derive on its own.

## Acceptance
- Observable, testable outcomes. Behavior, not internals.

## Verify
- `<exact targeted test/gate command>`

## Done when
- The decisions above hold and acceptance passes; named verify is green.
```

## Worked example — commit 04 thinned (~50 lines vs. the 235-line original)

```markdown
# Commit 04 — Refresh Grant + Rotation with Reuse Detection

> One commit. wyrd-server + wyrd-sql. Depends on: 01.

## Goal
Add the `refresh_token` grant to the existing `/auth/token`. Refresh tokens are
minted today but nothing consumes them. This commit implements rotation with
reuse detection: RFC 9700 refresh model + layer 1 of zero-trust revocation. No
schema change — the `auth_refresh_tokens` table already has the columns/indexes.

## Binding decisions
- Zero-trust revocation, layer 1 (spec) — rotation revokes the consumed row;
  presenting a rotated/revoked token is theft → revoke the whole family.
- Decode-only tenant routing (spec) — derive tenant from the existing signed
  refresh JWT claims; no token-format change, no wyrd-auth-issue edit.
- User-principal mint deferred to commit 06 — the `user` arm is an explicit
  `todo!()` here; keep rotation logic principal-generic so 06 slots in.

## Seams & invariants
- `consume_active_refresh` (new SQL slot) MUST be one atomic
  `UPDATE ... RETURNING` (revoke-and-return) — single-use + race safety depend on
  exactly one caller seeing `Some`.
- `token_hash` — reuse; the stored hash is SHA-256 of the transmitted token, so
  the hash lookup under RLS *is* the bearer check (why decode-only is safe).
- Do NOT route minting through `issue_for_subject` — it inserts via the
  non-rotated `insert_refresh_token` and would break rotation. Add a distinct
  `insert_refresh_token_rotated` slot; leave the existing insert untouched.
- `revoke_refresh_family` (new SQL slot) — bulk revoke by (principal_kind,
  principal_id) for the theft response.
- `insert_audit_token_exchange` — reuse; every transition (rotated,
  reuse_detected, not_found) writes one row to the existing `audit_token_exchange`
  table (F08). Do not invent a new audit path.
- `RefreshError` → `WyrdError` mapping (`impl From`): `Reused → RefreshReused`,
  `NotFound → RefreshRevoked` (also covers expired/revoked), `Database →
  AuthVerifyUnavailable` (503), `Issue → Internal`. Enumerate all four; the DB and
  Issue arms are easy to miss.
- Route owns the transaction boundary (acquire `TenantConn`, execute, commit) —
  mirrors the existing token-exchange arm.

## Approach
New `auth/refresh.rs` service + a `RefreshToken` arm in `auth/routes.rs`, all in
one transaction. Hash the presented token → `consume_active_refresh`. Some →
rotate (mint new access+refresh, insert rotated, audit `rotated`). None → look up
by hash: a stale/rotated row means reuse → `revoke_refresh_family`, audit
`reuse_detected`, return `Reused`; no row → `RefreshRevoked`. The atomic consume
closes the read-then-write race: two concurrent refreshes, exactly one mints, the
other trips reuse. Every transition writes an audit row.

## Acceptance
- Happy rotation: old row revoked (`rotated`), new pair issued, `rotated_from` set.
- Concurrent double-refresh: exactly one succeeds; the other gets `Reused` and the
  family is revoked. No double-mint.
- Replayed rotated token → family revoked. Expired/revoked → `RefreshRevoked`.
- Tenant A's refresh cannot rotate under tenant B (RLS).
- Every transition writes an audit row.

## Verify
- `cargo test -p wyrd-sql --all-features`
- `cargo test -p wyrd-server auth::refresh --all-features`

## Done when
- Rotation + reuse detection are atomic in one transaction; tenant derives from
  JWT claims; no schema change; audit rows on every transition.
```

Note what's gone vs. the original: the SQL bodies, the full Rust signatures, the
`exchange_api_key.rs:435` line refs, the inline algorithm pseudocode. Note what's
kept and sharpened: every decision, every seam **with its invariant**, the race
argument. That is exactly the surface `wyrd-architecture-reviewer` reasons about —
and the executor pulls the real `consume_active_refresh` / `token_hash` /
`issue_for_subject` source from CodeGraph when it writes the code.

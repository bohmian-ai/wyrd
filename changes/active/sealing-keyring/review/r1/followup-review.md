# Focused follow-up: operator rotation instructions

## Subject and scope

Base `526355fb8c58dae6d67d516f98dddcd5c02b0e79`; candidate and current HEAD `202e5bab768cb998a01f04555c39f43b53c09929`. Authority is the approved `changes/active/sealing-keyring/spec.md`; there is no original task, by explicit human direction. Nothing has shipped; this report imposes no compatibility requirement.

The question is whether SYS-001 identifies a reachable defect in the new operator documentation, despite the runtime rotation mechanism itself satisfying the specification. I inspected the cumulative diff and followed CodeGraph before source navigation. Focused source coverage includes the new deployment-table entry; sibling setup, quickstart and troubleshooting guidance; configuration loaders and `AuthConfig`; production boot ordering; `SealedSecretRewrap`; `SealedSecretTable`; `SealingKeyring`; Gateway administration, snapshot and credential resolution; and the changed rotation test. Applicable authority includes the spec, repository agent rules, deployment-and-release configuration and topology contracts, and the existing crypt/config rotation contracts.

## Evidence resolving the uncertainty

1. `docs/src/content/docs/operate/deploy-wyrd-for-a-team.md:72` adds an affirmative rotation instruction: install the new write key and list the old key as retained; startup re-encrypts stored secrets. It also explicitly addresses the key being the same on every replica. It neither pre-stages the next key nor links to a complete rotation procedure.
2. The page says at line 15 that multi-replica topology is not yet documented, and its metadata is draft/partial. This limits its scope but does not make the affirmative rotation instruction correct for the replicas it explicitly mentions. Supported topology remains multiple serving replicas (`architecture/operations/deployment-and-release.md:7-9`). No added sentence requires simultaneous downtime or draining all old-key processes first.
3. The existing `SealingKeyring` contract, `crates/shared/wyrd-crypt/src/lib.rs:181-191`, explicitly starts rotation by adding the new key as retained on every replica before switching the write key. `AuthConfig::sealing_key`, `crates/wyrd/wyrd-server/src/config.rs:2333-2348`, repeats that prerequisite. These are established mechanics, not a new availability or coordination requirement invented by this review.
4. Configuration reads the key files into `SecretString` values (`config.rs:3672-3730`); `build_sealing_keyring` creates an owned in-memory ring (`boot/mod.rs:1956-1976`) once during `build_state` at line 1148. Updating a mounted file cannot update a still-running ring. Production boot calls `rewrap_sealed_secrets` at lines 1206-1208 before returning ready state. The pass enumerates every sealed-secret table, including Gateway credentials, and swaps old bytes to ciphertext under its write key (`sealing.rs:82-109,130-161`; `human_connections.rs:438-490`).
5. The reachable sequence is two active replicas holding only K1, followed by the documented K2-write/K1-retained restart of replica A. A's boot rewrap writes K2 ciphertext into shared Postgres while replica B still holds only K1. A new Gateway snapshot on B contains those bytes; `CredentialResolver::resolve`, `credential.rs:147-157`, opens them using B's retained in-memory ring. `SealingKeyring::open`, `lib.rs:254-266`, cannot authenticate them without K2. Calls using that credential therefore fail before provider dispatch until B reloads a suitable key set. This is temporary availability loss, not ciphertext corruption, and it does not require an old software release or previously shipped data.
6. Sibling setup guidance only addresses preserving the local key. Troubleshooting at `troubleshoot-a-deployment.md:82` describes adding a missing old key after a change, not pre-staging the next key. There is no operator-doc link from the new row to the source-level contract. The module-level procedure in `wyrd-auth/src/sealing.rs:7-15` also starts at the write-key switch, so it is not a complete replacement runbook. Its omitted pre-stage is unchanged surrounding text; the source-local introduced problem is the new deployment row.

There is no conflict between runtime acceptance and this documentation finding: the existing runtime can perform a safe rotation, while the newly added recipe skips its prerequisite. No runtime change is indicated.

## Proposed finding

### FU-001 — New rotation recipe omits the loaded-reader prerequisite (same cause as SYS-001)

- Classification: INCORRECT.
- Changed location: `docs/src/content/docs/operate/deploy-wyrd-for-a-team.md:72`.
- Obligation: the instruction must accurately project the existing deployment keyring lifecycle required by REQ-007 and specified in `SealingKeyring`/`AuthConfig`; it must not direct operators into a reachable serving-key mismatch.
- Observable consequence: following the recipe during a rolling restart makes still-running K1-only replicas unable to use secrets that the first K2 writer has rewrapped. Fresh Gateway calls through those replicas fail until their keyrings are reloaded.
- Smallest correction boundary: remove the optional `To rotate, ... at startup` sentence from this setup row. The row's configuration, managed-secret behavior and missing-key guidance still fully serve the approved change. This deletes an incomplete procedure without adding an unrequested runbook or runtime coordination. If rotation guidance is retained instead, it must include loading K2 as retained on every serving replica before any K2 write/rewrap, then switching writers while retaining K1; that is the existing mechanism, not a new architecture. The retained-old-key retirement proof remains a later pass after all writers switch, as already documented in boot/sealing source.
- Focused proof: review the remaining row for absence of the unsafe recipe and correct setup information; run `mise run docs:check` for any documentation correction. If choosing a complete recipe, walk two loaded rings through its phases and verify each can open K2 before the first K2 boot pass. No new runtime test harness is needed for this prose correction.

## Result and limits

RESOLVED — SYS-001's failure sequence is reachable and rests on existing keyring contracts. Narrow its correction to the new documentation sentence; deletion is sufficient under the approved task and the Ponytail ladder. No additional runtime finding is proposed.

Supporting execution evidence is recorded by the orchestrator in `verification.md` (format, tenant-isolation and dependency checks passed). I did not run tests, broad gates or a live two-replica reproduction; the conclusion follows from owned configuration loading, boot rewrap and the consuming keyring's actual open path. Source and candidate commits remain unchanged; only this report was written.

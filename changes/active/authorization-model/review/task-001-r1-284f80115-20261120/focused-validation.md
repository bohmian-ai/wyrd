# Focused independent validation: authority observation

Candidate: `284f801159a391e92493c3072878fd644c9d173b`.
Remediation base: `3139c08cb0b7c0194583a66af3ab31bef28dc14f`.
Scope: independently validate the proposed remaining cold-load race in FIND-TASK-001-2, including production SQL/listener/request topology and adjacent invalidation behavior. This is source validation, not another complete task review.

## FIND-TASK-001-2 — CONFIRMED, incomplete correction

Classification: `INCORRECT`. Retain the existing finding ID: this is the same lost-observation cause in the authority owner, not a new requirement.

Authority: approved spec revision 2, REQ-004, requires a process that observes newer state to refresh before any allow. `architecture/wyrd-security-posture.md`, Current authority and freshness, states the same known-newer boundary. Neither permits a short grace period after known invalidation.

### Supported production trace

1. A running replica has not yet loaded a particular tenant. `ServerAuth::default` starts with an empty `AuthorityCache` (`crates/wyrd/wyrd-server/src/components/auth/state.rs:75`), and no boot preload creates entries for every tenant. A verified bearer reaches the shared authority owner through `VerifiedBearer::authorize` (`components/auth/token_extract.rs:176-187`). `acquire` finds no entry and starts `reload` (`wyrd-auth/src/authority.rs:536-568`).
2. `TenantAuthority::load` reads revision N and its corresponding grants in one PostgreSQL statement (`wyrd-auth/src/authority.rs:126-127`; `wyrd-sql/src/queries/auth/authority.rs:18-50,124-127`). The statement result does not install an entry. `reload` additionally awaits transaction commit before `install` (`authority.rs:560-568`). This creates a normal asynchronous interval after the N snapshot was read and before the first entry exists.
3. An administrator removes a binding and commits N+1. The mutation's revision SQL sends `<tenant>:<revision>` on commit (`wyrd-sql/src/queries/auth/role_bindings.rs:68-81`). The replica's production refresh worker has an independently polled listener that calls `authority.notify(payload)` (`server/src/components/auth/state.rs:111-134`). There is no lock, tenant-load barrier, or request serialization preventing delivery during step 2.
4. `notify` parses N+1 and calls `observe`; `observe` uses only `get_mut` on an existing entry (`authority.rs:638-641,646-655`). Because this tenant is still cold, it discards N+1.
5. The first load finishes and `install` takes the vacant branch, setting both the snapshot and `observed` to N, then returning the snapshot as usable (`authority.rs:612-632`). The existing bearer can therefore receive the removed grant after this process received N+1. A later background confirmation can repair the entry, but cannot make that known-obsolete allow compliant.

This is not a claim based on calling the helper in isolation: the one-statement load, awaited commit, separate listener, vacant entry, and bearer consumer are connected by the production path above. The process need not restart during the race; an ordinary first request for a tenant on an already-running replica suffices.

### Adjacent invalidation case

Malformed notifications also fence only currently present snapshots by setting `confirmed_at = None` (`authority.rs:656-663`). A pre-notification load may subsequently replace that state and set `confirmed_at = Some(now)` (`authority.rs:615-618`); a cold load has no entry to invalidate at all. The existing test invokes malformed notification only after a numeric observation of revision 6 (`authority.rs:1070-1077`), so the retained numeric fence masks this missing in-flight invalidation protection. This is the same observation/load-lifecycle boundary, not a separate finding or an invitation to add notification protocols.

### Smallest correction and closure

At the existing `AuthorityCache` owner, preserve observations independently of whether a usable snapshot has already been installed. A load must not clear an observation or unknown invalidation that occurred after its database read began. Either install authority proven current with the observation or refuse unavailable; keep short synchronous locks, memory-only warm reads, the sanctioned miss/invalidation acquisition, and the five-minute unobserved-outage bound.

Add deterministic in-owner assertions for the cold sequence (load N pending, observe N+1, install N refuses, install N+1 serves) and an in-flight malformed invalidation without a prior higher numeric fence. Existing warm-entry N/N+1 tests should remain. No new public contract or schema is needed.

## Verification and limits

Reviewed the actual remediation diff, current owner/source, SQL mutation/load owners, production listener wiring, bearer ingress, approved REQ-004 and security authority. CodeGraph was used before tracing code. The current test `an_older_load_never_serves_after_a_newer_revision_is_observed` always installs revision 3 before notification (`authority.rs:1043-1049`), so its assertions do not cover the cold sequence.

No Cargo commands, test mutations, benchmarks, implementation changes, commits, or Git configuration changes were performed. Runtime execution remains with the primary remediation reviewer. Source proves this violation; this report does not claim a newly executed reproducer. Final HEAD check remained `284f801159a391e92493c3072878fd644c9d173b`.

# Domain Review — SDK and Public Contract

## Immutable subject

- Base: `c5c20754a167e8f4d74a555a720bd51df6179a6f`
- Candidate: `261168087376095fa5ad9d66946e755f3baa8fe4`
- Approved spec: `changes/active/admin-principals/spec.md`, revision 14, SHA-256 `66d884ba4379c98ade7571e4d9bb4248abbb8a7ce6b73dec94db1b052594ed02`
- Remediation authorities: R10 and R11 task packets.

## Reviewed boundary and authority coverage

| Surface | Authority | Evidence | Result |
|---|---|---|---|
| Shared delegation owner | Spec REQ-047, R10 §4, AGENTS §§2–3 | `crates/shared/wyrd-client/src/client.rs:275-305` derives a delegated client; `auth.rs:355-380,583-605` owns subject/actor exchange, in-memory caching, and renewal | PASS |
| Rust SDK | R10 client-helper criterion; R11-4 | `sdks/wyrd-sdk-rust/src/lib.rs:1-13` re-exports `wyrd-client`; existing `Bifrost::from_env/connect/connect_with_table` remain the only Rust interfaces (`bifrost/facade.rs:54-111`) | PASS |
| Python projection | AGENTS §§7–8; R10; R11-4 | `sdks/wyrd-sdk-python/src/client.rs:34-87` calls Rust `WyrdClient::on_behalf_of`; `src/bifrost/mod.rs:249-307` accepts optional existing client, rejects all endpoint/credential conflicts, and otherwise retains `client_from_options`; public package/export/stub agree | PASS |
| TypeScript projection | TypeScript guide N-API rules; R10; R11-4 | `native/src/client.rs:71-138` calls Rust delegation and Rust Bifrost composition; `wyrd/src/index.ts:656-708,1012-1061` exposes the mutually exclusive public options and helper; generated declarations match | PASS |
| Environment defaults | R11-4 | Rust `client_from_options` remains the one ambient resolver; Python and TypeScript call it when no explicit client is supplied | PASS |
| Bifrost ownership | AGENTS §2; R11-4 non-goals | Bifrost only consumes `&WyrdClient`; no Bifrost `on_behalf_of`, raw bearer accessor, duplicate exchange, cache, header, or retry path was added | PASS |
| UUID public contract | R11 R9-2 | `IssueKeyResponse` uses `Uuid`; the served OpenAPI assertion requires `format: uuid`; CLI consumes the typed response directly | PASS |
| Generated artifacts | AGENTS §8 and contract-generation rules | Python public stubs and TypeScript emitted declarations reflect the source; R11 evidence records codegen and typechecks passing | PASS |
| Public journeys | AGENTS §11; R11-4 | Python and TypeScript integration journeys prove delegated A-read, denied/no-effect A-write, direct B-write, conflict refusal, and ambient resolution | PASS |

## Review findings

No material findings.

The implementation uses the existing Rust client and Bifrost facade end to end; Python and TypeScript add only boundary handles and argument projection, while normal environment-based construction remains unchanged. UUID typing is carried by the Rust contract and proved against the runtime OpenAPI document.

## Verification limits

- Candidate evidence records the focused SDK tests, Python and TypeScript integration lanes, typechecks, codegen, boundary checks, and broader gate as passing.
- An independent `mise run codegen:check` attempt was blocked by concurrent uncommitted worktree edits not present in the immutable candidate; those edits were not treated as candidate evidence.
- `git diff --check base..candidate` is clean. Candidate HEAD and spec digest remained fixed.

## Verdict

PASS

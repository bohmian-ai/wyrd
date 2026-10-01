# TASK-006 R7 independent findings validation

## Subject and method

Reviewed immutable cumulative range `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543`, approved spec revision 44, original TASK-006, R1–R6 review/remediation, all nine R7 Wave 1 reports, `AGENTS.md`, `architecture/agent-rules.md`, and applicable storage, deployment, security and test authorities. `.codegraph/` is absent. This is a source and recorded-evidence audit; I made no product edits or infrastructure test runs.

I traced the complete `pool_idle_timeout` → `build_operator` → `finish_op` path, its production caller `StorageHandle::from_settings`, local/test caller `StorageHandle::new`, and the Eval test caller; the socket test's helper and calls; the image scripts and compiled server dependency; and the documented production policy and credential flows. The cumulative diff includes the original task and all remediation, not only the R6 increment. The candidate remained HEAD during this validation.

## Disposition of Wave 1 proposals

| Wave 1 source | Decision | Reason |
| --- | --- | --- |
| `TASK-R7-1`, `SR-R7-1`, `DEPLOY-R7-01`, `CLIENT-1` | **CONFIRMED, merged** as `FIND-TASK-006-13` | Both recorded images were built from `abf963ccf`; later `f3c65147e` changed a direct, cloud-enabled server dependency. Existing scripts are sufficient to prove final source. |
| `TASK-R7-2` | **REJECTED** | No managed-cloud failure or provider close margin was observed, and the 15/60-second limits shorten the old 90-second default without adding retries. A universal 2-second limit would also impose unmeasured reconnect and TLS cost after brief idle gaps. Neither alternative has evidence to make its tradeoff a task acceptance requirement; this remains a recorded verification limit. |
| `SR-R7-2` | **CONFIRMED** as `FIND-TASK-006-33` | The new `--lib` test binds a TCP listener and serves HTTP; the repository's fast unit tier is IO-free and live-server tests are gated. The inlined `pg_tests` convention is the smallest relocation. |
| `SR-R7-3` | **CONFIRMED** as `FIND-TASK-006-34` | Function-scoped imports, a fully qualified return type, and missing panic documentation are explicit violations in the changed Rust test items. The correction is local to those items. |
| Security, data, peer, Eval and storage domain reports | **Validated empty** | No competing task-bound finding. The storage domain confirms `HttpClientLayer` adds no write retry and treats unmeasured provider timeouts as a verification limit. |

## Deduplicated retained findings

### FIND-TASK-006-13 — CONFIRMED / MISSING — final-source image journeys

- **Sources:** `TASK-R7-1`, `SR-R7-1`, `DEPLOY-R7-01`, `CLIENT-1`; reopens the prior stable finding.
- **Obligation:** Spec revision 44's pre-release rule before AC-034 requires AC-034/037 proof on the official image built from the reviewed source, pinned by immutable local image ID.
- **Location and evidence:** `scripts/server/test-startup.sh:101-112` and `scripts/server/test-kind-autoscale.sh:110-145` compile, build and pin the image. The R6 evidence records passing images from `abf963ccf`. `f3c65147e` subsequently changes `crates/wyrd/wyrd-storage/src/factory/mod.rs:21-64,164-215`. `wyrd-server/Cargo.toml` compiles `wyrd-storage` with `cloud`, so those images cannot contain final storage behavior. No later startup or kind pass is recorded.
- **Observable consequence:** Startup, Oracle scaling and remote-tail results prove an earlier binary, not the reviewed candidate. This is a proof gap, not evidence of a runtime failure.
- **Smallest correction:** Run the **existing** `mise run test:server:startup` and `mise run test:server:kind` after image inputs are final. Record each source commit and immutable local image ID, plus each script's check that its containers run that image. Later evidence-only commits may follow; show they change no image input. No published image or second kind journey is needed.
- **Focused closure proof:** Both lanes pass on final source with their provenance and container pin checks recorded.

### FIND-TASK-006-33 — CONFIRMED / VIOLATION — socket test in fast unit lane

- **Source:** `SR-R7-2`.
- **Obligation:** `AGENTS.md` §11 keeps the fast unit lane IO-free and server-free; `architecture/agent-rules.md:15-16` gates live-server tests.
- **Location and evidence:** `crates/wyrd/wyrd-storage/src/factory/mod.rs:261-298,343-369` places `closing_keep_alive_server` and `idle_connection_is_not_reused_after_the_pool_timeout` inside the ordinary `#[cfg(test)] mod tests`, reached by `--lib`. The helper binds `127.0.0.1:0`, accepts sockets and serves HTTP. That is a live server even though it runs in-process.
- **Observable consequence:** The fast library suite now depends on socket availability and performs network IO, contrary to its tier contract.
- **Smallest correction:** Move this helper and its sole socket-test caller into an inlined gated `pg_tests` module, using the repository's existing test placement convention. Keep the pure timeout assertion in the ordinary tests. No new external test binary or harness is needed.
- **Focused closure proof:** The ordinary library lane selects no socket test; an exact `mise exec -- cargo nextest run --locked` invocation selects and passes the moved test, plus the owning gated storage task if needed.

### FIND-TASK-006-34 — CONFIRMED / VIOLATION — Rust test imports, signature and panic docs

- **Source:** `SR-R7-3`.
- **Obligation:** `architecture/agent-rules.md:9-10` requires imported bare types in signatures and imports at module top. `AGENTS.md` §16 requires rustdoc, including `# Panics` when a touched item can panic.
- **Location and evidence:** `crates/wyrd/wyrd-storage/src/factory/mod.rs:261-265,305-306,343-344` has three function-scoped import blocks and the helper's fully qualified `Arc<AtomicUsize>` return type. The new helper and both new tests use `expect` or assertions without `# Panics` documentation. All these functions are called by the selected test path or ordinary library tests; none is dormant.
- **Observable consequence:** The changed Rust violates explicit source rules despite passing fmt and lint checks.
- **Smallest correction:** Move imports to their owning test module header, use imported type names in the helper signature, and document the touched panic conditions. Apply this to the helper and retained tests after FIND-33; do not add suppressions or unrelated documentation.
- **Focused closure proof:** Source inspection confirms module-top imports, bare types, and panic docs; `mise run fmt` and `mise run lints` pass.

## Prior finding closure and limits

`FIND-TASK-006-27` and `FIND-TASK-006-31` are closed: the production guide applies its existing network and mesh policies before serving pods and the edge last; both Kubernetes guides and `roles.sql` pass database passwords through environment/process-substitution rather than `psql` or `kubectl` arguments. R6 records a fresh-namespace denial/edge walkthrough, credential-argument capture, TLS refusal checks, and working migration/serving roles. The other R1–R5 closures are unchanged by this increment. The peer lane's absence in R6 is not a new finding because peer admission did not change. This review did not independently rerun infrastructure lanes or measure a cloud provider's idle timeout.

Three bounded findings remain. Proposed `FIND-TASK-006-32` is withdrawn and its ID is not reused. The managed-cloud timeout estimates are unverified, but neither the existing values nor the proposed universal 2-second limit is supported by outcome evidence; they are not a task finding. All retained corrections stay within approved revision 44 and reuse the current owners and tests; no new product or security decision is required.

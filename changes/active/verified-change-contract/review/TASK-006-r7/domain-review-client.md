# TASK-006 R7 client and image-journey domain review

## Subject and coverage

- Immutable range: `f8811ac5035c3aa165d34c38992f9889b3c9081f..f3c65147e0fd828fe5c657d2871ff80f9b3d5543`.
- Authority: approved specification revision 44 (REQ-153, AC-034, AC-037 and the pre-release image-proof rule), original TASK-006, R6 remediation, `AGENTS.md`, `architecture/agent-rules.md`, `architecture/wyrd-design.md`, `architecture/wyrd-doctrine.mdx`, and applicable client and testing references.
- Inspected the cumulative client transport/configuration diff, Rust/Python/TypeScript constructor projections and checks, official-image startup and kind scripts, R6 evidence, the final storage change, and the server's `wyrd-storage` dependency.

| Boundary | Source and verification evidence | Result |
| --- | --- | --- |
| Public gRPC address derives from the effective server URL, with an explicit override | Shared `ClientConfig::from_global_with_overrides` resolves the address; Python and TypeScript constructors wrap that Rust client. Public constructor tests and the startup SDK journey exercise it. | PASS |
| Public HTTPS gRPC verifies the edge certificate | Shared gRPC transport uses native roots; R5 records focused TLS and production edge walkthrough checks. Peer mTLS is separate. | PASS |
| Generated client declarations and examples track the contract | Python and TypeScript client getters and TypeScript declarations are present; R5 records `codegen:check`, example smoke, and public constructor checks. No R6/R7 client contract change was found. | PASS |
| Official-image startup and Oracle autoscaling prove the reviewed source | R6 records startup and kind runs from `abf963ccf`; `f3c65147e` later changed `wyrd-storage`, which the cloud-enabled `wyrd-server` binary compiles into the official image. | FAIL |

## Material proposed finding

**CLIENT-1 — REGRESSION / reopened `FIND-TASK-006-13`.** The specification's pre-release rule and AC-034/037 require the official image built from the reviewed commit. R6 records startup image `sha256:45eec80f…` and kind image `sha256:f7fd2568…`, both from `abf963ccf` (`TASK-006-R6-startup-proof-and-credentials.md:61,95-96`). The candidate's final commit `f3c65147e` changes `crates/wyrd/wyrd-storage/src/factory/mod.rs:53-63,190-207`; `crates/wyrd/wyrd-server/Cargo.toml:11,97` includes that crate with the `cloud` feature, and both image scripts build the server with `--features cloud` (`scripts/server/test-startup.sh:102-112`, `scripts/server/test-kind-autoscale.sh:111-130`). Thus the recorded images cannot contain the candidate's final storage behavior. The code may work, but the required image startup and Oracle scale journey has no passing proof on this candidate. After image inputs are final, rerun the **existing** `mise run test:server:startup` and `mise run test:server:kind` on the final source, record each immutable local image ID/source commit and the container image checks, and ensure subsequent commits change no image input. Do not add another kind journey or require a published image before release.

## Verification limits and result

This was source and recorded-evidence review only; I did not rerun Docker, kind, or SDK tests. The prior kind run proves Oracle scaling for `abf963ccf`, not `f3c65147e`. No independent client contract defect was found.

**Overall: FAIL** for the required current-source official-image journey.

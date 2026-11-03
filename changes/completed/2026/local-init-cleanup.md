# Local setup and verification discovery cleanup

- Change: `local-init-cleanup` (`SPEC-local-init-cleanup`)
- Completed: 2026-11-03
- Accepted target: `b64189f4b979bc8d7252968ac0deaaaeccc7bae8`
- Base: `d7de12a74c4bdbf93e727cc2383147e34298f689`
- Specification: approved revision 3, including revisions 1 and 2
- Completion authority: explicit maintainer acceptance of both tasks and the
  exceptions below, followed by authorization to complete without a final
  change review. No final change-review PASS is claimed.
- Delivery reference: not supplied

Local setup now describes what is actually configured without producing an
ERROR for every expected readiness poll. README and agent onboarding make
Wyrd's primitives for building custom verification systems easier to discover,
using existing workflow and architecture explanations.

## Shipped behavior

- MCP onboarding uses `https://docs.bohmian.ai/wyrd/`. Readiness still returns
  503 until dependencies are ready; expected probe responses log below ERROR.
  Becoming ready logs INFO, and losing readiness logs WARN with failed checks.
  Ordinary request failures and existing dependency diagnostics remain visible.
- `wyrd init` prints a labelled HTTP/gRPC/UI, storage, Bifrost and saved-settings
  checklist. Without a gateway document to apply, it reads stored tenant
  deployments and distinguishes existing configuration, a successful empty
  read and an unknown state after a failed read. Text and JSON share the same
  gateway facts; the JSON field remains a string.
- README has six linked use-case badges and a Why Wyrd table before Install.
  Already have tools? follows the example and is also present in the docs
  introduction so machine readers discover the same coexistence guidance.
- What can I build? offers six example workflows rather than an exhaustive
  capability list. Setup, quickstart and the hub form the canonical onboarding
  order across human links, the shared setup prompt, MCP Start and generated
  machine indexes. The nonexistent Choose your path instruction is removed.
- The existing Uploaded files recipe adds Service binding, invocation,
  citation inspection and the file-tool permission requirement. Selection,
  grouped-session, drift, verification, architecture and Bifrost explanations
  retain their existing owners.
- Dataset documentation now uses `bifrost.datasets`; Check::Agent rustdoc
  acknowledges declared tools. Architecture text removes unsupported general
  Policy enforcement and Source vendor-read promises. The maintainer chose to
  omit those incomplete Cards from the documented inventory while retaining
  their underlying registry/schema contracts unchanged.

## Lasting boundaries and decisions

- Readiness criteria, tenant authority and durable server ownership are
  unchanged. Reporting does not replace credentials, mutate deployments,
  expose secrets or call a provider to validate a key. Configured describes
  stored gateway configuration, not successful inference.
- Applications still own workflow execution and enforcement. Wyrd supplies
  versioned verification and retained evidence; it does not promise formal
  proof, a fixed Agent-judgment latency or automatic vendor integration.
- Documentation routes readers to canonical explanations rather than adding
  another architecture or selection reference. The introduction's comparison
  repeat is explicitly approved to serve docs-only and machine readers.
- Revision 1 established setup and capability discovery. Revision 2 removed
  benchmark publication and duplicated architecture work. Revision 3 moved
  comparisons to the two README reader jobs and repeated coexistence guidance
  in the introduction. No public benchmark tables, numerical vendor rankings,
  new runtime capabilities, dependencies or contract retirement shipped here.
- Card-registration INFO logging and native vendor connectors were outside
  this change.

## Acceptance and evidence

| Outcome | Evidence and accepted limit |
| --- | --- |
| Accurate setup, readiness diagnostics and retained configuration | Three focused logging/summary units passed independently. Implementation evidence records four CLI init journeys, two Python local-development journeys, format and lints passing. The maintainer accepted TASK-001. |
| Broader positioning, accessible routes and generated discovery | Independent review runs passed 62 focused docs tests, the shared-prompt invariant and local-link checks. Implementation evidence records docs:check passing 100 tests plus generation, build and accessibility, and MCP guide discovery passing. |
| Policy Artifact, selected-file tools and cited judgments | Existing real-server SDK journeys with a scripted model exercise upload, selection, file reads, authorization and citations. Exact newly published example-source/public-release execution proof was not supplied; the maintainer explicitly accepted that exception. No real-model judgment-quality result is claimed. |
| Four reader problems through human and MCP entry material | Recorded summaries and revision-3 reruns support the reported comprehension exercise; complete prompts and actual answers were not preserved. The maintainer accepted summaries without requiring recovery or reruns. MCP reader entry used a disclosed source projection rather than a live session. |
| Incomplete Policy/Source documentation | The maintainer accepted omission and current catalog wording despite live registry/schema, composition and attribution support. Missing enforcement/read runtimes were not implemented, and existing contracts were not retired. |

The three exceptions correspond to the independently recorded publication-proof,
reader-record and catalog findings. They were accepted by the maintainer, not
technically fixed or independently closed. No remediation remains required by
the maintainer for this change. Completion performs no additional product
verification and does not imply merge, release or deployment.

## Current owners

- [Protocol and Verifier authority](../../../architecture/wyrd-design.md),
  [Bifrost](../../../architecture/bifrost-design.md),
  [security posture](../../../architecture/wyrd-security-posture.md), and
  [operational readiness](../../../architecture/operations/README.md).
- [CLI setup/reporting](../../../crates/wyrd/wyrd-cli/src/init.rs),
  [HTTP tracing](../../../crates/wyrd/wyrd-server/src/http/router.rs), and
  [readiness](../../../crates/wyrd/wyrd-server/src/components/health/mod.rs).
- [README](../../../README.md),
  [use-case hub](../../../docs/src/content/docs/get-started/what-can-i-build.md),
  [policy recipe](../../../docs/src/content/docs/build-and-verify/verify/verifier-recipes.md),
  [shared setup prompt](../../../crates/wyrd-spec/src/setup-prompt.md), and
  [machine-index generator](../../../docs/scripts/generate_llms_txt.py).
- [CLI journeys](../../../crates/wyrd/wyrd-cli/tests/pg_tests.rs),
  [discovery index tests](../../../docs/src/lib/indexes.test.ts), and
  [Python verification journeys](../../../sdks/wyrd-sdk-python/tests/integration/test_verifier_building_blocks.py).

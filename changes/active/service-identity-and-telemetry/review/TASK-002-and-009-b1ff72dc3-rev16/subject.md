# Authority-resolution continuation of TASK-002/TASK-009 review

Source subject unchanged: repository /home/thorrester/Documents/GitHub/wyrd-worktrees/service-identity-and-telemetry-task-002, branch task/service-identity-and-telemetry/TASK-002, cumulative base036fc48fe26fb87648a277b10099bc475d293b92 to candidate b1ff72dc34c344b596c85cf408e6a9217685be35.

This continues the completed independent acceptance audit in ../TASK-002-and-009-b1ff72dc3/. It is not a new source candidate, post-remediation PASS review, or substitute for that audit. Preserve its eight independent discovery reports, followup, validation, matrices and original SPEC_REVISION_REQUIRED verdict by reference. Do not repeat unchanged discovery or claim fixes landed.

The maintainer explicitly directed the proposed two telemetry paths into the spec, then requested creation of remediation work. Current spec revision16 is approved by that instruction. Relevant owners/supplements/task instructions now distinguish Wyrd-managed frozen state context from application-managed standard resources and authenticated OTLP, with optional validated UID context and no new exporter API. Accepted environment context wins; absent it, resource environment promotes for owned and unowned telemetry. Schema/layout/auth permissions remain unchanged.

Review original FIND-009-4 against this approval; do not prescribe its formerly proposed exporter API. Reconcile the original six bounded findings against the same complete cumulative source and approved authority. New human-directed two-path acceptance is not a retroactive independently discovered FIND ID. TASK003 owns final standard OTLP projection/readback; TASK009 owns configure/provider refusal; TASK008 documents both paths.

Source immutable. Write reports/remediation here only; no implementation, generators, git mutation or broad test runs. Root/nested AGENTS and CodeGraph-first instructions apply. Initial review-time five focused tests are recorded in ../TASK-002-and-009-b1ff72dc3/verification.md; no additional runtime claims are made.

## Final authority hashes

- `changes/active/service-identity-and-telemetry/spec.md`: `9cb1bbe62a2d9adc0ed4efaab960879e488e10e86477a81f158532f825fd9de3`
- `changes/active/service-identity-and-telemetry/supplements/auth-system.md`: `7d32306ebcd264de93d9f62cbe5e2c8be443a07cbdc845a77954a15f8d506dd1`
- `changes/active/service-identity-and-telemetry/supplements/table-storage-and-correlation.md`: `bc72f05a10a8e69c1a251228f335f9d5a99e429d032d25ddc70be93dbe84b3de`
- `changes/active/service-identity-and-telemetry/supplements/telemetry-schemas.md`: `c50f53d7af2e3a86e86f2b9b337ca7ca2d960c6207b8553acdb59ccd200fec00`
- `changes/active/service-identity-and-telemetry/tasks/TASK-002-stable-accounts-and-auth.md`: `4912540517ff37355803b2e7ed6311ba213e8f2cef83a5478f8c3cf75240d73e`
- `changes/active/service-identity-and-telemetry/tasks/TASK-009-configured-service-context.md`: `9b520cae48897da2319193ca15bcae452dfa1d45253781bc9dbcebe20831f897`
- `architecture/wyrd-design.md`: `e389b846f10844d2b419f0afb3f55070992b983328e29097eaf94f5c67384ccf`

## Later tenant-exception authority clarification

The maintainer confirms the TenantConn default and existing isolated one-statement
hot-path exceptions. The focused independent report rejects FIND-009-3. Original
review/validation reports remain historical; the current verdict and remediation
remove its conversion work. Runtime source and exception allowlist are unchanged.

- `crates/wyrd/wyrd-sql/AGENTS.md`: `49093bb91fa26546b08ed3fe7070436c51dd5efcdb1d38c605b03778059031a7`
- `architecture/wyrd-security-posture.md`: `188d476d4eea678e267c8c21e19de9eca4af15a046d5647060aad7bd851ea8a6`
- `architecture/bifrost-design.md`: `8e597af5e37dba55e45f1e2b3af9e9bbbbe8063cc90935d4203b6d3d02193983`

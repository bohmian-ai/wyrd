# TASK-003 R1 re-review subject

- Repository: `/Users/stevenforrester/Documents/GitHub/wyrd`.
- Original base: `a5b1d54174227b26e2f3eb09e833a9172d32e2ab`.
- Prior candidate: `c93287b54bf944c8c8cc9323f81c12ba5174369c`.
- Remediation candidate: `d3db737e5fccacbe02421ea135fd5327b6335d79`.
- Workspace/evidence commit: `fa990be1989959de963d062ad8a47336b40c62da`, a direct child containing only seven prior review report/evidence files. Its production source, tests, original task, specification, public docs, and build configuration match the candidate. `git diff d3db737e5 HEAD` establishes correspondence.
- Remediation diff: `git diff c93287b54 d3db737e5`; cumulative `git diff a5b1d5417...d3db737e5` remains available.
- Approved spec: SPEC-stored-secret-encryption revision 1.
- Original task: `tasks/TASK-003-resource-rotate-and-revoke.md`.
- Prior verdict, validated findings, and implemented R1 task/evidence: sibling `task-003-c93287b54/` directory, recorded at evidence commit.

Current authority limits remain HTTP/CLI only, human/platform renewal HTTP only, no SDK/MCP/UI or migrations. Existing user modifications to AGENTS.md, TESTING.md and wyrd-implement/plan/spec skills are preserved. No CodeGraph directory exists.

## Assignments and navigation

Fresh `r1_ponytail_rev` independently verifies closure of FIND-TASK-003-1 through -5 and checks the affected paths, writing findings-validation.md and verdict.md. Root orchestrates, executes serial focused checks, and verifies candidate immutability; root does not discover or validate findings. Any new proposed finding requires another independent focused source check before retention.

The bounded remediation reuses prior specialized evidence: issuer SQL CAS, HumanConnections audit/locking, five owner lifecycle boundaries. The validator checks new issuer HTTP/CLI replacement, request type, resolver preparation and SQL swap; shared begin_locked audit movement and callers; real identity/session/workload and Gateway dispatch tests; Operator served contract and per-owner docs. No separate specialist is selected initially because these are the original findings' correction boundaries with prior security/lifecycle evidence; material new gaps require focused expertise, not duplicate full discovery.

Issuer proof is interpreted at its actual protocol boundary: workload JWT exchange does not send the trusted issuer's client secret to Keycloak. Journey proof can establish binding continuity, while resolver assertions establish the replacement plaintext. No provider acceptance claim is inferred from that exchange.

# Agent Model Routing

This is harness-facing orchestration policy, not Wyrd product architecture.
`AGENTS.md` defines provider-neutral capability requirements; this file maps
those requirements to models available in the active agent harness. Add another
provider mapping here without changing repository engineering standards.

## Capability tiers

- **Deep reasoning** — architecture, planning, plan review, difficult diagnosis,
  security-sensitive decisions, candidate review, and terminal review.
- **General engineering** — orchestration, integration, repository discovery,
  moderately complex implementation, and localized debugging.
- **Fast execution** — bounded evidence retrieval, mechanical implementation,
  test additions, formatting, compiler-directed repairs, and concrete review
  remediation.

Start at the lowest tier that matches the remaining decision complexity. Route
unexpected material ambiguity or unexplained diagnostic failures to deep
reasoning; return the resolved decision or diagnosis to a general/fast worker
for execution. Do not upgrade a mechanical task merely because it is large.

## OpenAI Codex mapping

| Role | Model | Reasoning |
|---|---|---|
| Architecture and `wyrd-plan-v3` | `gpt-5.6-sol` | high |
| `wyrd-plan-review-v3` | `gpt-5.6-sol` | high |
| Evidence scout | `gpt-5.6-luna` | medium |
| Standard `wyrd-implement-v3` task | `gpt-5.6-sol` | low |
| Mechanical `wyrd-implement-v3` task or concrete remediation | `gpt-5.6-sol` | low |
| `wyrd-implement-plan-v3` controller and normal integration | `gpt-5.6-terra` | medium |
| Complex debugging or reconciliation escalation | `gpt-5.6-sol` | high |
| `wyrd-review-v3` | `gpt-5.6-sol` | medium |
| `wyrd-review-and-plan-v3` root and specialists | `gpt-5.6-sol` | medium |
| Final quality-first adjudication when evidence conflicts | `gpt-5.6-sol` | high |

Classify an implementation task as `fast` only when its packet fixes the
contract and the remaining work is mechanical, localized, and low-risk. Use
`general` for Rust ownership/lifetimes, PyO3, SQL, async, multi-file consumer
closure, or any task whose implementation still requires moderate engineering
judgment. Verification commands consume compute capacity, not model capacity;
the controller runs them directly in resource lanes.

Treat this table as a measured baseline. Adjust a mapping only from repeated
representative-task evidence showing a material quality, latency, or cost gain.

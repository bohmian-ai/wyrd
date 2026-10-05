# Target-bound task closure

## TASK-001

**PASS.** Fresh reviewer availability was confirmed. Commits `e39600aa5` and
`0569b7970` are ancestors of target `395a3ad9`; later changes do not alter the
remediated lifecycle path.

`WyrdTestServer::settle_lifecycle` retains the budget on graceful serve/Bifrost
drain, awaits fallback `Bifrost::abort()` without that expired deadline, and
sets `bifrost_settled` only afterward. `Bifrost::abort` awaits storage abort,
which cancels its owner, awaits retained loaders, and waits for governed work to
be idle. Implicit drop still joins settlement before fixture release. Recorded
evidence selects three focused tests, all passing, plus format, lints,
`test:wyrd`, and patch hygiene.

The new harness test is postcondition proof rather than a differential RED, and
single-thread runtime implicit drop can hang when the blocked runtime owns live
work. These are non-critical, non-primary-path limits.

## TASK-003

**PASS.** Fresh reviewer availability was confirmed. Commits `2ddc56734` and
`b90335e09` are ancestors of target `395a3ad9`; later changes do not alter the
PyO3 method or either declaration.

The public PyO3 `Workflow.run` help now describes route-selected preparation,
Native/WyrdGateway/ExtGateway behavior, and selected external-secret reads at
run start. It agrees with the hand-authored and assembled declarations and
still delegates unchanged to the shared `wyrd-client` owner. The remediation
changed only rustdoc and added no checker, harness, setting, option, or runtime
mechanism.

Recorded format/codegen/diff evidence plus immutable source comparison is
credible for this documentation-only correction. Missing recorded lint and
Python typecheck reruns are a non-blocking verification limit.

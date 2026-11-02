# Focused exemplar stop-condition audit

User asks plainly what the reported exemplar blocker means and why it blocks. This is a bounded review of prior FIND-TASK-004-REV-02-4 and the claimed material stop condition, not a full F1–F10 acceptance review or visual acceptance.
Repository: /home/thorrester/Documents/GitHub/wyrd
Original base: 4a76331113864f9bddc43d3cc8df01080a696937
Prior candidate: f6aeb83a77179ac4450110671df6f564adc1914a
Candidate: fa613baee5e70f9458442f22d74ecebc22893ebc
HEAD and working tree clean at start. Keep implementation immutable. Write assigned reports only in this new directory.
Original task: changes/active/wyrd-ui-main-integration/tasks/TASK-004-revision/TASK-02-live-observe-workflow.md
Approved spec: changes/active/wyrd-ui-main-integration/spec.md revision 18
Prior review and remediation: changes/active/wyrd-ui-main-integration/review/task-004-rev-02-f6aeb83a7-1/{findings-validation.md,verdict.md,TASK-004-REV-02-R1-approved-observe-ui.md}
Read entire prior F4, current remediation evidence/stop-condition and applicable contract. Current report claims no existing SQL produces exact exemplar int_value with readable filtered_attributes and list structure.
Starting map: UI packages/ui/src/lib/observe.ts metric structure projections; ui-pages.integration.test.ts exemplar seeds/assertions; canonical metrics points schema and tables/metrics/projection.rs; wyrd-queue/src/json_result.rs JSON/Variant projection; existing DataFusion variant functions and nested/list query projection owners. Use CodeGraph first. Trace storage→SQL projection→JSON→SDK/BFF/browser.
Fresh focused discovery reviewer evaluates whether precision loss/storage claim and architectural escalation claim are independently supported, including existing nested transform/reconstruction/list aggregation mechanisms. Fresh independent validator then checks discovery evidence and correction burden. No unrelated finding rediscovery, source edits or full environment replay. No full task PASS implied. Existing unrelated review evidence is not reopened.

## Expanded scope by explicit user direction

The user subsequently explicitly requested `$wyrd-task-review review the remainder` and questioned every remaining visual difference. This review now covers full R1 closure for F1–F10, diff prior-candidate→candidate and affected callers/consumers, with cumulative original task/spec and prior verdict/ledger retained. The focused exemplar reviewer keeps its existing bounded assignment/report. No original task obligation is silently waived by an implementor difference list. Human visual acceptance remains separate; reviewer must inspect actual candidate/reference images and code, including controls placement, raw/truncated metric labels, disabled percentiles and mobile drawer. Review must distinguish mandatory truthful unsupported-percentile handling already in original task from arbitrary visual drift.

Reviewer selection: fresh remediation ponytail closure discovery covers all ten prior findings, fix diff, relevant current evidence and affected paths; fresh maintainer specialist covers materially changed SSR/state/metric component props and source-owner reuse; focused followup-rev covers unresolved exemplar precision/correction claim. Fresh independent ponytail validator follows completed discovery and specialist reports. No repo/security/system team recreated automatically: closure reviewer checks the bounded fixes with prior source evidence and their affected paths; add focused review only for unresolved material issue/new validator discovery.

Carried-forward acceptance: original behavior/domain/system/repo reports and findings-validation identify implemented exact trace/root/detail, Logs detail/correlation, SQL query inspector and predicate grammar/tenant projection paths; inspect only changes affecting them, and retain unaffected source-backed evidence. Previous candidate and original base remain recorded above; no rebase correspondence needed if ancestry confirms. No convergence assessment due yet: this is first remediation review after initial FIX_REQUIRED.

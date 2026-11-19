# Standard report delivery

Approved design example; not runnable on today's engine. PR extraction and the
code_review named response format require implementation.

1. Declare [wyrd.toml](wyrd.toml) with `wyrd declare wyrd.toml`.
2. A PR queues [verifier.yaml](verifier.yaml), which extracts the PR and supplies
   metadata, active changes and the existing library context directly to
   [reviewer.yaml](reviewer.yaml). The same extraction mounts as workspace JSON.
3. The reviewer returns the standard report as steps.review.output.
4. The normal code requirement judges completeness and critical/high findings.
5. The GitHub adapter recognizes the execution's one accepted report envelope
   and renders it alongside the execution/verdict. No selector configuration.

The provider/model/deployment values are example configuration. Exact nested
report schemas and GitHub retry behavior are not fully specified here. The
broader [approved contract](../../approved-report-delivery.md) owns selection,
ambiguity and source-validation behavior. Older copied examples retain their
historical publication-based authoring; they have not been migrated to this
output convention.

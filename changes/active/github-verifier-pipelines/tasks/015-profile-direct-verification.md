---
id: TASK-015
status: proposed
approved_spec_revision: 16
depends_on: []
---

# Profile Direct Verify under concurrency

Find where latency rises as canonical Direct Verify load increases and identify
the next fix for the caller-visible tail. Use the integrated server and the
existing L1 driver. These diagnostics support [revision 16 qualification](../spec-revisions/16-shared-workload-management.md#acceptance)
and can inform [TASK-014](014-direct-verification-throughput.md).

The saved `verification/direct-verify/20261117T195143Z-cfa3bee12.json` record
reports Rust c64 client p99 of 16.73 ms, full server p99 of 15.48 ms, and
overhead p99 of 1.10 ms against the 0.50 ms target. Its model and sandbox
phases reach 8.84 and 6.72 ms p99. Treat the observed model-only regression
as established. Use the saved records as evidence and profile the current branch.

```sh
mise run bench:l1:verification -- --steps direct-verify-c1,direct-verify-c4,direct-verify-c16,direct-verify-c32,direct-verify-c64,direct-verify-r3000
mise run bench:l1:verification -- --steps direct-verify-c64 --profile cpu
mise run bench:l1:verification -- --steps direct-verify-c64 --profile heap
mise run bench:l1:verification -- --steps direct-verify-python-c64 --profile cpu
```

1. Use the unprofiled sweep to locate where client p99 climbs relative to
   completions/s and observed server concurrency. Report offered and completed
   rates for the paced 3,000/s case. Profile the first rising step as well as
   c64 if they differ. If the sweep leaves the onset unclear, record which
   offered rates would resolve it using the existing paced driver.
2. Read CPU stacks alongside model-provider header waits, sandbox runnable and
   workspace waits, and evidence staging. Use a fresh c64 heap run to report
   server and worker RSS, live heap, and evidence queue depth through drain.
   Inspect Python GIL reattachment separately. It reached 24.94 ms p99 in
   the saved record.
3. Record the revision, commands, profile paths, phase and throughput evidence,
   and the smallest supported next change or the specific attribution gap.
   Report client p99, full server p99, and Wyrd overhead separately. Include
   verdict and evidence-integrity results. A production fix has its own
   unprofiled qualification under REQ-024.

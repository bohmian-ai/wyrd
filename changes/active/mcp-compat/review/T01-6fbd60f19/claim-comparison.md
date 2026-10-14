# Discovery claim comparison and follow-up decision

All six independent discovery reports are complete for the immutable cumulative subject in `subject.md`. None was given another reviewer's conclusions. All reported FAIL, with the following proposed causes grouped by obligation rather than location:

| Proposed cause | Discovery IDs | Independent validation question |
|---|---|---|
| TS reference proof omits the original full-catalog name comparison | B-01, INV-03, MNT-001 | Does the returned effect-hint set fulfill the specified completeness obligation, and what is the smallest permitted correction? |
| ADK negative proof returns a local undeclared-name error before the Wyrd call | B-02, INV-02, MNT-002, SYS-001, SEC-01 | Does the actual required refusal path reach Wyrd, and which existing public ADK invocation owner could prove it without new transport machinery? |
| ADK provider naming depends on custom copied-tool subclass | B-03, INV-01 | Is this native configuration permitted by r2, or Wyrd-specific adaptation excluded by r2? Distinguish model aliases from unchanged server wire names. If incompatible, determine whether approved authority permits a bounded correction or requires spec revision. |
| New/materially modified Rust items omit required panic documentation | STD-001 | Confirm actual panic paths, applicability of AGENTS §16, and the smallest static documentation closure. |

No focused follow-up discovery reviewer is required: the discovery reports agree on the reachable paths, identify the same owners, and reveal no conflicting implementation evidence or unreviewed path. The maintainer report deliberately leaves the ADK adapter authority judgment to acceptance review rather than asserting an opposing fact. The fresh structured validator must resolve that authority question and independently validate every proposed finding, including the standards-only proposal; agreement is not proof.

There are no prior formal FIND IDs or remediation verdicts to preserve. The current task evidence records the two earlier stop-hook topics (ADK protocol and provider naming); their claimed closure is part of this cumulative audit, not an approved change to r2.

Fresh executed verification is in `verification.md`: six Python framework tests, one TS reference test, and the exact Rust catalog test all pass. No candidate source changes or commit movement occurred.

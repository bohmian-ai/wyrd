# TASK-001 positioning remediation

Subject: candidate `40bcb4a07`, reviewed against `positioning.md`, spec revision 4
and this review's findings (BEH-001…005, MNT-001, EVID-001/002, ST-001/002).
Goal: every entry page shows the positioning's proof chain in runnable code:
request → requirements → evidence → synchronous verify → verdict with reasons →
application decision.

## Decisions for the maintainer (spec revision 5)

Two shipped choices contradict revision 4. Both were deliberate in `6452e02c0`,
so a reviewer can't settle them. Approve or reject each one; the remediation
below assumes the recommendation.

| # | Question | Recommendation | Why |
|---|---|---|---|
| D1 | Policy as a PDF extraction (rev 4) or as a Markdown file the program writes (shipped)? | **Decided by the maintainer: PDF.** Revision 4 stands; no spec change. See R0. | The first run shows PDF text extraction. |
| D2 | Keep the new SDK surface (MNT-001/BEH-003)? | **Decided by the maintainer: keep all.** Spec revision 5 names, as approved SDK additions: typed input models (Rust `InputSlot::fields_of`/`model`, TS schema-source inputs, one model instance projected onto its slots) and verdict predicates (Python `passed`/`failed`/`inconclusive`, Rust `is_passed`/`is_failed`/`is_inconclusive`, TS `VerdictPredicates`). | MNT-001 and BEH-003 close by revision. Every result path that builds a TS result must still attach the predicates; the existing unit and type tests own that. |

No decision remains open. Spec revision 5 covers D2 only; it needs your approval before implementation starts.

## Remediations

### R0 — Restore the PDF policy (D1; BEH-002, EVID-001, ST-002)

- Restore `examples/finance-release/company-sharing-policy.pdf` from `6452e02c0^`.
  In `finance_service.*`, use `Artifact.from_path("company-sharing-policy",
  "company-sharing-policy.pdf").extract(True)` and mount
  `extracted/company-sharing-policy.pdf.json` at `/inputs/policy.json`. The
  judge prompt reads `/inputs/policy.json`. Delete the `POLICY` string and the
  write-if-missing code; `finance_service.*` then has no import side effects,
  so `api.py` imports `Release` from it and drops its `dict[str, object]` copy.
- Add `company-sharing-policy-revised.pdf`, which permits `external audit or
  regulator` purposes, for R4's update step. The original is an 824-byte
  hand-made asset with no generator, so commit the revised PDF the same way
  and keep both under 1 KB.
- Quickstart step 2 downloads the policy before the program runs:
  `curl -LO https://raw.githubusercontent.com/bohmian-ai/wyrd/v<release>/examples/finance-release/company-sharing-policy.pdf`.
  Pin the release tag, not `main`. Neither `main` nor `v0.4.1` contains the
  PDF today, so a URL to `main` returns 404 until this branch merges.
- Restore the `WYRD_LOADER_400_INVALID_ENVELOPE` row in "If it fails" ("Run
  from the directory that holds `company-sharing-policy.pdf`").
- Put "PDF" back in the Quickstart description, the `follows-policy` table row,
  the README and Introduction example sentence, and `setup.md:194`. Keep
  `FinanceFlow`'s `Sharing policy · PDF` label as it is.
- Proof: the existing Rust and SDK authoring journeys already mock a judge that
  reads the extraction. Point the finance fixtures at
  `/inputs/policy.json` and assert that the judge's `fs.read` targets it.

### R0b — Install pins name a release that lacks this change

The docs pin `0.4.1`, but `v0.4.1` was tagged on 1 November, before this
branch. It lacks the typed input models (D2) and the finance example. A reader
who copies the Rust or TypeScript Quickstart against the published 0.4.1 gets
compile or type errors. Bump the workspace to `0.4.2` (additions only, so a
compatible patch under Cargo's 0.x rules), update every pin (README, Setup,
Quickstart, the `[pydantic]` extra line) and the R0 curl tag, and publish
before the docs go live.

### R1 — The programs decide, and print reasons (BEH-001, BEH-005; gaps 1, 2, 4)

`finance_release.{py,rs,ts}` change in four ways:

- Add `purpose: str` to `Release`, and bind `{{purpose}}` into the policy judge's
  prompt and inputs in `finance_service.*` (BEH-005).
- Verify three releases: allowed (`external audit`, quarterly total); the bank
  account (`external audit`, `BANK-ACCOUNT-DEMO-001`); and wrong purpose
  (`marketing partnership`, quarterly total). The third case answers the
  "a regex would do" objection, because only a reader of the policy fails it.
- Only `passed` releases. `failed`, `inconclusive` and a raised error hold.
- Print the reason for each requirement that did not pass.

Python shape (Rust uses `match` on `Ok`/`Err`; TS uses `try`/`catch` on `WyrdError`):

```python
    try:
        result = run.observe.verify("release-check", release)
    except WyrdError as error:
        print(f"held ({error.code}): {label}")
        continue
    if result.passed:
        print(f"released: {label}")  # Your application sends the report here.
    else:
        print(f"held ({result.verdict}): {label}")
        for requirement in result.requirements:
            if requirement["outcome"] != "passed":
                print(f"  {requirement['id']}: {requirement['reason']}")
```

`label` is `f"{purpose} · {text}"`. Captured output (fixture reasons; mark
"the judge's wording varies" under each block):

```text
released: external audit · Synthetic customer report: quarterly total $120.
held (failed): external audit · Synthetic customer report: paid from BANK-ACCOUNT-DEMO-001.
  follows-policy: <fixture reason citing the bank-account line>
held (failed): marketing partnership · Synthetic customer report: quarterly total $120.
  follows-policy: <fixture reason citing the external-audit line>
```

Proof: extend the existing journeys (`finance_release_passes_and_fails`,
`test_docsite_quickstart_verifies_and_repeats`, TS `runs the finance release
Quickstart…`) to assert all three lines. The local provider fixture must pass
only when it sees `external audit` and no bank marker. That fixture rule is
what proves the judge uses the purpose.

### R3 — `api.py` holds on errors (BEH-001)

Wrap the `verify` call in `try`/`except WyrdError`, and return
`hold_for_review(release)` from the `except` branch. Keep `r.passed` (D2). Re-run `test_finance_api_releases_only_passed_reports`,
adding one error case. Regenerate `FinanceFlow`'s 12 numbered lines from the new
regions. If the handler grows past the figure's height, keep the `try` outside
the `handler` anchor.

### R4 — Show which versions produced each verdict (BEH-004, EVID-002)

- Programs: append the Verifier version to every decision line, for example
  `released (release-check@0.1.0): …`. After step 5 the reader then sees `0.1.1`
  printed with no extra command.
- `stored-verdicts.sql`: add `execution_id` and `verifier_card_uid`. Then link
  step 4 to the existing `Execution.details` procedure for reading the exact
  policy file each verdict used.
- Step 5 policy update: download `company-sharing-policy-revised.pdf` over
  `company-sharing-policy.pdf` (R0), and add a fourth run with purpose `regulator request`.
  It is held under `0.1.0` and released under `0.1.1`. This makes the changed
  rule change a verdict. Extend `finance_release_policy_versions_remain_attributable`
  to assert both outcomes and the old execution's details naming the old
  policy file.

### R5 — Real-time without a latency claim (gap 3)

Add this after the Quickstart's step 2 output, and after the README example's output:

> `verify` returns before your code acts, so the verdict is part of the request.
> Assertions add little time; the policy judge adds a model call, up to three
> tool turns, to that request. If the judge can't decide, the verdict is
> `inconclusive`; if the call fails, `verify` raises. Both hold the report.
> `submit` queues the same check when the request can't wait.

Link `submit` to `verify/` until Task 2 publishes `/understand/real-time-verification/`,
then repoint it there (Task 2 handoff note).

### R6 — Make "only" mean something (gap 5)

- Introduction: delete "Wyrd is the only real-time verification system built
  for your applications:" from the third paragraph. That paragraph starts
  "Wyrd gives you open-source building blocks…" to match the README. The
  headline is the only place the claim appears.
- README and Introduction: add one row to "Already have tools?":
  `| Guardrails AI, NeMo Guardrails | Predefined input/output filters | Your own requirements, judged with your evidence and files, returned to your code to decide |`.
- Add this under "Where Wyrd stops" in both: "Wyrd returns verdicts; it never
  blocks, retries, or reroutes your application. Your code decides."

The competitor-scope evidence the positioning requires for "only" belongs in
Task 2's real-time page. Record it there as a publication blocker.

### R7 — Use cases follow the positioning (gap 6)

README badges, in this order: Runtime verification, Policy verification, Agent
evaluation, Long-horizon workflows, Drift monitoring. Drop the Observability
badge, which contradicts "Not just observability". The `what-can-i-build`
headings stay as they are (Task 2 owns them).

### R8 — The README example is shorter

Add `# ANCHOR: verifier` and `# ANCHOR_END: verifier` around `release_check` in
`finance_service.py`. The README embeds `finance_service.py region=verifier`,
which is about 25 lines of requirements, followed by the full program. Link
"the models and the policy judge" to the file. The Introduction and Quickstart
keep the full file. `readme.test.ts` already supports `region=`.

### R9 — Stale and overclaiming copy

| File | Change |
|---|---|
| `setup.md:194` | "verifies a finance report against a policy PDF and releases it only when it passes." |
| `+page.svelte:52` eyebrow | "Open-source real-time verification for your applications · v…" |
| `+page.svelte:112` Observe tab | "Start a Run and send each release the application is about to make. `verify` waits for the verdict; `submit` queues the check and returns at once." |
| `+page.server.ts:19` `RESULT` | Build it from the R1 fixture's captured result for the bank-account release, not hand-written reasons |
| README "How Wyrd fits", Observe item | "Your app sends Bifrost, Wyrd's data plane, the telemetry, data, and evidence you choose to record." |
| Quickstart intro | "your code releases the report or holds it" now matches R1. Delete "Only a `passed` verdict releases a report. Wyrd never sends or blocks anything itself; your code decides." because R5 and R6 cover it |
| `generate_llms_txt.py` | Rerun `docs:generate` after the edits above |

### R10 — Rustdoc on kept items (ST-001)

Add intent, invariants and `# Errors` to every `InputSlot` model and fields
item, schema fixture and verdict predicate kept under D2.

## Closure proof

Run these sequentially:

1. The Rust `finance_release` target (3 tests), the Python `test_docsite_quickstart.py`
   and `test_finance_api_releases_only_passed_reports`, and the TS `finance-release.test.ts`.
2. `vitest run src/lib/readme.test.ts src/lib/indexes.test.ts src/routes/shell-links.test.ts`,
   then `docs:generate` with no diff, then `docs:check:commands`, `docs:linkcheck`,
   `docs:svelte:check`, `docs:build` and `docs:a11y`.
3. Format, lint and typecheck for Rust, Python and TS, the public-signature diff
   against base (D2: exactly the additions spec revision 5 names), and
   `git diff --check`.
4. Browser: the homepage figure at 1440 and 390 wide, with the new handler lines,
   no clipping, and the hold branch still rendering.
5. A reader test: give a fresh agent only the Quickstart, and confirm it can say
   which requirement failed and why, and what the app did.

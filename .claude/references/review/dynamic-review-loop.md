# Dynamic Architecture Review Loop

The terminal review contract for pipeline gates. Gate 1 (`wyrd-spec`, on
`spec.md`) and Gate 2 (`wyrd-plan`, on `plan.md`) both run this loop. It wraps
repeated **full-sweep** reviews in a loop that only exits on an explicit
approval — so no artifact advances to the next stage on a single review plus a
single correction.

This does **not** weaken full-sweep behavior. Every iteration is a real full
sweep per `full-sweep-review.md`. The loop only adds the terminal condition:
review, revise, re-review, repeat until the reviewer returns `Decision: approve`.

## The Loop

1. **Submit.** The authoring agent submits the artifact (`spec.md` for Gate 1,
   `plan.md` for Gate 2) to `wyrd-architecture-reviewer`.
2. **Review.** The reviewer runs a **full-sweep** review and emits a consensus
   ledger plus a `Decision`.
3. **Check the decision.**
   - `Decision: approve` → **terminal pass.** Exit the loop and proceed to the
     next stage.
   - Any other decision → **non-terminal.** Continue.
4. **Revise.** The authoring agent addresses **every** confirmed finding, or —
   for any finding it does not fix — records an explicit **accepted deferral** or
   **reopen** decision with a one-line rationale. No confirmed finding may be
   left silently unaddressed.
5. **Fresh re-review.** The reviewer re-reviews the **revised** artifact. This is
   **not** a closure-only check: it verifies closure of every prior confirmed
   finding **and** runs a fresh full sweep for issues the revision introduced,
   then emits a new `Decision`.
6. **Repeat from step 3** until `Decision: approve`.

## Terminal State

`Decision: approve` is the **only** terminal pass state for a pipeline gate.

The other decisions are **non-terminal** — each forces another author revision
plus a fresh reviewer pass:

- `approve with changes` — the changes must be made and re-reviewed. At a
  pipeline gate this is **not** a pass; the user wants explicit sign-off on the
  final revised artifact before the stage advances.
- `needs redesign` — revise substantially and re-review.
- `reopen locked decision` — resolve the reopened decision (see stop
  conditions), then re-review.

## Iteration Evidence

Every iteration must preserve, at minimum:

- iteration number
- reviewed artifact path
- previous consensus ledger path (when a prior iteration exists)
- findings addressed
- findings deferred or reopened, with the accepted rationale
- new findings introduced by the revision
- final `Decision`

Persist this to a **loop ledger** at
`.dev/review/architecture/{REVIEW_ID}/loop-ledger.md`, appending one block per
iteration. Per-iteration intake and consensus ledgers live beside it, exactly as
`full-sweep-review.md` specifies. If the workspace is not writable, inline the
same loop ledger in the response.

### Loop Ledger Block Shape

```markdown
## Iteration {N}

- **Artifact**: {path to spec.md / plan.md reviewed this iteration}
- **Previous consensus ledger**: {path, or "—" for iteration 1}
- **Consensus ledger (this iteration)**: {path}
- **Findings addressed**: {IDs / one-line summaries}
- **Findings deferred / reopened**: {ID → accepted rationale, or "none"}
- **New findings introduced by the revision**: {IDs / summaries, or "none"}
- **Decision**: approve | approve with changes | needs redesign | reopen locked decision
```

## Stop Conditions

- **`approve`** → proceed to the next stage.
- **`reopen locked decision`** → return to interview/decision work, then resume
  the loop.
  - Gate 1: loop back into the `wyrd-plan-interviewer` / decision step of
    `wyrd-spec`, settle the decision, then re-submit `spec.md`.
  - Gate 2: if a **spec** decision reopens, return to `wyrd-spec`; if a
    plan-level decision reopens, revise `plan.md`. Then re-submit for review.
- **User explicitly stops or changes scope** → stop the loop and record the
  current non-terminal state (last `Decision`, open findings) in the loop ledger.
  Do not report the gate as passed.
- **Same blocking condition repeats and cannot be resolved without user input**
  → surface the blocker to the user with the repeated finding and why it cannot
  be closed. Do **not** pretend the gate approved.

## Anti-Pattern (reject this)

The authoring agent asks to advance to the next stage after only one correction,
without a fresh re-review. **Not allowed.** A gate advances only after the
reviewer has re-reviewed the revised artifact and returned `Decision: approve`.
One review plus one revision is never a pass on its own.

# Adversarial Static Review Contract

Use adversarial analysis to test the change's important assumptions, not to
make the report combative or inflate its finding count.

The operating posture is: aggressive during discovery, skeptical during
validation, conservative during promotion, and neutral in the maintainer
handoff. Never require a minimum number of findings, reward severity, or
promote a hypothetical failure without a reachable path and target-snapshot
evidence.

## Full-mode probes

Every specialist attempts to falsify the important invariants within its
assignment. At minimum:

1. Construct the strongest realistic failure case for each materially changed
   behavior in the assigned impact slice.
2. Attack relevant boundary transitions, including trusted to untrusted,
   tenant to storage, validation to mutation, build to publish, sync to async,
   and typed API to serialized representation.
3. Challenge happy-path assumptions with malformed and boundary inputs; stale,
   missing, duplicated, reordered, or concurrent state; partial completion and
   retry; caller misuse permitted by public types; and dependency failure or
   restart when applicable.
4. Search for semantic omissions in alternate entry points, callers, inverse
   and cleanup operations, feature-gated paths, and generated, persisted, or
   serialized projections.
5. Attempt to disprove every candidate before returning it.
6. When the assignment is clean, identify the most dangerous relevant changed
   invariant and record the counterexample attempted and why the implementation
   survived it. A clean result without a falsification attempt is incomplete.

Record each material attempt under `## Adversarial probe`:

```markdown
- Invariant attacked:
  Failure construction:
  Entry point and reachable state:
  Evidence inspected:
  Result: survived | candidate emitted | static limit
```

Tests and stated intent are evidence of considered behavior, not proof that the
implementation is correct. Inspect the production flow that is supposed to
enforce them.

## Validation

Validation is adversarial in both directions:

- try to disprove every proposed candidate before confirmation;
- try to disprove a clean conclusion rather than treating the absence of
  candidates as evidence;
- challenge whether guards dominate every mutation or durable effect;
- challenge whether the correction preserves intent, ownership, and public
  contracts without creating a wider failure.

Independent full-mode validation challenges both preliminary finding decisions
and high-risk boundaries declared clean. Static uncertainty remains a limit,
not a required finding.

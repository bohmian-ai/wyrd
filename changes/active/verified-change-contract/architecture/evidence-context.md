# Verification Evidence and runtime context

**Status:** working design with agreed foundations

## Agreed Evidence types

### EC-001: `code_diff`

**Status:** agreed on 2026-09-10

`code_diff` is the full commit diff across the Change Request revision's exact
Subjects. Opening or revising records each repository, base commit, and
candidate commit, so Wyrd can make `code_diff` available immediately without a
pull request. A code-diff-only Verifier is runnable immediately.

A code-review Verifier may materialize the recorded base and candidate commits
and use CodeGraph to compare them. That analysis is part of Verifier execution;
it is not another Evidence type.

### EC-002: `test_results`

**Status:** agreed on 2026-09-10

`test_results` is one canonical, candidate-bound contract containing CI
execution status, aggregate and suite summaries, and individual test outcomes.
Wyrd clients expose one universal parser that converts supported JUnit XML
dialects into the canonical contract before upload. The server validates and
stores canonical `test_results`; it does not own framework-specific parsing.

Parser compatibility is proven with representative reports from common Python,
Rust, JavaScript, JVM, Go, and .NET test runners. The parser accepts the common
JUnit family rather than claiming one universal XSD.

Large suites preserve every normalized test case and precompute summaries.
Inputs are bounded and rejected explicitly when oversized; Wyrd does not
silently truncate Evidence.

### EC-003: runtime `context`

**Status:** agreed on 2026-09-10

`context` is the dynamic JSON-object escape hatch for custom Verifiers. It is
provided when verification is requested, not authored as a static value in a
Verifier Card or Change Request. Wyrd binds its digest to the resulting run.

Context does not replace the typed `code_diff` or `test_results` contracts and
must not hide a dependency on another Verifier result.

## Execution boundary

Every Verifier receives one Verification Input containing its Claim or Change
Request target, only the Evidence it requires, and runtime context when the
Verifier accepts it. Built-ins own their typed input requirements. Custom tasks
declare the Evidence types they consume.

The exact canonical payload fields, context declaration syntax, size limits,
and submission API remain open.

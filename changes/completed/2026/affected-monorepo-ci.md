# Dependency-aware monorepo CI and release builds

- Change: `affected-monorepo-ci`
- Specification: `SPEC-affected-monorepo-ci`, approved revision 1
- Completed: 2026-10-01
- Reviewed target: `7d96c30066425e0cde2290842d5801307843283d`
- Delivery reference: not supplied

Wyrd CI now derives affected Rust packages and transitive consumers from workspace metadata, adds explicit cross-boundary SDK and journey lanes, and falls back to the full gate for mixed, global, deleted, renamed, unknown, or unclassifiable changes. Selection explains every chosen lane and never permits a changed code path to produce an empty green run.

Routine `main` builds package only affected deliverables. Release qualification still builds the complete supported platform set, and publication verifies the recorded digests of the tested artifacts. Server journeys remain Linux-owned; first-class SDK checks retain Linux and macOS coverage; credentialed cloud tests remain outside pull requests.

The lasting constraints are conservative closure, unchanged test meaning, bounded heavy journeys, compatible cache keys, full nightly and release regression, and publication of the verified bytes. No new build system or contract authority was introduced.

Acceptance closed through selector examples covering leaf, shared, wire, Bifrost, mixed, global, docs, unknown, renamed, deleted, SDK, identity, and storage changes; workflow assertions; affected packaging cases; and immutable artifact verification. The selector self-check passes all 53 cases on the completed target.

Current owners and evidence:

- [Release authority](../../../architecture/operations/deployment-and-release.md) and [testing map](../../../TESTING.md).
- [CI selector](../../../.github/scripts/select-ci.py), [selector checks](../../../.github/scripts/tests/test-detect-changes.sh), and [pull-request workflow](../../../.github/workflows/lints-test.yml).
- [Release workflow](../../../.github/workflows/release.yml) and [artifact verifier](../../../.github/scripts/verify-release-artifacts.sh).

# Platform portability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation tasks: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md` and `changes/active/oidc-production-readiness/review/TASK-012-r2/TASK-012-R2-documentation-closure.md`
- Candidate identity was rechecked after source inspection and focused verification and remained unchanged.

## Reviewed boundary and source coverage

This review covered only the device-login browser-launch and platform-portability boundary. It traced the verification URI from the `oauth2` device response through the CLI print, skip, launch, and fallback branches; inspected dependency ownership and resolved versions; confirmed deletion of Wyrd's former per-OS launcher; inspected the target-selected native Windows implementation of the pinned dependency; and checked that both remediation rounds preserved the accepted platform behavior.

| Boundary | Authority and source evidence | Result |
|---|---|---|
| CLI launch flow | REQ-011; TASK-012 Scenario 4; `crates/wyrd/wyrd-cli/src/auth/login.rs:84-125` | `LoginFlow::run` selects the standard complete verification URI when supplied, otherwise the ordinary verification URI, prints it before any launch attempt, passes that one `&str` directly to `webbrowser::open`, and retains the manual-open message when launch returns an error. PASS. |
| `--no-browser` | `crates/wyrd/wyrd-cli/src/auth/login.rs:30-42,149-155,281-301`; `crates/wyrd/wyrd-cli/tests/cli_login_journey.rs:203-215`; recorded filtered CLI journey evidence in TASK-012 | `login` derives one boolean from the public flag and skips `webbrowser::open` when requested. The journey invokes the flag while completing the real device flow. No browser-command setting or alternate launcher exists. PASS. |
| URL argument boundary | `crates/wyrd/wyrd-cli/src/auth/login.rs:106-116`; installed `webbrowser-1.2.4/src/lib.rs:278-316,406-428`; native Windows `src/windows.rs:118-139` | Wyrd performs no shell parsing, interpolation, escaping, or process construction. The library parses the supplied string as one URL target; its native Windows path substitutes the URL as one `std::process::Command` argument. PASS. |
| Removed Wyrd per-OS launcher | Cumulative base-to-candidate diff for `crates/wyrd/wyrd-cli/src/auth/login.rs`; repository search under `wyrd-cli` and `wyrd-client` for `browser_command`, `open_in_browser`, `cmd /C`, `cmd.exe`, `rundll32`, and `xdg-open` | The Wyrd-owned `open_in_browser`/`browser_command`, OS branches, and process spawning are deleted. The only Wyrd browser-launch call is `webbrowser::open`. PASS. |
| Dependency ownership and version | Root `Cargo.toml:106-107`; `crates/wyrd/wyrd-cli/Cargo.toml:48`; resolved `Cargo.lock` package `webbrowser 1.2.4`; focused `cargo tree` result | The approved version is pinned exactly and directly owned by `wyrd-cli`. No launcher implementation or dependency was added to `wyrd-client` or a language SDK, and no new Cargo feature was introduced for the browser path. PASS. |
| Native Windows path and proof | Installed `webbrowser-1.2.4/src/windows.rs:22-139`; `mise exec -- cargo check -p webbrowser --target x86_64-pc-windows-msvc` | The dependency uses `AssocQueryStringW` to resolve the native registered HTTP handler, builds the handler process directly, and passes the URL as an argument without `cmd.exe`. The exact lead-approved Windows-target check passed. PASS. |
| Remediation preservation | Latest remediation diff `5d9a3ddfad426eb545e866658a74428df72265ef..dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447`; R1/R2 preserved-behavior and non-goal clauses | R2 changes documentation and generated schema descriptions only; it does not modify the CLI launcher, dependency version, features, platform branch, option, test harness, or runtime behavior. R1 likewise preserves the accepted launcher boundary. PASS. |
| Standard-only complexity | Approved task and both remediation non-goals; standing lead direction | The candidate uses the selected conventional library directly and adds no Wyrd-specific launcher, escaping layer, configurable browser command, WSL branch, compatibility path, or platform harness. PASS. |

## Prior platform proposal

`PLAT-001` remains rejected. It concerned dependency-owned WSL behavior and was independently excluded from the validated round-1 ledger. The standing human direction expressly preserves that decision, accepts the native-Windows crate check as the Windows proof, and prohibits a Wyrd WSL branch, replacement launcher, escaping layer, browser-command option, or new platform harness. The cumulative candidate introduces no new Wyrd-owned platform path that could create a distinct finding, so this review does not reopen it.

## Material proposed findings

None.

## Verification assessment and limits

- `mise exec -- cargo check -p webbrowser --target x86_64-pc-windows-msvc` exited 0.
- `mise exec -- cargo tree -i webbrowser -e normal --workspace --locked` resolved `webbrowser v1.2.4` through `wyrd-cli`; the Python SDK appears only because it consumes the CLI crate, not because it owns another launcher.
- `git diff --check adf349081077b3cfe0d56ab9a665cf01e2d86da4 dc67bf1c31c3fc6f4e6e05744b75b9c83e9fa447` exited 0.
- The filtered CLI journey recorded in TASK-012 uses `--no-browser`; it proves the public skip path while source inspection proves the launch and failure-fallback control flow. A live desktop launch was not exercised on this headless Linux host.
- No full `wyrd-cli` Windows cross-build is required: the locked proof is the passing `webbrowser` Windows-target check. The previously observed absence of a Windows C cross-toolchain is therefore not a verification limit for this domain.
- No full identity journey, language sweep, or aggregate was run or required. Those exceed the user-directed narrow write-set verification for this review.

## Overall result

**PASS**

Proposed finding IDs: none.

# Platform portability domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `5d9a3ddfad426eb545e866658a74428df72265ef`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Original task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Remediation task: `changes/active/oidc-production-readiness/review/TASK-012-r1/TASK-012-R1-client-oauth2-closure.md`
- Prior platform proposal: `PLAT-001` was independently rejected in round 1 and remains rejected by standing lead direction. This review does not reopen it.
- Candidate identity was rechecked after source inspection and remained unchanged.

## Reviewed boundary and source coverage

This review covered only the device-login browser-launch and platform-portability boundary. It traced the verification URI from the `oauth2` device response through the CLI's print, skip, launch, and fallback branches; inspected dependency ownership and resolved versions; confirmed deletion of the Wyrd per-OS launcher; and inspected `webbrowser 1.2.4`'s target-selected native Windows implementation and build requirements.

| Boundary | Authority and source evidence | Result |
|---|---|---|
| CLI launch flow | REQ-011; TASK-012 Scenario 4; `crates/wyrd/wyrd-cli/src/auth/login.rs:100-125,149-155` | The CLI selects the standard complete verification URI when present, prints the URL before attempting launch, calls `webbrowser::open` only when `--no-browser` is absent, and retains a manual-open message when launch returns an error. PASS. |
| `--no-browser` contract | `crates/wyrd/wyrd-cli/src/auth/login.rs:35-42,149-155,281-295`; recorded filtered `cli_device_login_journey` evidence | The public option remains the conventional skip switch and does not introduce a browser-command setting or alternate launcher. PASS. |
| Dependency ownership | Root `Cargo.toml:101-106`; `crates/wyrd/wyrd-cli/Cargo.toml:40-49`; resolved `Cargo.lock` entry for `webbrowser 1.2.4`; `cargo tree -i webbrowser -e normal --workspace --locked` | `webbrowser` is pinned to the approved version and is directly owned by `wyrd-cli`; no browser-launch dependency or implementation was added to the shared client or language SDKs. PASS. |
| Removed Wyrd launcher | Cumulative base-to-candidate diff for `crates/wyrd/wyrd-cli/src/auth/login.rs`; repository search for `browser_command`, `open_in_browser`, `cmd /C`, `rundll32`, and `xdg-open` under the CLI/client owners | The Wyrd-owned `browser_command` and `open_in_browser` functions, OS branches, process construction, and bespoke platform test are deleted. One dependency call now owns launch behavior. PASS. |
| Native Windows path | Installed `webbrowser-1.2.4/src/windows.rs`, especially `open_browser_internal`, `AssocQueryStringW`, and `get_browser_cmd` | The selected Windows implementation obtains the registered HTTP handler with the native association API, substitutes the URL as a process argument, and does not invoke `cmd.exe`. This is the exact standard-library boundary selected by the approved dependency. PASS. |
| Remediation preservation | Latest-fix diff `29f7ae0ce8580cafc4873705b4c913e93bf7464f..5d9a3ddfad426eb545e866658a74428df72265ef`; TASK-012-R1 preserved-behavior clause | R1 changes the OAuth grant/origin owners and documentation but does not add a launcher, feature, platform branch, setting, check, or test harness. The approved `webbrowser::open`, printed fallback, and `--no-browser` flow remain intact. PASS. |
| Standard-only complexity check | Approved spec revision 11, research recommendation T4, original task, and remediation non-goals | The candidate uses the selected widely adopted browser-opening library directly. It adds no Wyrd-specific escaping, shell policy, command configuration, per-platform wrapper, compatibility path, or permanent repository check. No platform mechanism is present that should be classified as DRIFT. PASS. |

## Prior-finding closure

Round 1's `PLAT-001` proposal concerned dependency-owned WSL behavior. The structured validation rejected it because the approved task and lead decision require `webbrowser 1.2.4`, accept the native-Windows target check, and prohibit a Wyrd WSL branch, replacement launcher, escaping layer, option, or harness. The current human direction expressly preserves that rejection. Source inspection found no new or changed Wyrd-owned path that would create a distinct platform defect.

## Material proposed findings

None.

## Verification assessment and limits

- `mise exec -- cargo check -p webbrowser --target x86_64-pc-windows-msvc` was rerun during this review and exited 0.
- `mise exec -- cargo tree -i webbrowser -e normal --workspace --locked` resolved `webbrowser v1.2.4` through `wyrd-cli`; no sibling production owner appeared.
- `git diff --check adf349081077b3cfe0d56ab9a665cf01e2d86da4..5d9a3ddfad426eb545e866658a74428df72265ef` exited 0.
- The host does not have a Windows C cross-compiler. Per the locked decision, that does not make the review incomplete: the required Windows proof is the passing `webbrowser` Windows-target check, not a full `wyrd-cli` target build.
- No full identity journey, language sweep, or aggregate was run or required. The task records the filtered CLI journey for `--no-browser`; source inspection covers the launch and fallback branches. Broader journeys remain reserved for change review.

## Overall result

**PASS**

Proposed finding IDs: none.

# Platform domain review

## Immutable subject

- Repository: `/home/thorrester/Documents/GitHub/wyrd-oidc-mainline`
- Base: `adf349081077b3cfe0d56ab9a665cf01e2d86da4`
- Candidate: `29f7ae0ce8580cafc4873705b4c913e93bf7464f`
- Approved authority: `changes/active/oidc-production-readiness/spec.md`, revision 11
- Task: `changes/active/oidc-production-readiness/tasks/TASK-012-client-oauth2.md`
- Candidate identity was rechecked at the end of source inspection and remained unchanged.

## Reviewed boundary and source coverage

This pass reviewed the cross-platform device-login browser-launch boundary only. It traced the URL from the `oauth2` device response through `LoginFlow::run`, the `--no-browser` branch, launch-error fallback, workspace and CLI manifests, the resolved lockfile, and the target-specific implementation selected by the exact `webbrowser 1.2.4` dependency.

| Boundary | Authority and source inspected | Result |
|---|---|---|
| Wyrd call site and fallback | REQ-011; TASK-012 Scenario 4 and FIND-TASK-004-14; `crates/wyrd/wyrd-cli/src/auth/login.rs:84-125,150-155` | `webbrowser::open` receives the parsed device verification URI. The URL is printed first, `--no-browser` skips the call, and an `Err` prints the manual-open fallback. PASS. |
| Dependency and portability shape | Root `Cargo.toml:101-106`; `crates/wyrd/wyrd-cli/Cargo.toml:42-49`; `Cargo.lock` package `webbrowser 1.2.4`; upstream `webbrowser-1.2.4/Cargo.toml` | The version is pinned exactly, the CLI consumes the workspace dependency directly, and the lockfile contains the crate's target-specific Android, Apple, and WASM dependencies. There is no Wyrd feature, command setting, wrapper, or alternate launcher. PASS. |
| Native Windows | Upstream `webbrowser-1.2.4/src/windows.rs`, especially `open_browser_internal` and `get_browser_cmd` | The crate queries the registered HTTP association with `AssocQueryStringW`, tokenizes the registered application command, substitutes the URL as one `std::process::Command` argument, and does not invoke `cmd.exe`. Recorded `cargo check -p webbrowser --target x86_64-pc-windows-msvc` evidence and its emitted target artifact cover the lead-approved Windows proof. PASS. |
| macOS and Apple mobile | Upstream `webbrowser-1.2.4/src/macos.rs` and `src/ios.rs` | macOS uses `NSWorkspace`; iOS/tvOS/visionOS use `UIApplication`/`NSURL`. No Wyrd platform branch exists. PASS. |
| Android and WASM | Upstream `webbrowser-1.2.4/src/android.rs` and `src/wasm.rs` | Android uses an `ACTION_VIEW` intent (with the documented Termux command fallback); WASM uses `window.open`. These are dependency-owned conventional platform paths. PASS. |
| Unix desktop and WSL | Upstream `webbrowser-1.2.4/src/unix.rs`, including `open_browser_default`, `guess_desktop_env`, and `try_wsl`; `url-2.5.8/src/parser.rs:44-46` | Ordinary Unix candidates are spawned with the URL as an argument. The WSL fallback is materially different: it invokes `cmd.exe /c start` and then PowerShell. FAIL under `PLAT-001`. |
| Removed Wyrd launcher | Cumulative base-to-candidate diff for `crates/wyrd/wyrd-cli/src/auth/login.rs`; repository search across `wyrd-cli` and `wyrd-client` | The per-OS `browser_command`/`open_in_browser` implementation and its platform command test are deleted. The only Wyrd launch call is `webbrowser::open`. PASS. |

## Material proposed finding

### `PLAT-001` — INCORRECT — the all-platform no-command-interpreter guarantee is false on WSL

- **Violated obligation:** TASK-012 Scenario 4 requires the verification URL to open "on every platform with no command interpreter," and the changed rustdoc at `crates/wyrd/wyrd-cli/src/auth/login.rs:87-90` repeats that `webbrowser` uses the platform URL handler and "never a command interpreter."
- **Exact location:** `crates/wyrd/wyrd-cli/src/auth/login.rs:87-90,115`; dependency implementation `webbrowser 1.2.4` `src/unix.rs:241-261`.
- **Evidence:** when its Unix desktop detection resolves WSL, `webbrowser 1.2.4` calls `cmd.exe`, `"/c"`, `"start"`, followed by the URL, and falls back to `powershell.exe Start`. Its `cmd.exe` preparation escapes `^` and `&`, but not `|`. The resolved `url 2.5.8` query encode set escapes controls, space, quote, `#`, `<`, `>`, and apostrophe for special URLs, but does not encode `|`; therefore a parsed HTTP(S) verification URI does not make the task's claimed interpreter boundary true. This path is library-owned but reachable for the supported WSL platform.
- **Observable consequence:** Wyrd's default browser-open path crosses a command interpreter on WSL, contrary to the task and the changed source documentation. The native Windows finding is closed, but the broader all-platform claim is not.
- **Required correction boundary:** do not add a Wyrd-specific launcher, shell escaping, configurable command, platform check, or fake launcher test. Those would contradict the approved `webbrowser` decision and the standing requirement to use standard, well-vetted behavior. Resolve the mismatch at the authority/dependency boundary: either use a vetted `webbrowser` release whose supported WSL HTTP path no longer invokes an interpreter, with the same narrow target/source proof, or revise the task-level all-platform guarantee to the standard behavior actually supplied by the approved library and remove the false `never a command interpreter` rustdoc claim. The current exact `webbrowser 1.2.4` requirement and the current all-platform guarantee cannot both be demonstrated from the dependency's source.
- **Focused closure proof:** source inspection plus the narrow target check for the replacement library/version, or approved authority text explicitly limiting the guarantee to the native Windows path already proven. No full CLI target build, journey sweep, aggregate, source-grep gate, or Wyrd launcher harness is warranted.

## Verification assessment and limits

- No Cargo or mise command was started during this parallel review. I relied on the task's recorded green evidence and inspected the emitted `target/x86_64-pc-windows-msvc/.../libwebbrowser` metadata artifact.
- Per lead direction, the accepted native-Windows proof is `cargo check -p webbrowser --target x86_64-pc-windows-msvc`; the unavailable Windows C cross-compiler is not a limitation and a full `wyrd-cli` Windows-target build is not required.
- The recorded CLI journey uses `--no-browser`; it proves the skip branch but intentionally does not exercise a real desktop launch or launch failure. Source inspection proves the Wyrd control flow for the fallback.
- Native Windows, macOS, mobile, and WASM launch APIs were not executed on this Linux host. Their selected dependency implementations and target dependencies were inspected. Only the WSL implementation contradicts an explicit task guarantee.

## Overall result

**FAIL**

Proposed finding IDs: `PLAT-001`.

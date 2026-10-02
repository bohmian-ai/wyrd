# Focused follow-up review — Python tenant projection

## Immutable subject

- Base: `06f134dc14164c040c0e5014d21de29c240f4116`
- Candidate: `7996daaab9788f5d5c8fd2a40fb3bbf126aa0d84`
- Authority: approved `changes/active/oidc-production-readiness/spec.md`
  revision 7 and `changes/active/oidc-production-readiness/tasks/TASK-004-laptop-clients.md`
- Conflict reviewed: `standards-review.md` reports Python generated/public
  declaration parity PASS, while `maintainer-review.md` reports that the new
  public `tenant` option is absent from shipped `.pyi` declarations.

The candidate remained checked out at the stated commit. This pass inspected
only the disputed Python projection and did not modify candidate source.

## Source path inspected

- Runtime PyO3 signatures:
  `sdks/wyrd-sdk-python/src/{client.rs,bifrost/mod.rs,state/mod.rs,gateway.rs,operators.rs,verification.rs}`
- Public Python wrapper:
  `sdks/wyrd-sdk-python/python/wyrd/bifrost/__init__.py`
- Hand-authored declaration sources:
  `sdks/wyrd-sdk-python/python/wyrd/stubs/{client,bifrost,state,cards,gateway,operators,verification}.pyi`
- Assembled public declarations:
  `sdks/wyrd-sdk-python/python/wyrd/{client,bifrost,state,cards,gateway,operators,verification}/__init__.pyi`
  and the root `wyrd/__init__.pyi`
- Public package imports:
  `sdks/wyrd-sdk-python/python/wyrd/{__init__.py,client,bifrost,state,cards,gateway,operators,verification}`
- Stub assembler and gates:
  `sdks/wyrd-sdk-python/scripts/assemble_stubs.py` and `mise.toml`
- Existing public typing coverage and the TASK-004 Python journey:
  `sdks/wyrd-sdk-python/tests/` and
  `tests/integration/auth/test_saved_user_auth.py`

## Evidence resolving the conflict

1. The native signatures do contain the requested selector. `tenant=None` is
   present on `WyrdClient` (`src/client.rs:48-55`), native
   `TableConfig.describe` and `Bifrost` (`src/bifrost/mod.rs:185-195,307-330`),
   `WyrdState.start_bifrost` and `Cards`
   (`src/state/mod.rs:159-170,1596-1608`), `Gateway`
   (`src/gateway.rs:131-139`), `OperatorConnections`
   (`src/operators.rs:68-75`), and `Verification`
   (`src/verification.rs:40-47`). Runtime introspection confirms it on the
   directly exported native classes `WyrdClient`, `WyrdState.start_bifrost`,
   `Cards`, `Gateway`, `OperatorConnections`, and `Verification`.

2. Every disputed hand-authored declaration source omits that parameter:
   `stubs/client.pyi:15-19`, `stubs/bifrost.pyi:164-168,230-236`,
   `stubs/state.pyi:151-156`, `stubs/cards.pyi:245`,
   `stubs/gateway.pyi:244`, `stubs/operators.pyi:152`, and
   `stubs/verification.pyi:14`. The corresponding shipped public declarations
   repeat those omissions at `client/__init__.pyi:17-21`,
   `bifrost/__init__.pyi:166-170,232-238`,
   `state/__init__.pyi:153-158`, `cards/__init__.pyi:247`,
   `gateway/__init__.pyi:246`, `operators/__init__.pyi:154`, and
   `verification/__init__.pyi:16`. `WyrdClient` and `Cards` are also exported
   from the root public package (`wyrd/__init__.pyi:23-25,58,95`).

3. The public Bifrost Python wrapper has a second, runtime-visible omission.
   `TableConfig.describe` accepts and forwards only `table`, `server_url`,
   `credential`, and `grpc_url`
   (`python/wyrd/bifrost/__init__.py:246-261`). `_BifrostBase.__init__`, inherited
   by both `Bifrost` and `AsyncBifrost`, likewise accepts and forwards no
   `tenant` (`:408-447`). On the built candidate:

   - `Bifrost(tenant="tenant-a")` raises `TypeError: _BifrostBase.__init__()
     got an unexpected keyword argument 'tenant'`.
   - `TableConfig.describe("x.y", tenant="tenant-a")` raises `TypeError:
     TableConfig.describe() got an unexpected keyword argument 'tenant'`.

   Thus this is not only declaration drift for those two entry points; the
   required public runtime option is unreachable even though the private native
   layer accepts it.

4. The impact is reachable through ordinary supported imports. `WyrdClient`,
   `Cards`, and `WyrdState` are root exports; all other affected handles are
   exported by their public `wyrd.<module>` packages. The TASK-004 journey
   itself calls `Cards(..., tenant=...)` seven times. Running the repository's
   installed `ty` directly on that journey reports seven
   `unknown-argument` diagnostics against the shipped
   `wyrd/cards/__init__.pyi:247`, although the same calls execute at runtime.

5. `codegen:check` can pass despite the mismatch. The assembler reads the
   hand-authored `.pyi` files and copies them into the public package; it does
   not inspect PyO3 signatures (`scripts/assemble_stubs.py:202-211`). For all
   seven disputed modules, the assembled output is byte-for-byte the source
   stub after removing the two generated header lines. `codegen:check` hashes
   only generated `.pyi` outputs, reruns that assembler, and compares the two
   hashes (`mise.toml:1511-1532`), so stale source declarations deterministically
   regenerate the same stale public declarations and satisfy the check.

6. The normal `py:typecheck` lane also does not close this gap. Its explicit
   input list (`mise.toml:1196-1200`) omits
   `tests/integration/auth/test_saved_user_auth.py`, which is the changed Python
   consumer using `tenant`. Checking that file directly produces the seven
   diagnostics above. Merely parsing/type-checking the declaration files cannot
   compare them with the separate PyO3 implementation.

## Proposed finding

### FUP-TASK-004-1 — The public Python tenant projection is incomplete

- **Classification:** MISSING / VIOLATION
- **Violated obligation:** TASK-004 lines 60-72 require Python constructors to
  expose the same optional tenant selector and update public types/stubs from
  source; AGENTS.md section 8 requires the Rust binding, public package export,
  generated stubs, and Python tests to agree.
- **Location:** the public Bifrost wrapper and the seven hand-authored stub
  sources listed above.
- **Evidence:** native bindings accept `tenant`; shipped declarations omit it;
  public Bifrost entry points reject it at runtime; direct typing of the
  candidate's own Python journey fails seven times; the assembler and gate only
  preserve the stale declaration source.
- **Observable consequence:** editors and static checking reject valid tenant
  selection for directly exported native handles, while `Bifrost`,
  `AsyncBifrost`, and `TableConfig.describe` cannot select a saved login by
  tenant at all through their public Python APIs. A same-server multi-tenant
  user therefore cannot reliably use the documented Python surface required by
  the task.
- **Testable correction:** add the existing `tenant: str | None` option to the
  public Bifrost wrapper entry points and forward it to their existing native
  owners; update the existing hand-authored declarations and constructor docs
  for every affected public entry point; regenerate public stubs. Add the
  smallest typing proof using the public imports with `tenant` (including
  `Cards` and the Bifrost wrapper) and run it in `py:typecheck`; retain the
  existing runtime journey and codegen check. No new generator, credential
  owner, or language-specific authentication path is needed.

## Resolution

**RESOLVED.** `maintainer-review.md` is substantively correct that declaration
parity is broken, while `standards-review.md`'s Python declaration-parity PASS
is disproved. The finding is revised to include the additional public Bifrost
runtime projection gap rather than treating the whole defect as `.pyi` drift.

# Focused follow-up: concurrent Python telemetry installation

Result: **RESOLVED**. INV-001 identifies a reachable partial-install failure, but does not establish a material violation of the approved startup/recovery contract. Recommend **REJECTED** as a blocking proposal. No new material proposal IDs.

## Subject and scope

Candidate `480251a70f799b8581ca5a4be77983d6f345d5e3`; prior candidate `634e19cbc517293ed2458f30ce73401714f3447d`. HEAD and both approved authority hashes matched `review-scope.md` during this follow-up. Read committed repository source through `git show`; the installer and native state files had no working-tree changes. Read the invariant and system reports as competing questions, and traced only their shared uncertainty. No other finding or acceptance boundary was reviewed.

Applicable authority: approved positioning revision 6 REQ-017/AC-016; approved service-identity-and-telemetry revision 17 REQ-050, the two telemetry setup paths and AC-026; candidate `architecture/wyrd-design.md:717–756`; root/Python rules; spec-driven authority ordering and task-review materiality/reopening rules.

## Source evidence

| Owner or dependency | Evidence and effect |
|---|---|
| Python native state, `src/state/mod.rs:78–85,228–271` | `telemetry_setup` belongs to each state. It serializes that state's check/install/publication and permits idempotent private startup. It cannot serialize two independently loaded states. A Python `None` install result becomes `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS`; the managed handle is stored only after a non-None result. |
| Native `configure`, `src/state/mod.rs:333–351` | Native context freezes before managed installation. An installer error propagates; there is no success return or stored managed handle on refusal. The remediation adds this call/default; it does not change the installer body. |
| Python installer, `python/wyrd/otel.py:348–382` | Checks all three globals, constructs its own providers, makes three separate global setter calls, and requires identity equality for all three. On failed conjunction it shuts down its three local provider objects, returns None and never creates `_ManagedTelemetry`. It does not fetch a global provider and shut that fetched object down. |
| Correlation owner, `python/wyrd/otel.py:96–97,142–170` | `_INSTALL_LOCK` protects processor registration, not process-global provider publication. It is acquired during installation, so wrapping setup in the same non-reentrant lock would deadlock. |
| Native flush/shutdown, `src/state/mod.rs:424–451,667–686` | These call only the stored managed handle, if present, then the state's native lifecycle. Normal shutdown does not shut down SDK providers. A refused state has no handle with which to flush another installation. |
| ASGI startup, `python/wyrd/integrations/asgi/_middleware.py:104–134` | Startup runs in `asyncio.to_thread`. A private installer failure drains that state's writer and propagates as lifespan startup failure. This establishes a real threaded setup path, not a guarantee that a failed lifespan may continue serving. |
| Candidate test, `tests/unit/state/test_observe_surface.py:784–855` | Covers same-state overlap, sequential other-state/application refusal, application meter refusal, and simulated lost tracer registration. It does not establish atomic three-signal cross-state publication. Its absence of a cross-state test does not itself establish an unmet acceptance obligation. |

Inspected the installed OpenTelemetry API **1.42.1**, matching candidate `uv.lock`'s pinned API version, rather than assuming setter semantics. Under `sdks/wyrd-sdk-python/.venv/lib/python3.12/site-packages/`: `opentelemetry/trace/__init__.py:548–565`, `metrics/_internal/__init__.py:841–861`, and `_logs/_internal/__init__.py:406–423` each use a distinct `Once` to publish exactly one global provider. `opentelemetry/util/_once.py:18–36` locks and executes the assignment once; later calls cannot displace it. Meter installation additionally connects proxies to that same provider. These are installed dependency sources, not dirty repository candidate evidence.

## Resolved interleavings

The split install in INV-001 is possible. Both states may pass the proxy preflight before either publishes, and construction creates workers/uses Python synchronization, so the GIL is not a transaction around the complete installer. One valid order is:

1. A publishes tracer; B's tracer setter loses.
2. B publishes meter; A's meter setter loses.
3. A publishes logger; B's logger setter loses.
4. Each final conjunction fails. Each shuts down its own constructed providers, including the subsets it published, and returns None. Both public configure calls raise; neither stores a managed handle.

The remaining globals reference those failed attempts' shut-down providers. Continuing to rely on global export in that process is unsafe. The system report's ownership observation is correct (cleanup targets local objects), but that does not mean the losing attempt could not have published some of those objects. Conversely, the invariant report's process-global loss observation does not prove an already successful installation is harmed.

**A successfully installed Wyrd state cannot be harmed by this path.** For A to return a managed handle, every global must be A's provider at its identity check. Each setter is once-only. B can then neither replace any of those globals nor pass its own conjunction. Its cleanup list contains B's provider objects, not A's. If B had already won any global, A's conjunction could not succeed. Thus split publication and successful Wyrd installation are mutually exclusive, including when B began construction before A completed. A state's earlier telemetry-off configure is a context-only operation and supplies no managed process provider to damage.

**An existing application provider object is not replaced, flushed or shut down.** If present at preflight, setup immediately refuses. If the application wins one setter after Wyrd's preflight, Once prevents Wyrd from displacing that object, and Wyrd's cleanup list still contains only its own constructed objects. A concurrently initializing application can encounter occupied other-signal globals from a failed Wyrd attempt; that is another partial process initialization requiring recovery, not proof that a previously successful application pipeline was taken down. Application-owned providers passed directly to instrumentation retain their own objects. No claim of atomic application initialization is needed for this conclusion.

## Authority and materiality

The approved revision-17 contract explicitly says that a failed install leaves context fixed, subsequent configure refuses, and the caller corrects code/dependencies and **restarts the process** (`spec.md:491–493`). It requires refusal of incompatible startup and preservation of application-owned providers (`:515–522`), and allows native state/Run contexts to coexist without multiple ambient defaults. It does not promise that simultaneous incompatible managed installs elect one successful state, preserve a usable partial set of globals after both fail, or support continuing process operation after failed managed initialization. AC-016 requires default installation, existing-provider refusal and missing-package failure; AC-026 protects an existing application provider. The traced behavior meets those boundaries in the scenario at issue: both attempts explicitly fail, no successful managed owner exists, and another owner's objects are untouched.

The meaningful additional consequence asserted by INV-001 is loss of telemetry if the application continues after these setup errors. That continuation conflicts with the approved recovery procedure. There is no source-proven path here to take down a successfully configured managed sibling or an established application provider. The startup race is real, but reachability and irreversible globals alone do not establish a supported availability regression under this approved failure contract. The generic coexistence requirement does not authorize multiple ambient telemetry defaults.

The new default configure composition justified inspecting the unchanged installer and its consumers; it does not automatically make every old installer weakness a new blocker. Having traced the specific gap, the evidence does not meet the material reopening threshold. This rejects the blocking proposal rather than carrying it as optional remediation.

## Correction boundary

A process-wide lock around the existing Wyrd check/create/install/publication transaction could make competing Wyrd attempts choose one successful installation. It would preserve current public behavior and can be an ordinary reversible implementation choice; it would not require a new provider pipeline or reset protocol. It would not serialize arbitrary application-owned OTel setter calls, and reusing `_INSTALL_LOCK` without accounting for its nested acquisition would deadlock.

However, prescribing that lock as required remediation would add an unsupported acceptance promise that simultaneous incompatible Wyrd startups must produce a winner. Requiring complete atomicity with independent application startup, rollback, or continued operation after failed installation would add a concurrency/recovery decision beyond this task. None is necessary to meet the currently approved obligation. No specification revision is required to resolve this review: reject INV-001 as a blocker; any desired stronger startup guarantee should first be explicitly adopted rather than inferred here.

No tests, reproduction, Cargo commands or dependency changes were needed; source identity and the once-only setter proof resolve the uncertainty. No source was changed. Risk remains limited to continuing a process after managed installation has failed, for which approved recovery already requires restart.

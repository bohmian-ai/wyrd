Add Wyrd to this project and confirm one verification end to end. A person
started a local Wyrd server with `wyrd init` and connected you to its MCP
server. Do not start, stop, or restart that server.

Follow these steps in order. Stop at the first step that fails and report the
step, the Wyrd error `code` when one is printed or returned, otherwise the
tool's own diagnostic. Never print, copy, or write a credential, and never
write a machine-specific path into a project file.

1. Confirm the server. Call the Wyrd MCP tool `wyrd.guide` with
   `{"topic": "start"}`, then `cards.list` with `{"limit": 1}`. Both must
   succeed. If no Wyrd MCP tool is available, ask the person to connect this
   host with `wyrd mcp install --host <host>` and reload it.

2. Choose the language. Read the project manifest in the current directory:
   `pyproject.toml` is Python, `Cargo.toml` is Rust, and `package.json` is
   TypeScript. When none or more than one is present, or the project uses
   another language, ask the person to choose Python, Rust, or TypeScript
   before changing any file.

3. Install the SDK at the release of the `wyrd` CLI. Run `wyrd --version`;
   it prints `wyrd <version>`. Install exactly that version with the
   project's own package tool:

   - Python: `uv add "wyrd-ai==<version>"`
   - Rust: `cargo add wyrd-ai@<version> tokio --features tokio/full`
   - TypeScript: `npm install wyrd-ai@<version>`

   If that exact version is unavailable, stop and report it. Never install
   another version.

4. Write the example. Before writing, check whether `support-reply.yaml`,
   `no-refund-promise.yaml`, or the program file already exists. If one does
   and its content differs, show the difference and ask before replacing it.
   Leave every other project file unchanged.

   Save this Verifier as `no-refund-promise.yaml`:

   ```yaml
   apiVersion: wyrd/v1
   kind: Verifier
   metadata:
     name: no-refund-promise
     version: 1.0.0
     space: default
   spec:
     inputs:
       answer: { type: json }
     requirements:
       - id: no-refund-promise
         statement: The answer never promises a guaranteed refund
         check:
           kind: assert
           path: inputs.answer
           operator: not_contains
           expected: guaranteed refund
   ```

   Save this Service beside it as `support-reply.yaml`:

   ```yaml
   apiVersion: wyrd/v1
   kind: Service
   metadata:
     name: support-reply
     version: 1.0.0
     space: default
   spec:
     verified_by:
       - verifier: ./no-refund-promise.yaml
   ```

   Write the program for the chosen language from the "Run your first
   verification" page of the Wyrd documentation for this release
   (`get-started/quickstart/` in `llms-full.txt`): `quickstart.py` for
   Python, `src/main.rs` for Rust, or `quickstart.ts` for TypeScript. It
   declares the Service with `WyrdState.declare("support-reply.yaml")`, opens
   one Run, and verifies two answers with
   `run.observe.verify("no-refund-promise", ...)`. It needs no model or
   provider key.

5. Run it from the directory holding both YAML files, with the project's own
   tool: `uv run quickstart.py`, `cargo run`, or `node quickstart.ts`. It
   prints one `passed` and one `failed` verdict, each naming its subject as
   `Service/support-reply@1.0.0#<uid>`. A `failed` verdict is a
   result, not an error.

6. Confirm the stored evidence through MCP. Call `cards.get` with
   `{"card_ref": {"kind": "Service", "space": "default", "name":
   "support-reply", "version": "1.0.0"}}` and read its `uid`. Then call
   `bifrost.query` with this SQL, replacing `<uid>`:

   ```sql
   SELECT result_id, run_id, card_uid, verdict FROM vala.verification.results WHERE card_uid = '<uid>' ORDER BY ended_at DESC LIMIT 20
   ```

   Rows become queryable shortly after the server publishes them; repeat the
   query for up to a minute before reporting none. Done means at least one
   `passed` and one `failed` row with the same `run_id`.

7. Report the Service identity
   `Service/support-reply@1.0.0#<uid>`, the `run_id`, and both
   verdicts. Then point the person to the Wyrd documentation's
   "Choose your path" page for the next step.

#!/usr/bin/env python3
"""Generate lightweight API reference pages from repository metadata."""

from __future__ import annotations

import json
import re
from pathlib import Path


def md_escape(value: str) -> str:
    value = re.sub(r"[\n\r\t\x00-\x1f]", " ", value)
    value = value.replace("|", r"\|")
    value = value.replace("`", r"\`")
    value = value.replace("<", "&lt;").replace(">", "&gt;")
    return value


DOCS_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = DOCS_ROOT.parent
OPENAPI_PATH = REPO_ROOT / "openapi.yaml"
SCHEMA_DIR = REPO_ROOT / "crates" / "wyrd-spec" / "schemas"
OUT_DIR = DOCS_ROOT / "src" / "content" / "docs" / "api"


def yaml_value(key: str, text: str) -> str | None:
    match = re.search(rf"^\s*{re.escape(key)}:\s*(.+?)\s*$", text, re.MULTILINE)
    if not match:
        return None
    return match.group(1).strip().strip('"')


def route_lines(text: str) -> list[str]:
    lines = text.splitlines()
    in_paths = False
    routes: list[str] = []
    for line in lines:
        if line.startswith("paths:"):
            in_paths = True
            continue
        if in_paths and line and not line.startswith(" "):
            break
        match = re.match(r"^\s{2}(/[^:]+):\s*$", line)
        if match:
            routes.append(match.group(1))
    return routes


def render_openapi() -> str:
    text = OPENAPI_PATH.read_text(encoding="utf-8") if OPENAPI_PATH.exists() else ""
    title = yaml_value("title", text) or "Wyrd API"
    version = yaml_value("version", text) or "unversioned"
    routes = route_lines(text)

    lines = [
        "---",
        "title: OpenAPI",
        "description: Generated summary of the Wyrd OpenAPI contract.",
        "pillar: wyrd",
        "group: api",
        "order: 2",
        "---",
        "",
        "# OpenAPI",
        "",
        f"The repository OpenAPI document is `{md_escape(str(OPENAPI_PATH.relative_to(REPO_ROOT)))}`. Its current title is `{md_escape(title)}` and its version is `{md_escape(version)}`.",
        "",
        "## Routes",
        "",
    ]
    if routes:
        lines.extend(f"- `{md_escape(route)}`" for route in routes)
    else:
        lines.append("No HTTP routes are published in the current OpenAPI document.")

    lines.extend(
        [
            "",
            "## Refresh",
            "",
            "Run `mise run codegen:check` to verify generated API metadata and `mise run docs:generate` to refresh this page.",
            "",
        ]
    )
    return "\n".join(lines)


def render_schemas() -> str:
    schema_files = sorted(SCHEMA_DIR.glob("*.json"))
    schema_dir = SCHEMA_DIR.relative_to(REPO_ROOT)
    # Emit data rows (filename + type title); the shared DataTable component owns
    # the presentation. The directory prefix is stated once in the intro, so rows
    # carry only the filename.
    columns = [
        {"header": "Schema", "role": "name"},
        {"header": "Title", "role": "type"},
    ]
    rows = []
    for schema_path in schema_files:
        try:
            schema = json.loads(schema_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            schema = {}
        title = schema.get("title") or schema_path.stem
        rows.append([schema_path.name, str(title)])

    lines = [
        "---",
        "title: Schemas",
        "description: Generated inventory of Wyrd JSON Schemas.",
        "pillar: wyrd",
        "group: api",
        "order: 3",
        "---",
        "",
        "# Schemas",
        "",
        f"These schemas are generated from the Wyrd spec crate and checked into the repository under `{schema_dir}/` for clients, docs, and agents. `architecture/wyrd-design.md` remains the design authority; this inventory may include implementation drift while contracts are being reconciled.",
        "",
        f"<DataTable columns={{{json.dumps(columns)}}} rows={{{json.dumps(rows)}}} />",
        "",
    ]
    return "\n".join(lines)


def render_errors() -> str:
    return "\n".join(
        [
            "---",
            "title: Errors",
            "description: Error handling guidance for Wyrd API clients and agents.",
            "pillar: wyrd",
            "group: api",
            "order: 1",
            "---",
            "",
            "# Errors",
            "",
            "Wyrd returns errors as structured RFC 7807 Problem Details objects. Agents and SDK clients must preserve the full structure and must not collapse errors into prose.",
            "",
            "This page is the generated error catalog. For how an agent should act on these errors, see [Error remediation](/agents/error-remediation/).",
            "",
            "## Client transport codes",
            "",
            "| Code | Status | Title | Remediation |",
            "|---|---:|---|---|",
            "| `WYRD_CLIENT_400_CONFIG_INVALID` | 400 | Client transport configuration failed validation | Fix the `details.field` named on the error, then reconstruct the client. Not a retry path. |",
            "| `WYRD_CLIENT_401_NO_CREDENTIALS` | 401 | Credential chain produced no usable credential | Set `WYRD_ACCESS_TOKEN`, `WYRD_WORKLOAD_TOKEN`+`WYRD_TENANT`, or `WYRD_API_KEY`. Not a retry path. |",
            "| `WYRD_CLIENT_503_TRANSPORT_DOWN` | 503 | Client transport is unavailable | Retry with backoff once the network or server health recovers. |",
            "",
            "## Error fields",
            "",
            "| Field | Description |",
            "| --- | --- |",
            "| `type` | URI identifying the error class |",
            "| `title` | Human-readable summary of the error class (stable across instances) |",
            "| `status` | HTTP status code (integer) |",
            "| `code` | Wyrd-specific machine-readable error code (string, stable) |",
            "| `detail` | Instance-specific description of what went wrong |",
            "| `remediation` | Actionable guidance for the caller or agent |",
            "| `context` / `details` | Structured key/value map with operation-specific fields |",
            "| `request_id` | Present when the server can correlate the request |",
            "| `trace_id` | Present when distributed tracing is active |",
            "| `instance` | URI identifying the specific resource or operation that failed (when applicable) |",
            "",
            "## Agent handling",
            "",
            "- Preserve all structured fields; do not collapse errors into prose.",
            "- Use `code` for programmatic branching, not `title` or `detail` (those are for humans).",
            "- Surface policy and audit failures as decisions (not generic transport failures).",
            "- Keep retries bounded and tied to idempotent operations.",
            "- Link remediation steps to the card or run that caused the error.",
            "",
            "## Skald agent codes",
            "",
            "| Code | Status | When |",
            "| --- | --- | --- |",
            "| `SKALD_AGENT_404_PROVIDER` | 404 | The provider registry returned no client for the agent's `ProviderName`. Bind the missing provider before constructing the agent, or correct the agent def. |",
            "| `SKALD_AGENT_404_TOOL` | 404 | The provider response requested a tool name not registered for the agent. Add the tool to the agent's `ToolRegistry`, or remove the tool reference from the agent def. |",
            "| `SKALD_AGENT_409_PROVIDER_MISMATCH` | 409 | `Agent::run_prompt` received prompt messages for a different provider than the agent's bound provider. Rebuild the prompt for the agent provider before running it. |",
            "| `SKALD_AGENT_422_TOOL_ARGS` | 422 | A dispatched tool rejected the provider-emitted arguments. Inspect the tool's input schema and the response payload. |",
            "| `SKALD_AGENT_422_SYSTEM_PROMPT` | 422 | The agent's system prompt failed to shape into native messages for the chosen provider. Custom providers must author the system content into the prompt body directly. |",
            "| `SKALD_AGENT_422_PROMPT` | 422 | `Agent::run_prompt` received a prompt that cannot be sent by the agent's provider. Validate the prompt body and provider before dispatch. |",
            "| `SKALD_AGENT_500_MAX_ITERATIONS` | 500 | The bounded tool loop exceeded `RunConfig::max_iterations`. Raise the cap or fix the agent and tool set so the model terminates. |",
            "| `SKALD_AGENT_502_PROVIDER` | 502 | The underlying `skald-runtime` provider call failed. The chained `SKALD_RUNTIME_*` code identifies the transport or decode failure. |",
            "",
            "## Skald workflow codes",
            "",
            "| Code | Status | When |",
            "| --- | --- | --- |",
            "| `WYRD_WORKFLOW_422_CYCLE` | 422 | The workflow DAG contains a cycle. |",
            "| `WYRD_WORKFLOW_422_MISSING_DEPENDENCY` | 422 | A step depends on an id that does not exist. |",
            "| `WYRD_WORKFLOW_422_DUPLICATE_STEP_ID` | 422 | Two steps share the same id. |",
            "| `WYRD_WORKFLOW_422_MISSING_NAME` | 422 | `save` or `to_card` was called on an anonymous workflow. |",
            "| `WYRD_WORKFLOW_422_MISSING_PARAMETER` | 422 | A prompt placeholder exists in neither workflow input nor upstream parameters. |",
            "",
            "## Prompt and CLI authoring codes",
            "",
            "| Code | Status | When |",
            "| --- | --- | --- |",
            "| `WYRD_AGENT_422_STRUCTURED_DECODE` | 422 | Response text is not valid JSON when `Prompt.output` is set. |",
            "| `WYRD_PROMPT_422_PYDANTIC_REQUIRED` | 422 | Pydantic is required for class-based `Prompt(output=Model)` schemas. |",
            "| `WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA` | 422 | The `Prompt(output=...)` dict cannot be reduced to JSON Schema. |",
            "| `WYRD_PROMPT_400_PROVIDER_MISMATCH` | 400 | A provider accessor was called on a different provider's response. |",
            "| `WYRD_CLI_400_CARD_EXTENSION_UNSUPPORTED` | 400 | A card file extension is not `.json`, `.yaml`, or `.yml`. |",
            "| `WYRD_CLI_500_IO` | 500 | Filesystem IO failed during a CLI operation. |",
            "",
            "## Registry codes",
            "",
            "| Code | Status | When |",
            "| --- | --- | --- |",
            "| `WYRD_REG_409_SPEC_DRIFT` | 409 | Re-apply with the same identity but a different `spec_hash`; same-version cards are immutable. |",
            "",
            "## Eval protocol codes",
            "",
            "| Code | Status | When |",
            "| --- | --- | --- |",
            "| `WYRD_EVAL_404_RUN_NOT_FOUND` | 404 | The `run_id` path parameter does not match any open run. Re-open with `POST /api/v1/eval/runs`. |",
            "| `WYRD_EVAL_401_MISSING_LEASE` | 401 | The request did not include an `Authorization: Bearer <token>` header. Send the `lease_token` from the open response. |",
            "| `WYRD_EVAL_403_INVALID_LEASE` | 403 | The bearer token does not match the lease minted for the run. Re-open the run; leases are bound to one run. |",
            "| `WYRD_EVAL_401_API_KEY_INVALID` | 401 | The server has `WYRD_API_KEY` set and the request did not send a matching bearer token. |",
            "| `WYRD_EVAL_409_SUBMISSION_MISMATCH` | 409 | The submission kind, scenario id, or turn counter does not match the outstanding directive. Call `/next` to retrieve the current directive and retry. |",
            "| `WYRD_EVAL_429_TOO_MANY_RUNS` | 429 | The server has reached its concurrent-run cap. Wait for an existing run to complete, then retry. |",
            "| `WYRD_EVAL_500_RUN_FAILED` | 500 | The eval engine, simulator, or scenario loader encountered an internal error. Inspect server logs for the chained detail. |",
            "| `WYRD_EVAL_500_RESULTS_PERSISTENCE_FAILED` | 500 | Results serialization or filesystem write failed after the run completed. Inspect server logs and available disk space. |",
            "",
        ]
    )


def main() -> None:
    OUT_DIR.mkdir(parents=True, exist_ok=True)
    (OUT_DIR / "openapi.md").write_text(render_openapi(), encoding="utf-8")
    (OUT_DIR / "schemas.md").write_text(render_schemas(), encoding="utf-8")
    (OUT_DIR / "errors.md").write_text(render_errors(), encoding="utf-8")


if __name__ == "__main__":
    main()

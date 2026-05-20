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
    lines = [
        "---",
        "title: Schemas",
        "description: Generated inventory of Wyrd JSON Schemas.",
        "---",
        "",
        "# Schemas",
        "",
        "These schemas are generated from the Wyrd spec crate and checked into the repository for clients, docs, and agents.",
        "",
        "| Schema | Title |",
        "| --- | --- |",
    ]

    for schema_path in schema_files:
        try:
            schema = json.loads(schema_path.read_text(encoding="utf-8"))
        except json.JSONDecodeError:
            schema = {}
        title = schema.get("title") or schema_path.stem
        rel = schema_path.relative_to(REPO_ROOT)
        lines.append(f"| `{md_escape(str(rel))}` | {md_escape(str(title))} |")

    lines.append("")
    return "\n".join(lines)


def render_errors() -> str:
    return "\n".join(
        [
            "---",
            "title: Errors",
            "description: Error handling guidance for Wyrd API clients and agents.",
            "---",
            "",
            "# Errors",
            "",
            "Wyrd returns errors as structured RFC 7807 Problem Details objects. Agents and SDK clients must preserve the full structure and must not collapse errors into prose.",
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
            "## Catalog",
            "",
            "The public error catalog is not emitted as a standalone file yet. Once that generator lands, this page will link to the full catalog with per-code remediation tables.",
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

#!/usr/bin/env python3
"""Refresh published API reference pages from repository metadata.

This only rewrites pages that already exist in the content tree; adding a
page's file opts it in.
"""

from __future__ import annotations

import re
from pathlib import Path


def md_escape(value: str) -> str:
    value = re.sub(r"[\n\r\t\x00-\x1f]", " ", value)
    value = value.replace("|", r"\|")
    # The docs style bans em dashes; a few source messages use them as pauses.
    value = value.replace(" \u2014 ", "; ")
    value = value.replace("<", "&lt;").replace(">", "&gt;")
    # A brace outside a code span opens a Svelte expression, so code-format
    # brace tokens such as `${media:name}`; code spans are escaped for Svelte.
    parts = value.split("`")
    parts[::2] = [re.sub(r"\$?\{+[^{}]*\}+", lambda m: f"`{m.group(0)}`", part) for part in parts[::2]]
    return "`".join(parts)


DOCS_ROOT = Path(__file__).resolve().parents[1]
REPO_ROOT = DOCS_ROOT.parent
OUT_DIR = DOCS_ROOT / "src" / "content" / "docs" / "api"

STRING = r'"((?:[^"\\]|\\.)*)"'
WYRD_ERROR = re.compile(r"#\[wyrd_error\((.*?)\)\]", re.S)


def rust_string(literal: str) -> str:
    """Undo Rust string escapes: line continuations, then escaped characters."""
    literal = re.sub(r"\\\n\s*", "", literal)
    return re.sub(r"\\(.)", r"\1", literal)


def error_catalog() -> dict[str, tuple[int, str, str]]:
    """Read every `#[wyrd_error(...)]` declared in crate and SDK sources.

    The derive attributes are the catalog's single source; reading them keeps
    this page from becoming a second, hand-maintained list. Test harness codes
    and attributes without literal metadata (doc examples, delegates) are skipped.
    """
    catalog: dict[str, tuple[int, str, str]] = {}
    for root in ("crates", "sdks"):
        for path in sorted((REPO_ROOT / root).glob("**/src/**/*.rs")):
            if "wyrd-error-derive" in path.parts:
                continue
            for body in WYRD_ERROR.findall(path.read_text(encoding="utf-8")):
                fields = {
                    key: re.search(rf"\b{key}\s*=\s*{pattern}", body, re.S)
                    for key, pattern in (("code", STRING), ("status", r"(\d+)"), ("title", STRING), ("remediation", STRING))
                }
                if not all(fields.values()):
                    continue
                code = fields["code"].group(1)
                if code.startswith("WYRD_TESTING_"):
                    continue
                catalog.setdefault(
                    code,
                    (int(fields["status"].group(1)), rust_string(fields["title"].group(1)), rust_string(fields["remediation"].group(1))),
                )
    return dict(sorted(catalog.items()))


def render_openapi() -> str:
    """Point at the one live OpenAPI document instead of restating it.

    The document is built by `utoipa` from the server's own handlers and served
    at runtime, so a checked-in copy would be a second contract that can drift.
    """
    return "\n".join(
        [
            "---",
            "title: HTTP API",
            "description: Where a Wyrd server serves its OpenAPI document and how to use it.",
            "pillar: wyrd",
            "section: Reference",
            "order: 22",
            "---",
            "",
            "Every Wyrd server serves its own OpenAPI 3.1 document at `GET /openapi.json`, without authentication. It is built from the routes that server mounts, so it always matches the version you are calling.",
            "",
            "```bash",
            "curl -s $WYRD_SERVER_URL/openapi.json > openapi.json",
            "```",
            "",
            "Load the file or the URL into any OpenAPI tool, such as Swagger UI, Redoc, Postman, or a client generator.",
            "",
            "Each operation declares its authentication, request and response bodies, and the Wyrd error codes it can return. Errors use `application/problem+json`; see [Error codes](/api/errors/).",
            "",
            "The Rust, Python, and TypeScript SDKs call this same API, so you only need it for another language or for raw HTTP.",
            "",
        ]
    )


def render_errors() -> str:
    lines = [
        "---",
        "title: Error codes",
        "description: Every stable Wyrd error code with its HTTP status, title, and fix.",
        "pillar: wyrd",
        "section: Reference",
        "order: 21",
        "---",
        "",
        "Every Wyrd error carries a stable `code`, such as `WYRD_SPEC_400_VALIDATION`: the area, the HTTP status, and the cause. Branch on `code`, never on the message. The same code reaches you over HTTP, MCP, the CLI, and every SDK.",
        "",
        "Over HTTP an error is an RFC 9457 `application/problem+json` body:",
        "",
        "| Field | Meaning |",
        "| --- | --- |",
        "| `code` | The stable code from the tables below |",
        "| `status` | The HTTP status |",
        "| `title` | A fixed summary of the code |",
        "| `detail` | What went wrong in this request |",
        "| `remediation` | What to do about it |",
        "| `details` | Fields specific to the error, such as `rule` and `path` for a validation failure |",
        "",
        "This page is generated from the error definitions in the Wyrd source.",
        "",
    ]
    groups: dict[str, list[tuple[str, int, str, str]]] = {}
    for code, (status, title, remediation) in error_catalog().items():
        groups.setdefault("_".join(code.split("_")[:2]), []).append((code, status, title, remediation))
    for prefix, rows in groups.items():
        lines += [f"## {prefix}", "", "| Code | Status | Title | Fix |", "| --- | ---: | --- | --- |"]
        lines += [f"| `{code}` | {status} | {md_escape(title)} | {md_escape(fix)} |" for code, status, title, fix in rows]
        lines.append("")
    return "\n".join(lines)


def main() -> None:
    pages = {"openapi.md": render_openapi, "errors.md": render_errors}
    for name, render in pages.items():
        page = OUT_DIR / name
        if page.exists():
            page.write_text(render(), encoding="utf-8")


if __name__ == "__main__":
    main()

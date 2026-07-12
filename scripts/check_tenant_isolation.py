#!/usr/bin/env python3
"""Deterministic SQL foundation isolation checks."""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]

WYRD_SQL_MIGRATIONS = ROOT / "crates/wyrd/wyrd-sql/migrations"
VALA_SQL_MIGRATIONS = ROOT / "crates/vala/vala-sql/migrations"
WYRD_QUERIES = ROOT / "crates/wyrd/wyrd-sql/src/queries"
VALA_QUERIES = ROOT / "crates/vala/vala-sql/src/queries"
WYRD_SERVER = ROOT / "crates/wyrd/wyrd-server/src"
SQL_CRATES = [
    ROOT / "crates/wyrd/wyrd-sql",
    ROOT / "crates/vala/vala-sql",
]

PLATFORM_QUERY_ALLOWLIST = {
    "crates/wyrd/wyrd-sql/src/queries/platform/tenant_resolver.rs",
}

# Files under queries/platform/ whose public async fns may take TenantConn
# instead of PgPool/Transaction. These are cross-domain helpers that delegate
# tenant-scoped writes to tenant-shaped APIs.
PLATFORM_EXECUTOR_ALLOWLIST = {
    "crates/wyrd/wyrd-sql/src/queries/platform/audit_log.rs",
}

# Vala query modules that are intentionally tenant-free (M6/M12). These query
# the global `iceberg_catalog` JDBC catalog — a single cross-tenant namespace,
# gated behind the `diagnostics` feature — not tenant-scoped `vala.*` data. They
# take a raw pool by design: the request-path role is revoked from the schema,
# so isolation is a DB-role boundary, not RLS. Treated like wyrd's platform/
# admin modules — must take PgPool/Transaction, must not touch tenant schemas.
VALA_CATALOG_ALLOWLIST = {
    "crates/vala/vala-sql/src/queries/iceberg_catalog.rs",
}

# Vala query modules that perform cross-tenant relay or reconcile operations
# using the platform admin pool (BYPASSRLS `wyrd_platform_admin`). These
# intentionally enumerate across all tenant partitions and are never on the
# tenant request path. Isolation is enforced at the DB-role boundary. May
# reference tenant schemas and take PgPool by design.
VALA_RELAY_ALLOWLIST = {
    "crates/vala/vala-sql/src/queries/relay.rs",
}

RAW_QUERY_ALLOWLIST_MARKERS = [
    "Dynamic query is intentional",
    "raw-query grep allowlist",
    "SQLx offline bundle",
]

SERVER_POOL_ALLOWLIST_PREFIXES = (
    "crates/wyrd/wyrd-server/src/boot/",
    "crates/wyrd/wyrd-server/src/boot.rs",
    "crates/wyrd/wyrd-server/src/main.rs",
    "crates/wyrd/wyrd-server/src/postgres.rs",
    "crates/wyrd/wyrd-server/src/state.rs",
    "crates/wyrd/wyrd-server/src/routes/platform/",
    "crates/wyrd/wyrd-server/src/components/admin/",
)

CLIENT_TIER_CRATES = [
    "crates/wyrd-spec",
    "crates/vala/vala-client",
    "python/py-wyrd",
]

RLS_LOOKAHEAD_CHARS = 8_000


def main() -> int:
    failures: list[str] = []

    check_rls_triples(failures)
    check_migration_drift(failures)
    check_query_modules(failures)
    check_server_pool_usage(failures)
    check_sql_source_hygiene(failures)
    check_dependency_boundaries(failures)

    if failures:
        print("tenant isolation check failed:", file=sys.stderr)
        for failure in failures:
            print(f"- {failure}", file=sys.stderr)
        return 1

    print("tenant isolation check passed")
    return 0


def check_rls_triples(failures: list[str]) -> None:
    for migrations_dir, schema in [
        (WYRD_SQL_MIGRATIONS, "wyrd"),
        (VALA_SQL_MIGRATIONS, "vala"),
    ]:
        for path in sorted(migrations_dir.glob("*.sql")):
            sql = strip_sql_line_comments(path.read_text())
            for table, window in tenant_table_windows(sql, schema):
                qualified = f"{schema}.{table}"
                normalized_window = normalize_sql(window)
                table_patterns = [
                    rf"alter\s+table\s+{re.escape(qualified)}\s+enable\s+row\s+level\s+security",
                    rf"alter\s+table\s+{re.escape(qualified)}\s+force\s+row\s+level\s+security",
                    rf"create\s+policy\s+tenant_isolation\s+on\s+{re.escape(qualified)}",
                ]
                for pattern in table_patterns:
                    if not re.search(pattern, normalized_window, re.IGNORECASE):
                        failures.append(
                            f"{rel(path)}: missing RLS clause for {qualified}: {pattern}"
                        )

                policy_block = tenant_policy_block(normalized_window, qualified)
                if policy_block is None:
                    continue

                policy_patterns = [
                    r"using\s*\(\s*data_tenant_id\s*=\s*wyrd\.current_tenant\(\)\s*\)",
                    r"with\s+check\s*\(\s*data_tenant_id\s*=\s*wyrd\.current_tenant\(\)\s*\)",
                ]
                for pattern in policy_patterns:
                    if not re.search(pattern, policy_block, re.IGNORECASE):
                        failures.append(
                            f"{rel(path)}: policy block for {qualified} is missing {pattern}"
                        )


def check_migration_drift(failures: list[str]) -> None:
    for path in sorted(WYRD_SQL_MIGRATIONS.glob("*.sql")):
        sql = path.read_text()
        for schema in table_schemas(sql):
            if schema not in {"platform", "wyrd"}:
                failures.append(f"{rel(path)}: CREATE TABLE uses non-Wyrd schema {schema}")

    for path in sorted(VALA_SQL_MIGRATIONS.glob("*.sql")):
        sql = path.read_text()
        for schema in table_schemas(sql):
            if schema not in {"vala", "iceberg_catalog"}:
                failures.append(f"{rel(path)}: CREATE TABLE uses non-Vala schema {schema}")

    migration_text = "\n".join(path.read_text() for path in sql_migration_files())
    if re.search(r"\bCREATE\s+ROLE\b", migration_text, re.IGNORECASE):
        failures.append("SQL migrations must not create cluster roles")

    platform_path = find_platform_migration(failures)
    if platform_path is not None:
        platform_normalized = normalize_sql(platform_path.read_text())
        required_patterns = {
            "wyrd_migrator BYPASSRLS role assertion": r"rolname\s*=\s*'wyrd_migrator'\s+AND\s+rolbypassrls\s*=\s*true",
            "wyrd_app non-BYPASSRLS role assertion": r"rolname\s*=\s*'wyrd_app'\s+AND\s+rolbypassrls\s*=\s*false",
            "wyrd_platform_admin BYPASSRLS role assertion": r"rolname\s*=\s*'wyrd_platform_admin'\s+AND\s+rolbypassrls\s*=\s*true",
            "wyrd.current_tenant helper": r"CREATE\s+FUNCTION\s+wyrd\.current_tenant\(\)\s+RETURNS\s+uuid",
            "tenant slug resolver": r"CREATE\s+FUNCTION\s+platform\.resolve_tenant_by_slug",
            "resolver SECURITY DEFINER": r"SECURITY\s+DEFINER",
            "resolver search_path hardening": r"SET\s+search_path\s*=\s*pg_catalog,\s*platform",
            "resolver grant to app": r"GRANT\s+EXECUTE\s+ON\s+FUNCTION\s+platform\.resolve_tenant_by_slug.*TO\s+wyrd_app",
            "platform admin sequence grants": r"GRANT\s+USAGE,\s+SELECT\s+ON\s+ALL\s+SEQUENCES\s+IN\s+SCHEMA\s+platform,\s*wyrd\s+TO\s+wyrd_platform_admin",
            "app sequence grants": r"GRANT\s+USAGE,\s+SELECT\s+ON\s+ALL\s+SEQUENCES\s+IN\s+SCHEMA\s+wyrd\s+TO\s+wyrd_app",
        }
        for label, pattern in required_patterns.items():
            if not re.search(pattern, platform_normalized, re.IGNORECASE):
                failures.append(f"{rel(platform_path)}: missing {label}")


def find_platform_migration(failures: list[str]) -> Path | None:
    matches = sorted(WYRD_SQL_MIGRATIONS.glob("*_platform.sql"))
    if not matches:
        failures.append(f"missing platform migration in {rel(WYRD_SQL_MIGRATIONS)}")
        return None
    if len(matches) > 1:
        failures.append(
            "multiple platform migrations found: "
            + ", ".join(rel(p) for p in matches)
        )
        return None
    return matches[0]


def check_query_modules(failures: list[str]) -> None:
    check_wyrd_query_modules(failures)
    check_vala_query_modules(failures)


def check_wyrd_query_modules(failures: list[str]) -> None:
    for path in rust_files(WYRD_QUERIES):
        relative = rel(path)
        body = production_source(path.read_text())
        code = strip_line_comments(body)
        is_platform = "/platform/" in relative
        is_admin = "/storage/admin/" in relative

        if is_platform:
            if references_tenant_schema(code):
                failures.append(f"{relative}: platform query module references tenant schema")
            if (
                has_public_async_fn(code)
                and not has_platform_executor(code)
                and relative not in PLATFORM_EXECUTOR_ALLOWLIST
            ):
                failures.append(f"{relative}: platform public async fn must take PgPool or Transaction")
            continue

        if is_admin:
            if has_public_async_fn(code) and not has_platform_executor(code):
                failures.append(f"{relative}: admin public async fn must take PgPool or Transaction")
            continue

        check_tenant_query_file(relative, body, code, failures)


def check_vala_query_modules(failures: list[str]) -> None:
    for path in rust_files(VALA_QUERIES):
        relative = rel(path)
        body = production_source(path.read_text())
        code = strip_line_comments(body)

        if relative in VALA_CATALOG_ALLOWLIST:
            if references_tenant_schema(code):
                failures.append(f"{relative}: catalog query module must not reference tenant schema")
            if has_public_async_fn(code) and not has_platform_executor(code):
                failures.append(f"{relative}: catalog public async fn must take PgPool or Transaction")
            continue

        if relative in VALA_RELAY_ALLOWLIST:
            if has_public_async_fn(code) and not has_platform_executor(code):
                failures.append(f"{relative}: relay public async fn must take PgPool or Transaction")
            continue

        check_tenant_query_file(relative, body, code, failures)


def check_tenant_query_file(relative: str, body: str, code: str, failures: list[str]) -> None:
    if re.search(r"&\s*PgPool\b|\bPgPool\s*,|Transaction\s*<\s*'_", code):
        failures.append(f"{relative}: tenant query module must not take raw PgPool/Transaction")
    if re.search(r"\.begin\s*\(", code):
        failures.append(f"{relative}: tenant query module must not open transactions")

    for fn_name, params in public_async_fns(code):
        if "TenantConn<'_" not in params and "TenantConn < '_" not in params:
            failures.append(f"{relative}: public async fn {fn_name} must take &mut TenantConn<'_>")

    if references_tenant_schema(code) and not (
        re.search(r"data_tenant_id\s*=\s*\$", code)
        or re.search(r"wyrd\.current_tenant\(\)", code)
    ):
        failures.append(f"{relative}: tenant table query is missing data_tenant_id predicate")

    if re.search(r"sqlx::query(?:_as|_scalar)?\s*\(", code) and not has_raw_query_marker(body):
        failures.append(f"{relative}: raw sqlx::query* requires an explicit justification comment")


def check_server_pool_usage(failures: list[str]) -> None:
    for path in rust_files(WYRD_SERVER):
        relative = rel(path)
        code = strip_line_comments(production_source(path.read_text()))
        if is_server_pool_allowlisted(relative):
            continue
        if "platform_admin_pool" in code:
            failures.append(f"{relative}: platform_admin_pool is only allowed in platform routes, boot, or state")
        if re.search(r"&\s*PgPool\b|\bPgPool\s*,", code) and references_tenant_schema(code):
            failures.append(f"{relative}: tenant-scoped server code must use TenantConn, not raw PgPool")


def check_sql_source_hygiene(failures: list[str]) -> None:
    combined_sql_paths = [*SQL_CRATES]
    forbidden_patterns = {
        "non-Postgres SQLx dialect/runtime": r"sqlx::sqlite|sqlx::mysql|runtime-async-std|tls-native-tls",
        "raw transaction control": r"\bBEGIN;|\bCOMMIT;|\bSAVEPOINT\s+",
        "bare tenant table reference": r"\bFROM\s+(auth_|registry_|audit_|drift_|eval_)",
        "predecessor naming drift": r"opsml_|scouter_",
    }
    for label, pattern in forbidden_patterns.items():
        for path in source_files(combined_sql_paths):
            text = path.read_text(errors="ignore")
            if re.search(pattern, strip_line_comments(text), re.IGNORECASE):
                failures.append(f"{rel(path)}: {label}")

    for path in rust_files(WYRD_QUERIES):
        text = production_source(path.read_text())
        code = strip_line_comments(text)
        if re.search(r"sqlx::query\s*\(", code) and not has_raw_query_marker(text):
            failures.append(f"{rel(path)}: use query macros or document the runtime query exception")
    for path in rust_files(VALA_QUERIES):
        text = production_source(path.read_text())
        code = strip_line_comments(text)
        if re.search(r"sqlx::query\s*\(", code) and not has_raw_query_marker(text):
            failures.append(f"{rel(path)}: use query macros or document the runtime query exception")

    for crate in CLIENT_TIER_CRATES:
        path = ROOT / crate
        if path.exists():
            for source in source_files([path]):
                if "sqlx" in source.read_text(errors="ignore"):
                    failures.append(f"{rel(source)}: client-tier code must stay SQLx-free")

    vala_manifest = ROOT / "crates/vala/vala-sql/Cargo.toml"
    if re.search(r"\bpyo3\b|\bpython\b", vala_manifest.read_text(), re.IGNORECASE):
        failures.append(f"{rel(vala_manifest)}: vala-sql must not expose PyO3 or python features")
    for path in rust_files(ROOT / "crates/vala/vala-sql/src"):
        if re.search(r"\bpyo3\b|use\s+pyo3", path.read_text(), re.IGNORECASE):
            failures.append(f"{rel(path)}: vala-sql source must stay PyO3-free")


def check_dependency_boundaries(failures: list[str]) -> None:
    vala_tree = run(["cargo", "tree", "-p", "vala-sql", "--edges", "normal"])
    if vala_tree.returncode == 0 and re.search(r"^[\u251c\u2514]\u2500\u2500 skald-", vala_tree.stdout, re.MULTILINE):
        failures.append("vala-sql must not depend on skald crates")
    elif vala_tree.returncode != 0:
        failures.append(f"cargo tree -p vala-sql failed: {vala_tree.stderr.strip()}")

    skald_tree = run(["cargo", "tree", "-p", "skald-runtime", "--edges", "normal"])
    if skald_tree.returncode == 0 and re.search(r"^[\u251c\u2514]\u2500\u2500 vala-", skald_tree.stdout, re.MULTILINE):
        failures.append("skald-runtime must not depend on Vala crates")
    elif skald_tree.returncode != 0:
        failures.append(f"cargo tree -p skald-runtime failed: {skald_tree.stderr.strip()}")


def tenant_tables(sql: str, schema: str) -> list[str]:
    pattern = re.compile(rf"\bCREATE\s+TABLE\s+{re.escape(schema)}\.([a-z_][a-z0-9_]*)", re.IGNORECASE)
    return pattern.findall(sql)


def tenant_table_windows(sql: str, schema: str) -> list[tuple[str, str]]:
    create_tenant_table = re.compile(
        rf"\bCREATE\s+TABLE\s+{re.escape(schema)}\.([a-z_][a-z0-9_]*)",
        re.IGNORECASE,
    )
    create_any_table = re.compile(
        r"\bCREATE\s+TABLE\s+[a-z_][a-z0-9_]*\.[a-z_][a-z0-9_]*",
        re.IGNORECASE,
    )

    windows: list[tuple[str, str]] = []
    for match in create_tenant_table.finditer(sql):
        next_table = create_any_table.search(sql, match.end())
        next_table_start = next_table.start() if next_table else len(sql)
        lookahead_end = min(len(sql), match.start() + RLS_LOOKAHEAD_CHARS)
        window_end = min(next_table_start, lookahead_end)
        windows.append((match.group(1), sql[match.start():window_end]))
    return windows


def tenant_policy_block(normalized_window: str, qualified_table: str) -> str | None:
    policy_match = re.search(
        rf"create\s+policy\s+tenant_isolation\s+on\s+{re.escape(qualified_table)}\b",
        normalized_window,
        re.IGNORECASE,
    )
    if policy_match is None:
        return None

    statement_end = normalized_window.find(";", policy_match.end())
    if statement_end == -1:
        statement_end = len(normalized_window)
    return normalized_window[policy_match.start():statement_end]


def table_schemas(sql: str) -> list[str]:
    return re.findall(r"\bCREATE\s+TABLE\s+([a-z_][a-z0-9_]*)\.", sql, re.IGNORECASE)


def normalize_sql(sql: str) -> str:
    return re.sub(r"\s+", " ", sql)


def strip_line_comments(text: str) -> str:
    return "\n".join(line.split("//", 1)[0] for line in text.splitlines())


def production_source(text: str) -> str:
    """Return text with the in-file `#[cfg(test)] mod ...` block removed."""
    return text.split("\n#[cfg(test)]", 1)[0]


def strip_sql_line_comments(text: str) -> str:
    return "\n".join(line.split("--", 1)[0] for line in text.splitlines())


def references_tenant_schema(code: str) -> bool:
    return re.search(r"\b(?:FROM|JOIN|INTO|UPDATE|DELETE\s+FROM)\s+(?:wyrd|vala)\.", code, re.IGNORECASE) is not None


def has_public_async_fn(code: str) -> bool:
    return bool(public_async_fns(code))


def public_async_fns(code: str) -> list[tuple[str, str]]:
    pattern = re.compile(
        r"pub\s+async\s+fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(([^)]*)\)",
        re.DOTALL,
    )
    return pattern.findall(code)


def has_platform_executor(code: str) -> bool:
    return re.search(r"&\s*PgPool\b|&\s*mut\s+Transaction\s*<\s*'_|&\s*OperatorPool\b", code) is not None


def has_raw_query_marker(body: str) -> bool:
    return any(marker in body for marker in RAW_QUERY_ALLOWLIST_MARKERS)


def rust_files(path: Path) -> list[Path]:
    if not path.exists():
        return []
    return sorted(path.rglob("*.rs"))


def source_files(paths: list[Path]) -> list[Path]:
    files: list[Path] = []
    for path in paths:
        if path.is_file():
            files.append(path)
            continue
        if not path.exists():
            continue
        files.extend(
            file
            for file in path.rglob("*")
            if file.is_file()
            and file.suffix in {".rs", ".sql", ".toml"}
            and "/target/" not in file.as_posix()
        )
    return sorted(files)


def sql_migration_files() -> list[Path]:
    return sorted([*WYRD_SQL_MIGRATIONS.glob("*.sql"), *VALA_SQL_MIGRATIONS.glob("*.sql")])


def is_server_pool_allowlisted(relative: str) -> bool:
    return relative.startswith(SERVER_POOL_ALLOWLIST_PREFIXES)


def run(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)


def rel(path: Path) -> str:
    return path.resolve().relative_to(ROOT).as_posix()


if __name__ == "__main__":
    sys.exit(main())

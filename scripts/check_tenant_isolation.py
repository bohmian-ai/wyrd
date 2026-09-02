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

# Vala query modules that perform narrow cross-tenant maintenance work through
# the OperatorPool (`wyrd_platform_admin` BYPASSRLS). Maintenance leases use
# global keys without a tenant column; the Forge active-table roster deliberately
# inventories active tenant registrations. Both are operator-only surfaces, so
# their isolation boundary is the DB role rather than tenant-scoped RLS. Must
# take PgPool/OperatorPool.
VALA_OPERATOR_ALLOWLIST = {
    "crates/vala/vala-sql/src/queries/maintenance_leases.rs",
    # Cross-tenant active Bifrost roster used only by the Forge scheduler.
    "crates/vala/vala-sql/src/queries/forge_catalog_operator.rs",
    # Oracle admission recovery is a bounded global counter reconciliation
    # performed through the operator pool before readiness.
    "crates/vala/vala-sql/src/queries/oracle_admission_operator.rs",
}

# Cohesive owners that intentionally expose both cross-tenant OperatorPool
# coordination and TenantConn lifecycle/audit methods. Operator workflows may
# use a dependency-owning handle; tenant workflows still take TenantConn.
VALA_MIXED_EXECUTOR_ALLOWLIST = {
    "crates/vala/vala-sql/src/queries/forge_tasks.rs",
    # Forge operation state: tenant-scoped state reads take a TenantConn, while
    # the prepare/settle/reset lifecycle runs as one fenced operator
    # transaction that must lock the table's maintenance-authority row.
    "crates/vala/vala-sql/src/queries/forge_operations.rs",
}

# Cohesive owners that expose a narrow tenant-bound capability through a struct
# that owns a verified `tenant: DataTenantId` and re-establishes the tenant RLS
# boundary by executing `BIND_CURRENT_TENANT_SQL` before it writes, instead of
# taking a `TenantConn`. This is the fenced-operator audit path: a Forge worker
# appends audit evidence on the operator transaction it already holds, so the
# capability binds the current tenant on that shared connection rather than
# threading a separate TenantConn. Entries must still satisfy both shape rules:
# every public async fn either takes a `TenantConn<'_>` or is a method of the
# tenant-owning capability struct, and the module must execute
# `BIND_CURRENT_TENANT_SQL`.
VALA_TENANT_BOUND_CAPABILITY_ALLOWLIST = {
    "crates/vala/vala-sql/src/queries/audit_outbox.rs",
}

# Cohesive owners whose capability structs *hold* the caller's `TenantConn`
# borrow for the lifetime of a multi-statement workflow, instead of taking one
# per call. The isolation property is unchanged and in fact tighter: every
# statement such a method issues runs on a tenant-scoped RLS connection the
# struct cannot outlive, and the borrow keeps the whole workflow inside one
# tenant transaction. Entries must satisfy the shape rule below: every public
# async fn is either a method of a struct owning a `TenantConn` field, or a
# free function taking `OperatorPool` for the bounded read-only cross-tenant
# recovery enumeration. Every other tenant-query rule still runs unchanged.
VALA_TENANT_CONN_OWNER_ALLOWLIST = {
    "crates/vala/vala-sql/src/queries/oracle_reader_authority.rs",
}

# Vala tables that are intentionally cross-tenant control-plane surfaces with no
# tenant column and NO RLS (accessed only via the OperatorPool). They are
# exempt from the RLS-triple requirement because there is no `data_tenant_id`
# column to scope. Keep this set narrow and every entry justified by a
# control-plane migration.
VALA_NON_RLS_CONTROL_TABLES = {
    "vala.maintenance_leases",
    "vala.cluster_nodes",  # cluster-level node registry, not tenant data
    "vala.forge_scheduler_state",  # singleton cross-tenant scheduler fence
    "vala.forge_worker_claim_state",  # singleton cross-tenant worker fairness cursor
    "vala.forge_large_lane_lease",  # singleton cluster-wide large-task fence
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
                if qualified in VALA_NON_RLS_CONTROL_TABLES:
                    continue
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

                column = re.escape(tenant_column(normalized_window))
                policy_patterns = [
                    rf"using\s*\(\s*{column}\s*=\s*wyrd\.current_tenant\(\)\s*\)",
                    rf"with\s+check\s*\(\s*{column}\s*=\s*wyrd\.current_tenant\(\)\s*\)",
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
                failures.append(
                    f"{rel(path)}: CREATE TABLE uses non-Wyrd schema {schema}"
                )

    for path in sorted(VALA_SQL_MIGRATIONS.glob("*.sql")):
        sql = path.read_text()
        for schema in table_schemas(sql):
            if schema not in {"vala", "iceberg_catalog"}:
                failures.append(
                    f"{rel(path)}: CREATE TABLE uses non-Vala schema {schema}"
                )

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
            "multiple platform migrations found: " + ", ".join(rel(p) for p in matches)
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
                failures.append(
                    f"{relative}: platform query module references tenant schema"
                )
            if (
                has_public_async_fn(code)
                and not has_platform_executor(code)
                and relative not in PLATFORM_EXECUTOR_ALLOWLIST
            ):
                failures.append(
                    f"{relative}: platform public async fn must take PgPool or Transaction"
                )
            continue

        if is_admin:
            if has_public_async_fn(code) and not has_platform_executor(code):
                failures.append(
                    f"{relative}: admin public async fn must take PgPool or Transaction"
                )
            continue

        check_tenant_query_file(relative, body, code, failures)


def check_vala_query_modules(failures: list[str]) -> None:
    for path in rust_files(VALA_QUERIES):
        relative = rel(path)
        body = production_source(path.read_text())
        code = strip_line_comments(body)

        if relative in VALA_CATALOG_ALLOWLIST:
            if references_tenant_schema(code):
                failures.append(
                    f"{relative}: catalog query module must not reference tenant schema"
                )
            if has_public_async_fn(code) and not has_platform_executor(code):
                failures.append(
                    f"{relative}: catalog public async fn must take PgPool or Transaction"
                )
            continue

        if relative in VALA_OPERATOR_ALLOWLIST:
            if has_public_async_fn(code) and not has_platform_executor(code):
                failures.append(
                    f"{relative}: operator public async fn must take PgPool or OperatorPool"
                )
            continue

        if relative in VALA_MIXED_EXECUTOR_ALLOWLIST:
            owns_operator_pool = re.search(
                r"struct\s+ForgeTasks\s*\{[^}]*OperatorPool", code, re.DOTALL
            ) is not None
            for fn_name, params in public_async_fns(code):
                if (
                    "OperatorPool" not in params
                    and "TenantConn<'_" not in params
                    and "TenantConn < '_" not in params
                    and not (owns_operator_pool and "&self" in params)
                ):
                    failures.append(
                        f"{relative}: mixed-executor public async fn {fn_name} must use an OperatorPool-owning self or take OperatorPool/TenantConn"
                    )
            continue

        if relative in VALA_TENANT_CONN_OWNER_ALLOWLIST:
            for fn_name in tenant_conn_owner_violations(code):
                failures.append(
                    f"{relative}: public async fn {fn_name} must take &mut TenantConn<'_>, "
                    "take an OperatorPool, or be a method of the struct owning a TenantConn"
                )
            check_tenant_query_file(
                relative,
                body,
                code,
                failures,
                exempt_tenant_conn_param=True,
            )
            continue

        if relative in VALA_TENANT_BOUND_CAPABILITY_ALLOWLIST:
            if "BIND_CURRENT_TENANT_SQL" not in code:
                failures.append(
                    f"{relative}: tenant-bound capability module must execute BIND_CURRENT_TENANT_SQL to re-establish the RLS boundary"
                )
            owns_tenant_field = re.search(
                r"struct\s+\w+[^{]*\{[^}]*tenant:\s*DataTenantId", code, re.DOTALL
            ) is not None
            for fn_name, params in public_async_fns(code):
                if (
                    "TenantConn<'_" not in params
                    and "TenantConn < '_" not in params
                    and not (owns_tenant_field and "self" in params)
                ):
                    failures.append(
                        f"{relative}: tenant-bound public async fn {fn_name} must take TenantConn or be a method of a struct owning tenant: DataTenantId"
                    )
            check_tenant_query_file(
                relative,
                body,
                code,
                failures,
                allow_operator_transaction=True,
                exempt_tenant_conn_param=True,
            )
            continue

        check_tenant_query_file(relative, body, code, failures)


def check_tenant_query_file(
    relative: str,
    body: str,
    code: str,
    failures: list[str],
    *,
    allow_operator_transaction: bool = False,
    exempt_tenant_conn_param: bool = False,
) -> None:
    """Run every tenant-query SQL rule against one module.

    `allow_operator_transaction` and `exempt_tenant_conn_param` suppress only the
    two rules a tenant-bound capability legitimately replaces with its own shape
    guarantees (see `VALA_TENANT_BOUND_CAPABILITY_ALLOWLIST`): a capability that
    holds the caller's operator transaction is permitted a `Transaction<'_>`
    parameter, and its public async fns are methods of a tenant-owning struct
    rather than `TenantConn` takers. Every other rule — the raw-`PgPool`
    prohibition, the self-opened-transaction prohibition, the tenant-predicate
    requirement, and the raw-query justification — still runs unchanged.
    """
    pool_pattern = (
        r"&\s*PgPool\b|\bPgPool\s*,"
        if allow_operator_transaction
        else r"&\s*PgPool\b|\bPgPool\s*,|Transaction\s*<\s*'_"
    )
    if re.search(pool_pattern, code):
        failures.append(
            f"{relative}: tenant query module must not take raw PgPool/Transaction"
        )
    if re.search(r"\.begin\s*\(", code):
        failures.append(f"{relative}: tenant query module must not open transactions")

    if not exempt_tenant_conn_param:
        for fn_name, params in public_async_fns(code):
            if "TenantConn<'_" not in params and "TenantConn < '_" not in params:
                failures.append(
                    f"{relative}: public async fn {fn_name} must take &mut TenantConn<'_>"
                )

    if references_tenant_schema(code) and not (
        re.search(r"data_tenant_id\s*=\s*\$", code)
        or re.search(r"wyrd\.current_tenant\(\)", code)
    ):
        failures.append(
            f"{relative}: tenant table query is missing data_tenant_id predicate"
        )

    if re.search(
        r"sqlx::query(?:_as|_scalar)?\s*\(", code
    ) and not has_raw_query_marker(body):
        failures.append(
            f"{relative}: raw sqlx::query* requires an explicit justification comment"
        )


def check_server_pool_usage(failures: list[str]) -> None:
    for path in rust_files(WYRD_SERVER):
        relative = rel(path)
        code = strip_line_comments(production_source(path.read_text()))
        if is_server_pool_allowlisted(relative):
            continue
        if "platform_admin_pool" in code:
            failures.append(
                f"{relative}: platform_admin_pool is only allowed in platform routes, boot, or state"
            )
        if re.search(r"&\s*PgPool\b|\bPgPool\s*,", code) and references_tenant_schema(
            code
        ):
            failures.append(
                f"{relative}: tenant-scoped server code must use TenantConn, not raw PgPool"
            )


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
            failures.append(
                f"{rel(path)}: use query macros or document the runtime query exception"
            )
    for path in rust_files(VALA_QUERIES):
        text = production_source(path.read_text())
        code = strip_line_comments(text)
        if re.search(r"sqlx::query\s*\(", code) and not has_raw_query_marker(text):
            failures.append(
                f"{rel(path)}: use query macros or document the runtime query exception"
            )

    for crate in CLIENT_TIER_CRATES:
        path = ROOT / crate
        if path.exists():
            for source in source_files([path]):
                if "sqlx" in source.read_text(errors="ignore"):
                    failures.append(
                        f"{rel(source)}: client-tier code must stay SQLx-free"
                    )

    vala_manifest = ROOT / "crates/vala/vala-sql/Cargo.toml"
    if re.search(r"\bpyo3\b|\bpython\b", vala_manifest.read_text(), re.IGNORECASE):
        failures.append(
            f"{rel(vala_manifest)}: vala-sql must not expose PyO3 or python features"
        )
    for path in rust_files(ROOT / "crates/vala/vala-sql/src"):
        if re.search(r"\bpyo3\b|use\s+pyo3", path.read_text(), re.IGNORECASE):
            failures.append(f"{rel(path)}: vala-sql source must stay PyO3-free")


def check_dependency_boundaries(failures: list[str]) -> None:
    vala_tree = run(["cargo", "tree", "-p", "vala-sql", "--edges", "normal"])
    if vala_tree.returncode == 0 and re.search(
        r"^[\u251c\u2514]\u2500\u2500 skald-", vala_tree.stdout, re.MULTILINE
    ):
        failures.append("vala-sql must not depend on skald crates")
    elif vala_tree.returncode != 0:
        failures.append(f"cargo tree -p vala-sql failed: {vala_tree.stderr.strip()}")

    skald_tree = run(["cargo", "tree", "-p", "skald-runtime", "--edges", "normal"])
    if skald_tree.returncode == 0 and re.search(
        r"^[\u251c\u2514]\u2500\u2500 vala-", skald_tree.stdout, re.MULTILINE
    ):
        failures.append("skald-runtime must not depend on Vala crates")
    elif skald_tree.returncode != 0:
        failures.append(
            f"cargo tree -p skald-runtime failed: {skald_tree.stderr.strip()}"
        )


def tenant_tables(sql: str, schema: str) -> list[str]:
    pattern = re.compile(
        rf"\bCREATE\s+TABLE\s+{re.escape(schema)}\.([a-z_][a-z0-9_]*)", re.IGNORECASE
    )
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
        windows.append((match.group(1), sql[match.start() : window_end]))
    return windows


def tenant_column(normalized_window: str) -> str:
    """Names the tenant column one table's RLS policy must be written against.

    Nearly every tenant table calls the column `data_tenant_id`, and that name
    wins whenever it is present. A table whose tenant is an ownership role
    rather than the row's own data tenant names it accordingly — Oracle reader
    epochs are owned by `epoch_owner_tenant_id` — and the policy has to match
    the column that actually exists, so the single `*_tenant_id` column the
    table declares is used instead. The rule itself is unchanged: both policy
    directions must equal `wyrd.current_tenant()` on that column.
    """
    definition_end = normalized_window.find(");")
    definition = (
        normalized_window if definition_end == -1 else normalized_window[:definition_end]
    )
    declared = set(re.findall(r"\b([a-z_]*tenant_id)\s+uuid\b", definition))
    if "data_tenant_id" in declared:
        return "data_tenant_id"
    candidates = declared
    if len(candidates) == 1:
        return candidates.pop()
    return "data_tenant_id"


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
    return normalized_window[policy_match.start() : statement_end]


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
    return (
        re.search(
            r"\b(?:FROM|JOIN|INTO|UPDATE|DELETE\s+FROM)\s+(?:wyrd|vala)\.",
            code,
            re.IGNORECASE,
        )
        is not None
    )


def has_public_async_fn(code: str) -> bool:
    return bool(public_async_fns(code))


def public_async_fns(code: str) -> list[tuple[str, str]]:
    return [(name, params) for name, params, _ in _public_async_fn_sites(code)]


PUBLIC_ASYNC_FN_RE = re.compile(
    r"pub\s+async\s+fn\s+([a-zA-Z_][a-zA-Z0-9_]*)\s*\(([^)]*)\)",
    re.DOTALL,
)

TENANT_CONN_FIELD_RE = re.compile(r"&\s*'\w+\s+mut\s+TenantConn\b")

STRUCT_RE = re.compile(r"\bstruct\s+([A-Za-z_][A-Za-z0-9_]*)")

IMPL_RE = re.compile(r"\bimpl\b")


def _public_async_fn_sites(code: str) -> list[tuple[str, str, int]]:
    """Return every public async fn as `(name, parameter text, offset)`."""
    return [
        (match.group(1), match.group(2), match.start())
        for match in PUBLIC_ASYNC_FN_RE.finditer(code)
    ]


def _matching_brace(code: str, opening: int) -> int | None:
    """Return the index of the `}` closing the `{` at `opening`, if balanced.

    Only brace depth is tracked. Callers pass comment-stripped Rust, and a
    brace inside a string literal is rare enough in query modules that the
    depth scan stays the simplest thing that answers "which block encloses
    this offset" without adding a parser dependency.
    """
    depth = 0
    for index in range(opening, len(code)):
        char = code[index]
        if char == "{":
            depth += 1
        elif char == "}":
            depth -= 1
            if depth == 0:
                return index
    return None


def _impl_type_name(header: str) -> str | None:
    """Reduce one `impl` header to the concrete type the block belongs to.

    `impl<'a> Trait for Owner<'a>` and `impl<'a> Owner<'a>` both resolve to
    `Owner`, because the receiver's authority comes from the type the methods
    are attached to, never from the trait they satisfy.
    """
    header = re.split(r"\bwhere\b", header, maxsplit=1)[0]
    if " for " in header:
        header = header.rsplit(" for ", 1)[1]
    else:
        header = re.sub(r"^\s*<[^>]*>", "", header)
    header = header.strip().lstrip("&").strip()
    header = re.sub(r"<.*$", "", header).strip()
    name = header.rsplit("::", 1)[-1].strip()
    return name or None


def _impl_blocks(code: str) -> list[tuple[str, int, int]]:
    """Return `(type name, body start, body end)` for every `impl` block.

    A `-> impl Trait` return position is skipped: its "header" would run past
    a parenthesis or semicolon before any block brace, which no real `impl`
    item does.
    """
    blocks: list[tuple[str, int, int]] = []
    for match in IMPL_RE.finditer(code):
        opening = code.find("{", match.end())
        if opening == -1:
            continue
        header = code[match.end() : opening]
        if any(token in header for token in "();="):
            continue
        closing = _matching_brace(code, opening)
        if closing is None:
            continue
        name = _impl_type_name(header)
        if name:
            blocks.append((name, opening, closing))
    return blocks


def _enclosing_impl_type(blocks: list[tuple[str, int, int]], offset: int) -> str | None:
    """Return the innermost `impl` type whose body contains `offset`."""
    best: tuple[str, int] | None = None
    for name, start, end in blocks:
        if start < offset < end and (best is None or start > best[1]):
            best = (name, start)
    return best[0] if best else None


def structs_owning_tenant_conn(code: str) -> set[str]:
    """Return every concrete struct that owns a borrowed `TenantConn` field.

    Ownership is what makes a `self` receiver proof of tenant scope: every
    statement such a method issues runs on the caller's RLS-bound connection.
    Both brace and tuple struct bodies are read, and the body is bounded by
    the struct's own terminator so an unrelated later struct cannot lend it a
    field it does not have.
    """
    owners: set[str] = set()
    for match in STRUCT_RE.finditer(code):
        opening = code.find("{", match.end())
        semicolon = code.find(";", match.end())
        if opening != -1 and (semicolon == -1 or opening < semicolon):
            closing = _matching_brace(code, opening)
            body = code[opening : closing if closing is not None else len(code)]
        elif semicolon != -1:
            body = code[match.end() : semicolon]
        else:
            continue
        if TENANT_CONN_FIELD_RE.search(body):
            owners.add(match.group(1))
    return owners


def tenant_conn_owner_violations(code: str) -> list[str]:
    """Return public async fns in `code` that prove no tenant-scoped authority.

    A function passes on one of three exact grounds: it takes a
    `&mut TenantConn<'_>`, it takes an `OperatorPool` for bounded read-only
    cross-tenant work, or it is a method of the very struct that owns the
    connection. The last ground is receiver-exact on purpose — an unrelated
    struct's `&self` in the same module is not evidence of anything.
    """
    owners = structs_owning_tenant_conn(code)
    blocks = _impl_blocks(code)
    violations: list[str] = []
    for name, params, offset in _public_async_fn_sites(code):
        if (
            "TenantConn<'_" in params
            or "TenantConn < '_" in params
            or "OperatorPool" in params
        ):
            continue
        if "self" in params and _enclosing_impl_type(blocks, offset) in owners:
            continue
        violations.append(name)
    return violations


def has_platform_executor(code: str) -> bool:
    return (
        re.search(
            r"&\s*PgPool\b|&\s*mut\s+Transaction\s*<\s*'_|&\s*OperatorPool\b", code
        )
        is not None
    )


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
    return sorted(
        [*WYRD_SQL_MIGRATIONS.glob("*.sql"), *VALA_SQL_MIGRATIONS.glob("*.sql")]
    )


def is_server_pool_allowlisted(relative: str) -> bool:
    return relative.startswith(SERVER_POOL_ALLOWLIST_PREFIXES)


def run(args: list[str]) -> subprocess.CompletedProcess[str]:
    return subprocess.run(args, cwd=ROOT, text=True, capture_output=True, check=False)


def rel(path: Path) -> str:
    return path.resolve().relative_to(ROOT).as_posix()


if __name__ == "__main__":
    sys.exit(main())

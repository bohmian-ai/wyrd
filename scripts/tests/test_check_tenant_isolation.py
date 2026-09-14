#!/usr/bin/env python3
"""Self-tests for `scripts/check_tenant_isolation.py`.

Run with `python scripts/tests/test_check_tenant_isolation.py`. Exits non-zero
on the first failed assertion. Wired into `mise run check:audit-script`.

The rule under test is the receiver rule for a `TenantConn`-owning query
module: a public async fn may omit an explicit connection parameter only when
its own enclosing `impl` type is the concrete struct that owns the connection.
A second, unrelated struct in the same file must not lend its authority.
"""

from __future__ import annotations

import sys
from pathlib import Path

SCRIPT_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPT_DIR))

from check_tenant_isolation import (  # noqa: E402
    VALA_NON_RLS_CONTROL_TABLES,
    ROOT,
    has_raw_query_marker,
    normalize_sql,
    strip_sql_line_comments,
    structs_owning_tenant_conn,
    tenant_conn_owner_violations,
    tenant_table_windows,
)


MIXED_MODULE = """
pub struct OracleReaderEpochs<'a> {
    conn: &'a mut TenantConn<'a>,
}

impl<'a> OracleReaderEpochs<'a> {
    pub async fn read(&mut self, node_id: Uuid) -> Result<(), SqlError> {
        Ok(())
    }
}

pub struct Unguarded;

impl Unguarded {
    pub async fn leak(&self) -> Result<(), SqlError> {
        Ok(())
    }
}

pub async fn list_expired_epochs_for_operator(
    pool: &OperatorPool,
    limit: i64,
) -> Result<(), SqlError> {
    Ok(())
}

pub async fn unbound_free_function(limit: i64) -> Result<(), SqlError> {
    Ok(())
}
"""


def test_only_the_connection_owning_struct_is_detected() -> None:
    assert structs_owning_tenant_conn(MIXED_MODULE) == {"OracleReaderEpochs"}


def test_exact_receiver_owner_passes_and_unrelated_self_fails() -> None:
    violations = tenant_conn_owner_violations(MIXED_MODULE)
    assert "read" not in violations, "the connection owner's own method is authorized"
    assert "leak" in violations, "an unrelated struct's self receiver proves nothing"
    assert (
        "list_expired_epochs_for_operator" not in violations
    ), "a free function taking OperatorPool keeps its explicit rule"
    assert "unbound_free_function" in violations


def test_trait_impl_for_the_owner_still_resolves_to_the_owner() -> None:
    code = """
    pub struct Epochs<'a> {
        conn: &'a mut TenantConn<'a>,
    }

    impl<'a> SomeTrait for Epochs<'a> {
        pub async fn read(&mut self) -> Result<(), SqlError> {
            Ok(())
        }
    }
    """
    assert tenant_conn_owner_violations(code) == []


def test_tuple_struct_owner_is_detected() -> None:
    code = "pub struct Held<'a>(&'a mut TenantConn<'a>);\n"
    assert structs_owning_tenant_conn(code) == {"Held"}


def test_module_with_no_owner_rejects_every_self_receiver() -> None:
    code = """
    pub struct Nothing;

    impl Nothing {
        pub async fn read(&self) -> Result<(), SqlError> {
            Ok(())
        }
    }
    """
    assert tenant_conn_owner_violations(code) == ["read"]


def test_explicit_tenant_conn_parameter_always_passes() -> None:
    code = """
    pub async fn append_audit(conn: &mut TenantConn<'_>) -> Result<(), SqlError> {
        Ok(())
    }
    """
    assert tenant_conn_owner_violations(code) == []


def _repo_text(relative: str) -> str:
    """Reads one repository file the pinned classifications describe."""
    return (ROOT / relative).read_text()


ORACLE_ADMISSION_MIGRATION = (
    "crates/vala/vala-sql/migrations/20260910000022_oracle_admission_blocks.sql"
)
ORACLE_READER_AUTHORITY_MODULE = (
    "crates/vala/vala-sql/src/queries/oracle_reader_authority.rs"
)
SCRIBE_BATCH_COMMITS_MODULE = (
    "crates/vala/vala-sql/src/queries/scribe_batch_commits.rs"
)


def test_both_oracle_admission_tables_carry_a_complete_rls_triple() -> None:
    migration = strip_sql_line_comments(_repo_text(ORACLE_ADMISSION_MIGRATION))
    windows = dict(tenant_table_windows(migration, "vala"))
    for table in ("oracle_admission_policies", "oracle_admission_blocks"):
        qualified = f"vala.{table}"
        assert qualified not in VALA_NON_RLS_CONTROL_TABLES, (
            "an admission table must never be classified as a non-RLS control table"
        )
        window = normalize_sql(windows[table])
        for clause in (
            f"alter table {qualified} enable row level security",
            f"alter table {qualified} force row level security",
            f"create policy tenant_isolation on {qualified}",
            "using (data_tenant_id = wyrd.current_tenant())",
            "with check (data_tenant_id = wyrd.current_tenant())",
        ):
            assert clause in window.lower(), f"{qualified} is missing `{clause}`"


def test_table_protections_owns_a_tenant_conn_and_rejects_a_raw_connection() -> None:
    code = _repo_text(ORACLE_READER_AUTHORITY_MODULE)
    assert "OracleTableProtections" in structs_owning_tenant_conn(code)
    assert tenant_conn_owner_violations(code) == []
    assert "for_connection" not in code, (
        "a raw-connection constructor would reintroduce the unowned binding"
    )
    raw = code.replace(
        "conn: &'conn mut TenantConn<'tx>,\n}",
        "conn: &'conn mut sqlx::PgConnection,\n}",
    )
    assert "OracleTableProtections" not in structs_owning_tenant_conn(raw), (
        "a raw PgConnection field must not count as tenant ownership"
    )


def test_the_two_sanctioned_raw_query_markers_are_accepted() -> None:
    for relative in (ORACLE_READER_AUTHORITY_MODULE, SCRIBE_BATCH_COMMITS_MODULE):
        code = _repo_text(relative)
        assert has_raw_query_marker(code), f"{relative} lost its raw-query marker"
        assert not has_raw_query_marker(
            code.replace("raw-query grep allowlist", "raw-query grep")
        ), f"{relative} must be justified by the exact sanctioned marker"


def main() -> int:
    failures = 0
    for name, test in sorted(globals().items()):
        if not name.startswith("test_") or not callable(test):
            continue
        try:
            test()
        except AssertionError as error:
            failures += 1
            print(f"FAIL {name}: {error}", file=sys.stderr)
    if failures:
        print(f"{failures} test(s) failed", file=sys.stderr)
        return 1
    print("check_tenant_isolation self-tests passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())

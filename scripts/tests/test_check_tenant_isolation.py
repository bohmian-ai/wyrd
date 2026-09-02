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
    structs_owning_tenant_conn,
    tenant_conn_owner_violations,
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

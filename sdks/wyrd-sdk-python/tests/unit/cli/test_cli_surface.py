"""Python packaging coverage for the shared Rust CLI implementation."""

import os
import sys
from pathlib import Path

import pytest
import wyrd.cli
from wyrd import WyrdError
from wyrd.cli import run_wyrd_cli
from wyrd.testing.cli import plan

EX_FAILURE = 1
EX_VERIFICATION_FAILED = 2


@pytest.fixture
def run_cli(monkeypatch: pytest.MonkeyPatch):
    """Invoke the packaged no-argument entrypoint with an isolated argv."""

    def run(*args: str) -> int:
        monkeypatch.setattr(sys, "argv", ["wyrd", *args])
        return run_wyrd_cli()

    return run


@pytest.fixture
def eval_run(fixtures_dir: Path, tmp_path: Path) -> list[str]:
    """An ``eval run`` of the ok-check Verifier, short of its records."""
    return [
        "eval",
        "run",
        "--eval",
        str(fixtures_dir / "authoring" / "verifier" / "ok-check.yaml"),
        "--subject",
        "tests/Agent/agent-under-test@1.0.0",
        "--out",
        str(tmp_path / "out"),
    ]


def test_production_cli_exposes_only_the_console_script() -> None:
    assert wyrd.cli.__all__ == ["run_wyrd_cli"]
    assert not hasattr(wyrd.cli, "apply")


def test_help_exits_zero(run_cli) -> None:
    assert run_cli("--help") == 0


def test_server_install_is_projected(run_cli) -> None:
    assert run_cli("server", "install", "--help") == 0


def test_unknown_subcommand_exits_with_usage(run_cli) -> None:
    assert run_cli("dev", "bootstrap") == os.EX_USAGE


def test_missing_records_file_exits_with_failure(
    run_cli, eval_run: list[str], tmp_path: Path
) -> None:
    assert run_cli(*eval_run, "--records", str(tmp_path / "missing-records.jsonl")) == EX_FAILURE


def test_failing_records_exit_with_verification_failed(
    run_cli, eval_run: list[str], fixtures_dir: Path
) -> None:
    records = fixtures_dir / "authoring" / "verifier" / "not-ok-records.jsonl"

    assert run_cli(*eval_run, "--records", str(records)) == EX_VERIFICATION_FAILED


def test_in_process_plan_raises_catalog_error_for_missing_tree(tmp_path: Path) -> None:
    """In-process commands raise WyrdError instead of exiting."""
    with pytest.raises(WyrdError) as caught:
        plan(str(tmp_path / "missing.yaml"))
    assert caught.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"

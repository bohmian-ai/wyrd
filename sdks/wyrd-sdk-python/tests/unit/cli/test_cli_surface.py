"""Python packaging coverage for the shared Rust CLI implementation."""

import os
import sys
from pathlib import Path

import pytest
import wyrd.cli
from wyrd import WyrdError
from wyrd.cli import run_wyrd_cli
from wyrd.testing.cli import plan


@pytest.fixture
def run_cli(monkeypatch: pytest.MonkeyPatch):
    """Invoke the packaged no-argument entrypoint with an isolated argv."""

    def run(*args: str) -> int:
        monkeypatch.setattr(sys, "argv", ["wyrd", *args])
        return run_wyrd_cli()

    return run


def test_production_cli_exposes_only_the_console_script() -> None:
    assert wyrd.cli.__all__ == ["run_wyrd_cli"]
    assert not hasattr(wyrd.cli, "apply")


def test_help_exits_zero(run_cli) -> None:
    assert run_cli("--help") == 0


def test_unknown_subcommand_exits_with_usage(run_cli) -> None:
    assert run_cli("dev", "bootstrap") == os.EX_USAGE


def test_in_process_plan_raises_catalog_error_for_missing_tree(tmp_path: Path) -> None:
    """In-process commands raise WyrdError instead of exiting."""
    with pytest.raises(WyrdError) as caught:
        plan(str(tmp_path / "missing.yaml"))
    assert caught.value.code == "WYRD_LOADER_400_INVALID_ENVELOPE"

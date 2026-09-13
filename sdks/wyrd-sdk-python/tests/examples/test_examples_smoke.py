"""Every workflow example must import and run without error."""

import importlib
import sys
from pathlib import Path

import pytest

EXAMPLES_DIR = Path(__file__).resolve().parents[2] / "examples"
sys.path.insert(0, str(EXAMPLES_DIR))


@pytest.mark.parametrize(
    "module_name",
    [
        "from_yaml",
        "from_builder",
        "structured_pipeline",
        "with_observer",
        "transport_grpc",
        "transport_http",
        "transport_mock",
        "transport_secrets",
    ],
)
def test_example_runs(module_name: str) -> None:
    module = importlib.import_module(module_name)
    module.main()

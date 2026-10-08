"""Every workflow example runs as its own script, without credentials."""

import runpy
from pathlib import Path

import pytest

EXAMPLES = Path(__file__).resolve().parents[2] / "examples"


@pytest.mark.parametrize(
    "example",
    [
        "from_yaml",
        "structured_pipeline",
        "transport_grpc",
        "transport_http",
        "transport_mock",
    ],
)
def test_example_runs(example: str) -> None:
    runpy.run_path(str(EXAMPLES / f"{example}.py"), run_name="__main__")

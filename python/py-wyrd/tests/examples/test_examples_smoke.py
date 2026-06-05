"""Every workflow example must import and run without error."""

import importlib

import pytest


@pytest.mark.parametrize(
    "module_name",
    [
        "examples.from_yaml",
        "examples.from_builder",
        "examples.structured_pipeline",
        "examples.with_observer",
    ],
)
def test_example_runs(module_name: str) -> None:
    module = importlib.import_module(module_name)
    module.main()

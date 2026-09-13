"""Run the Wyrd Python unit test partition."""

from __future__ import annotations

import pytest


def main() -> int:
    """Run unit tests while excluding the TensorFlow partition."""
    return pytest.main(
        [
            "-q",
            "-m",
            "not tensorflow",
            "tests/unit",
        ]
    )


if __name__ == "__main__":
    raise SystemExit(main())

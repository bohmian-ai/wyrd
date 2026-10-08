"""The installed ``wyrd`` executable.

``run_wyrd_cli`` is the console script. The in-process command functions are
a test surface in ``wyrd.testing.cli``.
"""

from __future__ import annotations

from wyrd._wyrd.cli import run_wyrd_cli

__all__ = ["run_wyrd_cli"]

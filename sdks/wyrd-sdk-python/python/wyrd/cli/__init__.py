"""Python entry point for the Wyrd CLI.

``run_wyrd_cli()`` reads ``sys.argv``, runs the same parser and commands as the
``wyrd`` binary, and returns the process exit status as an ``int``. The
installed ``wyrd`` console script calls it.
"""

from .._wyrd import run_wyrd_cli

__all__ = ["run_wyrd_cli"]

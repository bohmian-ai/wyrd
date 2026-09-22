#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

uv run python - <<'PY'
import json
import subprocess
import sys

metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--format-version", "1", "--no-deps"],
        text=True,
    )
)

violations = []
for package in metadata["packages"]:
    for dependency in package["dependencies"]:
        if dependency["name"] not in {"wyrd-testing", "wyrd-bench"}:
            continue
        # The test-tier crate owns the Task-15 benchmark adapters. Keep the
        # benchmark crate optional and behind the explicit `bench` feature so
        # its library/test consumers do not pull it into the default build.
        if (
            package["name"] == "wyrd-testing"
            and dependency["name"] == "wyrd-bench"
            and dependency.get("optional")
        ):
            continue
        if dependency.get("kind") not in (None, "normal"):
            continue
        if (
            package["name"] == "wyrd-sdk-python"
            and dependency["name"] == "wyrd-testing"
            and dependency.get("optional")
        ):
            continue
        if (
            package["name"] == "wyrd-sdk-ts-testing"
            and dependency["name"] == "wyrd-testing"
        ):
            continue
        violations.append(
            f"{package['name']} declares {dependency['name']} as a normal dependency"
        )

if violations:
    print("test-tier production dependency check failed:", file=sys.stderr)
    print("\n".join(f"- {violation}" for violation in violations), file=sys.stderr)
    sys.exit(1)

print("test-tier production dependency check passed")
PY

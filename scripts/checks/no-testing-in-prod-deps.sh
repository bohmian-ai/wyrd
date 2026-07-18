#!/usr/bin/env bash
set -euo pipefail

repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"

python3 - <<'PY'
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
        if dependency.get("kind") not in (None, "normal"):
            continue
        if (
            package["name"] == "py-wyrd"
            and dependency["name"] == "wyrd-testing"
            and dependency.get("optional")
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

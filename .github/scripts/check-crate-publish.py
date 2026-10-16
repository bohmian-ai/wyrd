#!/usr/bin/env python3
"""Fail when the crates named in release-plz.toml cannot all reach crates.io.

crates.io needs every dependency of a published crate published first,
optional ones included. A versioned dev-dependency must exist there too, but
release-plz orders publishing by normal dependencies only, so an internal
dev-dependency must be path-only for cargo to strip it.
"""

import json
import subprocess
import sys
import tomllib

published = {
    p["name"]
    for p in tomllib.load(open("release-plz.toml", "rb"))["package"]
    if p.get("publish")
}
metadata = json.loads(
    subprocess.check_output(
        ["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1"]
    )
)
workspace = {p["name"] for p in metadata["packages"]}

errors = []
for package in metadata["packages"]:
    if package["name"] not in published:
        continue
    for dep in package["dependencies"]:
        if dep["name"] not in workspace:
            continue
        if dep["kind"] == "dev":
            if dep["req"] != "*":
                errors.append(
                    f"{package['name']}: dev-dependency {dep['name']} has a version; make it path-only"
                )
        elif dep["name"] not in published:
            errors.append(
                f"{package['name']}: depends on {dep['name']}, which release-plz.toml does not publish"
            )

for error in errors:
    print(error, file=sys.stderr)
sys.exit(1 if errors else 0)

#!/usr/bin/env python3
"""Select CI verification lanes and release packages for a set of changed files.

Usage: select-ci.py <changed-files>

Each changed path is owned by a workspace package (longest manifest-directory
prefix) or classified by path. Changed packages expand to their consumer
closure: normal and build edges propagate transitively, while a dev-dependency
edge adds the dependent's tests without propagating further, because a test
dependency never reaches the dependent's own consumers. Target- and
feature-specific edges are included unconditionally.

Outputs are appended to $GITHUB_OUTPUT as name=value lines, and the reason for
every selection is printed (and appended to $GITHUB_STEP_SUMMARY when set). Any
path or package the selector cannot place, or any failure to read the
workspace graph, selects the complete gate rather than a smaller plan.
WYRD_CARGO_METADATA names a prepared `cargo metadata` document for self-tests.
"""

import json
import os
import re
import subprocess
import sys
from collections import deque

# Global paths configure every lane at once.
GLOBAL = re.compile(r"^(Cargo\.toml|Cargo\.lock|rust-toolchain\.toml|deny\.toml|mise\.toml|\.github/)")
# Paths that select no verification of their own.
UNVERIFIED = re.compile(r"^(changes/|architecture/|[^/]+\.md$)")
DOCS = re.compile(r"^(docs/|openapi\.yaml|crates/wyrd-spec/schemas/|examples/)")
# UI also covers the generated token targets so check:tokens fires on a
# hand-edit to any of them. The UI is bundled beside the server binary, not
# compiled into it, so it does not select the server's Rust closure.
UI = re.compile(
    r"^(crates/wyrd/wyrd-server/wyrd-ui/|docs/src/styles/wyrd-tokens\.css"
    r"|\.claude/skills/wyrd-ui/references/wyrd-theme\.css|\.agents/skills/wyrd-ui/references/wyrd-theme\.css)"
)
PYTHON = re.compile(r"^(sdks/wyrd-sdk-python/|examples/python/)")
TYPESCRIPT = re.compile(r"^sdks/wyrd-sdk-ts/")
# Non-package inputs of the environment-owning journeys; package changes reach
# them through the closure.
STORAGE = re.compile(r"^(crates/wyrd/wyrd-storage/docker/|\.github/workflows/storage-integration)")
IDENTITY = re.compile(r"^(docker-compose\.yml|tests/fixtures/identity/|\.github/workflows/identity-e2e\.yml)")
# Paths owned by the Bifrost capability gate. A change made only of these runs
# verify:bifrost; Bifrost plus other Rust takes the complete gate.
BIFROST_ONLY = re.compile(
    r"^(architecture/bifrost-design\.md"
    r"|architecture/references/domain/(olap-serving|iceberg|datafusion|arrow-analytical-interop|analytical-operations-reliability)\.md"
    r"|crates/vala/vala-bifrost-redux/"
    r"|crates/shared/wyrd-client/(src/bifrost/|tests/pg_bifrost_e2e\.rs)"
    r"|crates/vala/vala-sql/(src/(queries|row_types)/(forge|oracle|file_list|maintenance|scribe)"
    r"|tests/(oracle_admission|pg_(file_list|forge|maintenance|olap|oracle|stream)))"
    r"|crates/wyrd/wyrd-testing/(src/bifrost/|tests/bifrost/)"
    r"|crates/wyrd/wyrd-server/src/(bifrost/|oracle/|query/|grpc/(query|scribe_tail)\.rs)"
    r"|crates/wyrd/wyrd-server/tests/(pg_eval_v1_protocol|pg_grpc_ingest_smoke|pg_grpc_smoke|pg_merge_http_protected|pg_router_smoke)\.rs"
    r"|crates/wyrd/wyrd-mcp/(src/bifrost/|tests/bifrost/)"
    r"|crates/wyrd-spec/src/vala/(api|assignment_authority|error|ids|managed_columns)\.rs"
    r"|sdks/wyrd-sdk-python/(python/wyrd/bifrost/|tests/bifrost/|tests/test_bifrost\.py|tests/integration/test_bifrost_(e2e|query)\.py)"
    r"|sdks/wyrd-sdk-ts/wyrd/(tests/unit/bifrost-query\.test\.ts|tests/integration/oracle-query\.test\.ts))"
)
# Trees whose every file must be owned by a workspace package.
PACKAGE_TREES = re.compile(r"^(crates/|sdks/[^/]+/(src|native|native-testing)/|examples/rust/)")

# Packages outside the four Rust families; each has a dedicated owning lane.
LANGUAGE_PACKAGES = {"wyrd-sdk-python", "wyrd-sdk-ts", "wyrd-sdk-ts-testing", "wyrd-rust-examples"}
# Packages whose ignored Bifrost journeys test:bifrost:gate runs.
BIFROST_JOURNEY = {"vala-bifrost-redux", "wyrd-client", "wyrd-testing", "wyrd-mcp"}
STORAGE_PACKAGES = {"wyrd-storage", "wyrd-server", "wyrd-sql"}
IDENTITY_PACKAGES = {
    "wyrd-auth", "wyrd-auth-check", "wyrd-auth-issue", "wyrd-auth-oidc",
    "wyrd-auth-verify", "wyrd-client", "wyrd-server", "wyrd-testing",
}
CODEGEN_PACKAGES = {"wyrd-spec", "wyrd-client", "vala-core", "wyrd-sdk-python"}
RUST_CLIENT_PACKAGES = {"wyrd-client", "wyrd-sdk-rust"}
EXAMPLE_PACKAGES = {"wyrd-rust-examples", "wyrd-cli"}
# release-plz publishes these crates.
PUBLISHED_CRATES = {"wyrd-spec"}
FAMILIES_FILE = "scripts/test-families.sh"


def read_families(root):
    """Map each package to its Rust test family from scripts/test-families.sh."""
    families = {}
    text = open(os.path.join(root, FAMILIES_FILE), encoding="utf-8").read()
    for name, body in re.findall(r"FAMILY_([A-Z]+)=\(([^)]*)\)", text):
        for package in body.split():
            families[package] = name.lower()
    return families


def read_metadata(root):
    """Load the workspace graph without resolving registry dependencies."""
    override = os.environ.get("WYRD_CARGO_METADATA")
    if override:
        return json.load(open(override, encoding="utf-8"))
    raw = subprocess.run(
        ["cargo", "metadata", "--no-deps", "--format-version", "1", "--locked"],
        cwd=root, check=True, capture_output=True, text=True,
    ).stdout
    return json.loads(raw)


class Workspace:
    """The workspace packages, their directories, and reverse dependency edges."""

    def __init__(self, metadata):
        root = metadata["workspace_root"].rstrip("/") + "/"
        members = set(metadata["workspace_members"])
        packages = [p for p in metadata["packages"] if p["id"] in members]
        names = {p["name"] for p in packages}
        self.dirs = sorted(
            ((os.path.dirname(p["manifest_path"]).removeprefix(root) + "/", p["name"]) for p in packages),
            key=lambda item: len(item[0]), reverse=True,
        )
        # consumers[dep] = [(consumer, kind)]; kind is None for normal edges.
        self.consumers = {name: [] for name in names}
        for package in packages:
            for dep in package["dependencies"]:
                if dep["name"] in names and dep["name"] != package["name"]:
                    self.consumers[dep["name"]].append((package["name"], dep.get("kind")))

    def owner(self, path):
        """Return the package whose directory is the longest prefix of path."""
        return next((name for directory, name in self.dirs if path.startswith(directory)), None)

    def closure(self, changed):
        """Return {package: reason} for the changed packages and their consumers.

        The second result holds only the packages whose shipped bytes can
        change (normal and build edges), which drives release packaging.
        """
        reasons = {name: "changed" for name in changed}
        queue = deque(changed)
        while queue:
            dep = queue.popleft()
            for consumer, kind in self.consumers[dep]:
                if kind == "dev" or consumer in reasons:
                    continue
                reasons[consumer] = f"depends on {dep}"
                queue.append(consumer)
        shipped = set(reasons)
        for dep in shipped:
            for consumer, kind in self.consumers[dep]:
                if kind == "dev" and consumer not in reasons:
                    reasons[consumer] = f"tests depend on {dep}"
        return reasons, shipped


class Selection:
    """Accumulates selected lanes, flags, and the reason for each."""

    def __init__(self):
        self.lanes = []
        self.flags = {}
        self.notes = []
        self.full_gate_reason = ""

    def lane(self, name, reason):
        if name not in self.lanes:
            self.lanes.append(name)
            self.notes.append(f"lane {name}: {reason}")

    def flag(self, name, reason):
        if not self.flags.get(name):
            self.flags[name] = True
            self.notes.append(f"{name}: {reason}")

    def full_gate(self, reason):
        if not self.full_gate_reason:
            self.full_gate_reason = reason
            self.notes.append(f"full gate: {reason}")


def select(paths, root):
    """Classify paths and return (selection, outputs)."""
    sel = Selection()
    outputs = {"changed_packages": "", "rust_packages": ""}

    if not paths:
        return sel, outputs
    for path in paths:
        if GLOBAL.match(path):
            sel.full_gate(f"global input {path}")
    bifrost = [p for p in paths if BIFROST_ONLY.match(p)]
    bifrost_only = bool(bifrost) and len(bifrost) == len(paths)

    try:
        workspace = Workspace(read_metadata(root))
        families = read_families(root)
    except (OSError, ValueError, KeyError, subprocess.CalledProcessError) as error:
        sel.full_gate(f"cannot read the workspace graph: {error}")
        return sel, outputs

    changed = {}
    for path in paths:
        package = None if UI.match(path) else workspace.owner(path)
        if package:
            changed.setdefault(package, path)
            if not BIFROST_ONLY.match(path) and bifrost:
                sel.full_gate(f"mixed Bifrost ({bifrost[0]}) and other Rust ({path})")
        elif PACKAGE_TREES.match(path) and not UI.match(path):
            sel.full_gate(f"{path} is not owned by a workspace package")
        elif not (GLOBAL.match(path) or UNVERIFIED.match(path) or DOCS.match(path) or UI.match(path)
                  or PYTHON.match(path) or TYPESCRIPT.match(path) or STORAGE.match(path)
                  or IDENTITY.match(path) or BIFROST_ONLY.match(path)):
            sel.full_gate(f"unclassified path {path}")

    for path in paths:
        if DOCS.match(path):
            sel.flag("docs", f"docs input {path}")
        if UI.match(path):
            sel.flag("ui", f"UI input {path}")
            sel.flag("package_server", f"UI bundle input {path}")
        if STORAGE.match(path):
            sel.flag("storage", f"storage environment input {path}")
        if IDENTITY.match(path):
            sel.flag("identity", f"identity environment input {path}")

    if bifrost_only:
        sel.notes.append("bifrost_only: every path is owned by the Bifrost capability gate")
        outputs["bifrost_only"] = "true"
        outputs["changed_packages"] = " ".join(sorted(changed))
        return sel, outputs

    affected, shipped = workspace.closure(changed)
    outputs["changed_packages"] = " ".join(sorted(changed))
    outputs["rust_packages"] = " ".join(sorted(affected))
    for package, reason in sorted(affected.items()):
        sel.notes.append(f"package {package}: {reason}" + (f" ({changed[package]})" if package in changed else ""))

    for package in sorted(affected):
        family = families.get(package)
        if family:
            sel.lane(f"test:{family}", f"tests {package}")
        elif package not in LANGUAGE_PACKAGES:
            sel.full_gate(f"{package} has no owning test lane")

    python = [p for p in paths if PYTHON.match(p)]
    typescript = [p for p in paths if TYPESCRIPT.match(p)]
    if "wyrd-sdk-python" in affected or python:
        why = "wyrd-sdk-python is affected" if "wyrd-sdk-python" in affected else f"Python input {python[0]}"
        sel.flag("python", why)
        sel.lane("py:test:integration", why)
    if affected.keys() & {"wyrd-sdk-ts", "wyrd-sdk-ts-testing"} or typescript:
        why = "the TypeScript addon is affected" if not typescript else f"TypeScript input {typescript[0]}"
        sel.flag("typescript", why)
        sel.lane("ts:napi:check", why)
        sel.lane("ts:test:integration", why)
    if hits := sorted(shipped & CODEGEN_PACKAGES) or typescript:
        sel.lane("codegen:check", f"generated contracts derive from {hits[0]}")
    if hits := sorted(affected.keys() & BIFROST_JOURNEY):
        sel.lane("test:bifrost:gate", f"Bifrost journeys exercise {hits[0]}")
    if hits := sorted(affected.keys() & EXAMPLE_PACKAGES):
        sel.lane("check:examples", f"examples build {hits[0]}")
    if hits := sorted(affected.keys() & RUST_CLIENT_PACKAGES):
        sel.flag("rust_client", f"{hits[0]} is affected")
    if hits := sorted(affected.keys() & STORAGE_PACKAGES):
        sel.flag("storage", f"{hits[0]} is on the storage server boundary")
    if hits := sorted(affected.keys() & IDENTITY_PACKAGES):
        sel.flag("identity", f"{hits[0]} is on the identity server boundary")

    if "wyrd-sdk-python" in shipped or python:
        sel.flag("package_python", "the Python package contents are affected")
    if shipped & {"wyrd-sdk-ts"} or typescript:
        sel.flag("package_typescript", "the TypeScript addon contents are affected")
    if "wyrd-server" in shipped:
        sel.flag("package_server", "the server binary is affected")
    if shipped & PUBLISHED_CRATES:
        sel.flag("package_crates", "a published crate is affected")
    return sel, outputs


def main():
    root = subprocess.run(
        ["git", "rev-parse", "--show-toplevel"], check=True, capture_output=True, text=True,
    ).stdout.strip()
    paths = [line.strip() for line in open(sys.argv[1], encoding="utf-8") if line.strip()]
    sel, outputs = select(paths, root)

    full_gate = bool(sel.full_gate_reason)
    flags = ["docs", "ui", "python", "typescript", "rust_client", "storage", "identity",
             "package_python", "package_typescript", "package_server", "package_crates"]
    result = {
        "any": str(bool(paths)).lower(),
        "full_gate": str(full_gate).lower(),
        "full_gate_reason": sel.full_gate_reason,
        "bifrost_only": "true" if outputs.get("bifrost_only") and not full_gate else "false",
        "changed_packages": outputs["changed_packages"],
        "rust_packages": outputs["rust_packages"],
        # The full gate runs every lane itself.
        "ci_lanes": "" if full_gate else " ".join(sel.lanes),
    }
    for name in flags:
        # Unknown impact selects every platform job and the complete package matrix.
        unknown = full_gate and (name.startswith("package_") or name in ("python", "typescript", "rust_client"))
        result[name] = str(unknown or bool(sel.flags.get(name))).lower()

    with open(os.environ.get("GITHUB_OUTPUT", "/dev/stdout"), "a", encoding="utf-8") as out:
        for name, value in result.items():
            out.write(f"{name}={value}\n")

    report = ["Changed files:", *(f"- {p}" for p in paths), "", "Selection:",
              *(f"- {note}" for note in sel.notes),
              *(f"- {name}={value}" for name, value in result.items())]
    print("\n".join(report))
    if summary := os.environ.get("GITHUB_STEP_SUMMARY"):
        with open(summary, "a", encoding="utf-8") as out:
            out.write("### CI selection\n\n```text\n" + "\n".join(report) + "\n```\n")


if __name__ == "__main__":
    main()

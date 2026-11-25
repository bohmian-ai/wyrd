"""Register wyrd-example-py for pull request review once GitHub covers it.

`example-app.sh register` runs this in the app's environment, with the
production SDK and the ambient development credential. It waits for the
tenant's GitHub connection to cover the demo repository, then declares a
Repository Card through the ordinary Cards API: the provider host and
repository id come from that connection entry, and the one binding runs the
Verifier `example-app.sh setup` declared on every pull request. It exits 0
once the declared version is selected with exactly that binding, and 1 on any
refusal. A server without a GitHub App leaves review off and exits 0.
"""

from __future__ import annotations

import json
import sys
import tempfile
import time
from pathlib import Path
from typing import TYPE_CHECKING

from wyrd import WyrdError
from wyrd.cards import CardKind, CardRef, Cards
from wyrd.github import GithubConnection

if TYPE_CHECKING:
    from wyrd.github import GithubConnectionView

REPOSITORY = "bohmian-ai/wyrd-example-py"
VERIFIER = CardRef(CardKind.Verifier, "code-review", "1.0.0")


def waiting_for(view: GithubConnectionView) -> str | None:
    """Why `view` cannot register the demo yet, or `None` once it can."""
    if view["state"] != "active":
        return f"GitHub connection is {view['state']}"
    if not any(entry["full_name"].lower() == REPOSITORY for entry in view["repositories"]):
        return f"GitHub connection does not cover {REPOSITORY}"
    return None


def wait(github: GithubConnection, poll_seconds: float) -> GithubConnectionView | None:
    """The first connection view covering the demo; `None` without a GitHub App."""
    reported = None
    while True:
        try:
            view = github.get()
        except WyrdError as error:
            if error.code == "WYRD_GITHUB_503_NOT_CONFIGURED":
                return None
            raise
        reason = waiting_for(view)
        if reason is None:
            return view
        if reason != reported:
            print(f"review       waiting: {reason}; connect it in Settings", flush=True)
            reported = reason
        time.sleep(poll_seconds)


def declare(cards: Cards, view: GithubConnectionView) -> CardRef:
    """Declare the demo's Repository and return the selected version.

    Raises:
        WyrdError: the server's refusal of the declaration.
        RuntimeError: when the declared version is not selected with exactly
            the seeded Verifier's pull request binding.
    """
    entry = next(e for e in view["repositories"] if e["full_name"].lower() == REPOSITORY)
    card = {
        "apiVersion": "wyrd/v1",
        "kind": "Repository",
        "metadata": {"name": "wyrd-example-py"},
        "spec": {
            "host": view["host"],
            "repository_id": entry["repository_id"],
            "verified_by": [
                {
                    "verifier": {
                        "kind": "Verifier",
                        "name": VERIFIER.name,
                        "version": VERIFIER.version,
                    },
                    "runs_on": {"kind": "pull_request"},
                }
            ],
        },
    }
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "repository.yaml"
        path.write_text(json.dumps(card))
        root = cards.declare_from_path(path).root
    declared = cards.get(root)
    selection = (declared.status or {}).get("repository") or {}
    bindings = declared.spec.get("verified_by", [])
    verifier = cards.get(VERIFIER).card_ref
    bound = [binding["verifier"] for binding in bindings]
    if (
        root.kind != CardKind.Repository
        or selection.get("selected") is not True
        or selection.get("selected_version") != root.version
        or selection.get("full_name", "").lower() != REPOSITORY
        or len(bound) != 1
        or bound[0].get("uid", verifier.uid) != verifier.uid
        or (bound[0].get("name"), bound[0].get("version")) != (verifier.name, verifier.version)
        or bindings[0].get("runs_on") != {"kind": "pull_request"}
    ):
        raise RuntimeError(
            f"{root} is not selected with exactly {VERIFIER} on pull requests: {selection} {bindings}"
        )
    return root


def main() -> int:
    poll_seconds = float(sys.argv[1]) if len(sys.argv) > 1 else 2.0
    try:
        view = wait(GithubConnection(), poll_seconds)
        if view is None:
            print(
                "review       off: this server has no GitHub App (see CONTRIBUTING.md)", flush=True
            )
            return 0
        root = declare(Cards(), view)
    except (WyrdError, RuntimeError) as error:
        print(f"review       FAILED to register {REPOSITORY}: {error}", file=sys.stderr, flush=True)
        return 1
    print(
        f"review       registered {REPOSITORY} as {root.kind.name}/{root.name}@{root.version}, "
        f"reviewing pull requests with {VERIFIER.kind.name}/{VERIFIER.name}@{VERIFIER.version}",
        flush=True,
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())

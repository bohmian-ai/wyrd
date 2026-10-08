"""External-gateway workflow example; run with `uv run python examples/python/workflow_gateway.py`."""

from __future__ import annotations

from pathlib import Path

from wyrd import Workflow

WORKFLOW = Path(__file__).resolve().parents[1] / "workflows" / "gateway-demo" / "workflow.yaml"


def main() -> None:
    # The researcher step routes to the `example-gateway` binding, which the
    # client config.toml declares; the writer step calls OpenAI directly.
    workflow = Workflow.from_path(WORKFLOW)
    run = workflow.run("renewable energy")

    print(f"status: {run.status}")
    print(f"outputs: {run.outputs}")


if __name__ == "__main__":
    main()

"""Load a workflow definition from YAML and inspect its steps."""

from pathlib import Path

from wyrd import Workflow

BUNDLE = Path(__file__).parents[3] / "examples" / "workflows" / "code-review"


def main() -> None:
    wf = Workflow.from_path(BUNDLE / "workflow.yaml")
    print(f"workflow: {wf.name}")
    print(f"steps: {', '.join(wf.steps)}")


if __name__ == "__main__":
    main()

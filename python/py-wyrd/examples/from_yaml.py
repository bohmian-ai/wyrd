"""Load a workflow definition from YAML and inspect its steps."""

from pathlib import Path

from wyrd import Workflow


def main() -> None:
    wf = Workflow.load(Path(__file__).parent / "workflows" / "research.yaml")
    print(f"workflow: {wf.name}")
    print(f"steps: {', '.join(wf.steps)}")


if __name__ == "__main__":
    main()

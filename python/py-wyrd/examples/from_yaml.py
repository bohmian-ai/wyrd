"""Load a workflow from YAML and execute it locally."""

from pathlib import Path

from wyrd import Workflow


def main() -> None:
    wf = Workflow.load(Path(__file__).parent / "workflows" / "research.yaml")
    run = wf.run({"topic": "climate change", "summary": "local mock plan"})
    for step_id, outcome in run.outcomes.items():
        print(step_id, outcome.status)


if __name__ == "__main__":
    main()

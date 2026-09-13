"""Parallel workflow: two researchers fan into one synthesizer."""

from wyrd import Agent, Prompt, Workflow


def main() -> None:
    researcher_a = Agent(
        name="researcher_a",
        prompt=Prompt(
            provider="mock",
            model="mock-model",
            messages=[r'{"summary":"A summary","steps":["a1","a2"]}'],
            output={"summary": str, "steps": list[str]},
        ),
    )
    researcher_b = Agent(
        name="researcher_b",
        prompt=Prompt(
            provider="mock",
            model="mock-model",
            messages=[r'{"summary":"B summary","steps":["b1","b2"]}'],
            output={"summary": str, "steps": list[str]},
        ),
    )
    synthesizer = Agent(
        name="synthesizer",
        prompt=Prompt(
            provider="mock",
            model="mock-model",
            messages=["Synthesize: ${summary}"],
        ),
    )

    wf = Workflow.parallel("parallel_research", researcher_a, researcher_b)
    wf.add_after(synthesizer, after=[researcher_a, researcher_b])

    run = wf.run({"topic": "the Rust borrow checker"})
    print(f"steps: {len(run.outcomes)}")
    print(f"final: {run.final_output}")


if __name__ == "__main__":
    main()

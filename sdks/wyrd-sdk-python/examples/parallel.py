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
    wf.with_step_inputs(
        "synthesizer", {"summary": "steps.researcher_a.output.structured.summary"}
    ).with_outputs(
        {"brief": "steps.synthesizer.output.text", "b_plan": "steps.researcher_b.output.structured"}
    )

    run = wf.run()
    print(f"steps: {len(run.steps)}")
    print(f"outputs: {run.outputs}")


if __name__ == "__main__":
    main()

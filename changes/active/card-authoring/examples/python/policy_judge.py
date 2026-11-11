"""Revision-4 sketch: named Prompt and Agent Cards using YAML field names."""

from wyrd.authoring import Agent, Prompt


def make_policy_judge(name: str = "sharing-policy-judge") -> Agent:
    prompt = Prompt(
        f"{name}-prompt",
        "1.0.0",
        provider="openai",
        model="gpt-4o",
        response_format="verdict",
        messages=[
            {
                "role": "user",
                "content": (
                    "Read /inputs/policy.json with fs.read. Determine whether the proposed "
                    "release satisfies the company policy. Cite original request and policy "
                    "evidence. Report: {{report}} Recipient: {{recipient}} "
                    "Purpose: {{purpose}} Destination: {{destination}}"
                ),
            }
        ],
    )
    return Agent(
        name,
        "1.0.0",
        prompt=prompt,
        execution_route={"kind": "wyrd_gateway", "deployment": "openai"},
        tool_names=["fs.read"],
        run_config={"max_iterations": 3},
    )

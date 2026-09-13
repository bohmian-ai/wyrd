from pathlib import Path

from wyrd import Agent, Prompt


def main() -> None:
    prompt = Prompt.openai_chat(
        "gpt-4o-mini",
        system="Plan concise next steps.",
        messages=["Plan the release for {{topic}}."],
        variables=["topic"],
        version="0.3.0",
    )
    agent = Agent(
        name="planner-agent",
        version="0.3.0",
        space="research",
        prompt=prompt,
    )
    Path("python_planner.yaml").write_text(agent.to_yaml_string(), encoding="utf-8")


if __name__ == "__main__":
    main()

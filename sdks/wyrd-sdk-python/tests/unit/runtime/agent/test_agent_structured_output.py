"""AgentRun.structured_output is the model's JSON answer for a schema prompt."""

from collections.abc import Callable

import pytest
from wyrd import Agent, Prompt, WyrdError
from wyrd.agent import MockProvider


@pytest.fixture
def schema_agent() -> Callable[[str], Agent]:
    """Build an offline two-field schema Agent whose model answers once with the given text."""

    def build(answer: str) -> Agent:
        return Agent(
            prompt=Prompt(
                messages=["Hi"],
                model="mock-model",
                provider="mock",
                output={"foo": str, "bar": str},
            ),
            mock_provider=MockProvider([answer]),
        )

    return build


def test_structured_output_parses_into_dict(schema_agent: Callable[[str], Agent]) -> None:
    assert schema_agent('{"foo":"A","bar":"B"}').run("go").structured_output == {
        "foo": "A",
        "bar": "B",
    }


def test_non_json_answer_is_refused(schema_agent: Callable[[str], Agent]) -> None:
    with pytest.raises(WyrdError) as exc:
        schema_agent("not json").run("go")

    assert exc.value.code == "WYRD_AGENT_422_STRUCTURED_DECODE"

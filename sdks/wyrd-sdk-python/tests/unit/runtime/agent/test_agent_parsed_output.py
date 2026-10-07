"""AgentRun.parsed returns the model's structured answer as an output-class instance."""

from collections.abc import Callable

import pydantic
import pytest
from wyrd import Agent, Prompt, WyrdError
from wyrd.agent import MockProvider


class Plan(pydantic.BaseModel):
    summary: str
    steps: list[str] = []


@pytest.fixture
def mock_agent() -> Callable[..., Agent]:
    """Build an offline Agent whose model answers once with the given text."""

    def build(
        answer: str, output: type | dict[str, type], output_type: type | None = None
    ) -> Agent:
        return Agent(
            prompt=Prompt(
                messages=["Plan it."], model="mock-model", provider="mock", output=output
            ),
            mock_provider=MockProvider([answer]),
            output_type=output_type,
        )

    return build


def test_parsed_returns_pydantic_instance(mock_agent: Callable[..., Agent]) -> None:
    run = mock_agent('{"summary":"Rust is great","steps":["learn ownership"]}', Plan).run("go")

    assert run.parsed == Plan(summary="Rust is great", steps=["learn ownership"])


def test_structured_output_stays_a_dict_beside_parsed(mock_agent: Callable[..., Agent]) -> None:
    run = mock_agent('{"summary":"hello"}', Plan).run("go")

    assert run.structured_output == {"summary": "hello"}


def test_parsed_is_none_for_text_prompt() -> None:
    agent = Agent(prompt=Prompt(messages=["go"], model="mock-model", provider="mock"))
    run = agent.run("go")
    assert run.parsed is None
    assert run.structured_output is None


def test_agent_output_type_parses_a_prompt_schema(mock_agent: Callable[..., Agent]) -> None:
    run = mock_agent('{"summary":"from agent"}', {"summary": str}, output_type=Plan).run("go")

    assert run.parsed == Plan(summary="from agent")


def test_run_output_type_overrides_the_agent_output_type(mock_agent: Callable[..., Agent]) -> None:
    class Other(pydantic.BaseModel):
        summary: str

    run = mock_agent('{"summary":"run-level"}', {"summary": str}, output_type=Other).run(
        "go", output_type=Plan
    )

    assert run.parsed == Plan(summary="run-level")


def test_answer_that_does_not_fit_the_output_class_is_refused(
    mock_agent: Callable[..., Agent],
) -> None:
    class Strict(pydantic.BaseModel):
        required_field: str

    with pytest.raises(WyrdError) as error:
        mock_agent('{"wrong_field":"value"}', Strict).run("go")

    assert error.value.code == "WYRD_AGENT_422_STRUCTURED_DECODE"


def test_plain_class_output_type_is_built_from_keywords(mock_agent: Callable[..., Agent]) -> None:
    class MyResult:
        def __init__(self, value: str) -> None:
            self.value = value

    run = mock_agent('{"value":"hello"}', {"value": str}, output_type=MyResult).run("go")

    assert run.parsed.value == "hello"

"""Prompt templates declare variables with ``${name}`` and ``{{name}}``."""

import pytest
from wyrd.prompt import Prompt, PromptCard, WyrdError


def test_dollar_brace_variables_auto_extract() -> None:
    prompt = Prompt(messages=["hello ${name}"], model="gpt-test", provider="openai")
    assert prompt.variables == ["name"]


def test_mustache_variables_auto_extract() -> None:
    prompt = Prompt(messages=["hello {{name}}"], model="gpt-test", provider="openai")
    assert prompt.variables == ["name"]


def test_mixed_syntax_auto_extracts_both() -> None:
    prompt = Prompt(messages=["${a} and {{b}}"], model="gpt-test", provider="openai")
    assert sorted(prompt.variables) == ["a", "b"]


def test_single_brace_is_not_a_placeholder() -> None:
    prompt = Prompt(messages=["hello {name}"], model="gpt-test", provider="openai")
    assert prompt.variables == []


def test_placeholder_outside_the_declared_variables_is_refused() -> None:
    with pytest.raises(WyrdError) as error:
        PromptCard(
            Prompt.openai_chat("gpt-4o", messages="hi {{name}}", variables=[])
        ).model_dump_json()

    assert error.value.code == "WYRD_PROMPT_422_UNDECLARED_PLACEHOLDER"

from __future__ import annotations

from wyrd import Prompt


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

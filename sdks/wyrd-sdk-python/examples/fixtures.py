"""Local fixtures for examples."""

from wyrd import Prompt


def mock_prompt(message: str) -> Prompt:
    """Return a mock-provider prompt for local examples."""
    return Prompt(provider="mock", model="mock-model", messages=[message])

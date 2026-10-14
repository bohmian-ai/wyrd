"""Wyrd's ``/mcp`` through the MCP clients of Python agent frameworks.

Each journey drives a framework's own public MCP API against the session
server the way an agent built on it would: the tools it would offer a model,
and a call through its own tool path. No model credentials are used; Pydantic
AI runs a scripted model. The official ``mcp`` client's ``tools/list`` is the
reference for what the server advertises to each caller.

The ``mcp-frameworks`` dependency group provides the frameworks, and only
``py:test:mcp-frameworks`` collects this directory.
"""

from __future__ import annotations

import asyncio
import json
import re

import httpx2
import pytest
from agents import Agent as OpenAIAgent
from agents import RunContextWrapper
from agents.mcp import MCPServerStreamableHttp
from agents.tool import FunctionTool
from agents.tool_context import ToolContext
from mcp import types
from mcp.client import Client
from mcp.client.streamable_http import streamable_http_client
from mcp.shared.exceptions import MCPError
from pydantic_ai import Agent as PydanticAgent
from pydantic_ai.exceptions import ModelRetry
from pydantic_ai.mcp import MCPToolset
from pydantic_ai.messages import (
    ModelMessage,
    ModelResponse,
    TextPart,
    ToolCallPart,
    ToolReturnPart,
)
from pydantic_ai.models.function import AgentInfo, FunctionModel
from wyrd.client import WyrdClient
from wyrd.testing import WyrdTestServer

pytestmark = pytest.mark.integration

PROVIDER_TOOL_NAME = re.compile(r"^[a-zA-Z0-9_-]{1,64}$")
"""The tool-name rule OpenAI and other providers apply to every function tool."""

GUIDE_ARGUMENTS = {"topic": "overview"}
"""The ``wyrd.guide`` call every journey makes."""

HIDDEN_WRITE = "verification.execute"
"""A write tool the reader is not shown and calls anyway."""

HIDDEN_WRITE_ARGUMENTS = {
    "subject_card_uid": "01890a5d-ac96-774b-bcce-b302099a8057",
    "verifier_uid": "01890a5d-ac96-774b-bcce-b302099a8058",
    "input": {"kind": "task_context", "context": {}},
}
"""Well-formed ``verification.execute`` arguments, so only authorization can refuse them."""


@pytest.fixture(scope="module")
def mcp_url(wyrd_server: WyrdTestServer) -> str:
    """The session server's MCP endpoint."""
    return f"{wyrd_server.base_url}/mcp"


@pytest.fixture(scope="module")
def admin_headers(wyrd_server: WyrdTestServer) -> dict[str, str]:
    """The administrator's bearer, to which every write tool is advertised."""
    return {"x-wyrd-access-token": f"Bearer {wyrd_server.access_token()}"}


@pytest.fixture(scope="module")
def viewer_headers(viewer_key: str) -> dict[str, str]:
    """A reader's bearer, to which no write tool is advertised."""
    return {"x-wyrd-access-token": f"Bearer {WyrdClient(credential=viewer_key).access_token()}"}


def advertised(url: str, headers: dict[str, str]) -> list[types.Tool]:
    """The tools the server advertises to ``headers``' caller, per the official client."""

    async def tools() -> list[types.Tool]:
        async with httpx2.AsyncClient(headers=headers) as http:
            async with Client(streamable_http_client(url, http_client=http)) as client:
                return (await client.list_tools()).tools

    return asyncio.run(tools())


def openai_server(url: str, headers: dict[str, str]) -> MCPServerStreamableHttp:
    """An OpenAI Agents SDK connection to Wyrd as ``headers``' caller."""
    return MCPServerStreamableHttp({"url": url, "headers": headers}, name="wyrd")


async def openai_tools(server: MCPServerStreamableHttp) -> list[FunctionTool]:
    """The function tools an OpenAI Agents SDK agent offers its model from ``server``.

    Strict schema conversion is on, and server-qualified names are the SDK's
    way to keep every name within the provider rule.
    """
    agent = OpenAIAgent(
        name="on-call",
        mcp_servers=[server],
        mcp_config={"convert_schemas_to_strict": True, "include_server_in_tool_names": True},
    )
    listed = await agent.get_all_tools(RunContextWrapper(None))
    return [tool for tool in listed if isinstance(tool, FunctionTool)]


def pydantic_run(url: str, headers: dict[str, str], call: str, arguments: dict) -> tuple:
    """Run a Pydantic AI agent whose scripted model calls ``call`` once, then answers.

    Returns the tool definitions the agent offered its model and the run's
    tool returns.
    """
    offered: list = []

    def model(messages: list[ModelMessage], info: AgentInfo) -> ModelResponse:
        if offered:
            return ModelResponse(parts=[TextPart("done")])
        offered.extend(info.function_tools)
        return ModelResponse(parts=[ToolCallPart(call, arguments)])

    agent = PydanticAgent(FunctionModel(model), toolsets=[MCPToolset(url, headers=headers)])
    result = agent.run_sync("What is failing?")
    returns = [
        part
        for message in result.all_messages()
        for part in message.parts
        if isinstance(part, ToolReturnPart)
    ]
    return offered, returns


def test_openai_agents_converts_every_tool(mcp_url: str, admin_headers: dict[str, str]) -> None:
    catalog = advertised(mcp_url, admin_headers)

    async def journey() -> tuple[list[FunctionTool], object]:
        async with openai_server(mcp_url, admin_headers) as server:
            tools = await openai_tools(server)
            (guide,) = [tool for tool in tools if tool.name == "mcp_wyrd__wyrd_guide"]
            payload = json.dumps(GUIDE_ARGUMENTS)
            context = ToolContext(
                context=None, tool_name=guide.name, tool_call_id="call-1", tool_arguments=payload
            )
            return tools, await guide.on_invoke_tool(context, payload)

    tools, output = asyncio.run(journey())

    assert len(tools) == len(catalog)
    assert [tool.name for tool in tools if not PROVIDER_TOOL_NAME.match(tool.name)] == []
    assert isinstance(output, dict)
    assert json.loads(output["text"])["topic"] == "overview"


def test_pydantic_ai_agent_calls_wyrd_guide(mcp_url: str, admin_headers: dict[str, str]) -> None:
    catalog = advertised(mcp_url, admin_headers)

    offered, returns = pydantic_run(mcp_url, admin_headers, "wyrd.guide", GUIDE_ARGUMENTS)

    assert [tool.name for tool in offered] == [tool.name for tool in catalog]
    (guide,) = returns
    assert guide.content["topic"] == "overview"


def test_openai_agents_reader_sees_and_reaches_no_write_tool(
    mcp_url: str, viewer_headers: dict[str, str]
) -> None:
    catalog = advertised(mcp_url, viewer_headers)
    assert all(tool.annotations and tool.annotations.read_only_hint for tool in catalog)

    async def journey() -> list[FunctionTool]:
        async with openai_server(mcp_url, viewer_headers) as server:
            with pytest.raises(MCPError, match="WYRD_PERMISSION_403_DENIED_RBAC"):
                await server.call_tool(HIDDEN_WRITE, HIDDEN_WRITE_ARGUMENTS)
            # The refusal leaves the connection usable.
            return await openai_tools(server)

    assert len(asyncio.run(journey())) == len(catalog)


def test_pydantic_ai_reader_sees_and_reaches_no_write_tool(
    mcp_url: str, viewer_headers: dict[str, str]
) -> None:
    catalog = advertised(mcp_url, viewer_headers)
    assert all(tool.annotations and tool.annotations.read_only_hint for tool in catalog)

    offered, _ = pydantic_run(mcp_url, viewer_headers, "wyrd.guide", GUIDE_ARGUMENTS)

    assert [tool.name for tool in offered] == [tool.name for tool in catalog]

    async def call_hidden() -> object:
        async with MCPToolset(mcp_url, headers=viewer_headers) as toolset:
            return await toolset.direct_call_tool(HIDDEN_WRITE, HIDDEN_WRITE_ARGUMENTS)

    # Pydantic AI hands a refused call back to the model as a retryable tool error.
    with pytest.raises(ModelRetry, match="WYRD_PERMISSION_403_DENIED_RBAC"):
        asyncio.run(call_hidden())

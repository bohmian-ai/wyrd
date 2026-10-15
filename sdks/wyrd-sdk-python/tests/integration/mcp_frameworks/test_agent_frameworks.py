"""Wyrd's ``/mcp`` through the MCP clients of Python agent frameworks.

Each journey drives a framework's own public MCP API against the session
server the way an agent built on it would: the tools it would offer a model,
and a call through its own tool path. No model credentials are used; Pydantic
AI and Google ADK run scripted models. The official ``mcp`` client's
``tools/list`` is the reference for what the server advertises to each caller.

Wyrd's dotted tool names are a public contract, so each framework is
configured the way its users would to forward provider-valid names: OpenAI
Agents SDK qualifies them with the server name, Pydantic AI renames its
toolset, and Google ADK, which has no renaming option, declares local renamed
copies. These are model-facing aliases only; each framework's MCP client still
calls the dotted wire tool.
"""

from __future__ import annotations

import asyncio
import copy
import json
import re
from collections.abc import AsyncGenerator
from typing import Any

import httpx2
import pytest
from agents import Agent as OpenAIAgent
from agents import RunContextWrapper
from agents.mcp import MCPServerStreamableHttp
from agents.tool import FunctionTool
from agents.tool_context import ToolContext
from google.adk.agents import LlmAgent
from google.adk.agents.llm_agent import ToolUnion
from google.adk.models.base_llm import BaseLlm
from google.adk.models.llm_request import LlmRequest
from google.adk.models.llm_response import LlmResponse
from google.adk.runners import InMemoryRunner
from google.adk.tools.base_tool import BaseTool
from google.adk.tools.mcp_tool import McpTool, McpToolset, StreamableHTTPConnectionParams
from google.adk.tools.mcp_tool.mcp_session_manager import MCPSessionManager
from google.genai import types as genai
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


@pytest.fixture
def adk_environment(monkeypatch: pytest.MonkeyPatch) -> None:
    """Google ADK settings for a non-Google MCP server.

    ADK reaches protocol 2026-07-28 only through its experimental modern MCP
    client. Its default mTLS probe looks up Google Cloud credentials before
    connecting, which off Google Cloud waits on a metadata-server timeout.
    """
    monkeypatch.setenv("ADK_ENABLE_MCP_MODERN_PROTOCOL", "1")
    monkeypatch.setenv("GOOGLE_API_USE_CLIENT_CERTIFICATE", "false")


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


def provider_name(name: str) -> str:
    """The provider-valid name a user forwards for Wyrd tool ``name``."""
    return name.replace(".", "_")


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


def pydantic_run(
    url: str, headers: dict[str, str], catalog: list[types.Tool], call: str, arguments: dict
) -> tuple:
    """Run a Pydantic AI agent whose scripted model calls ``call`` once, then answers.

    The toolset renames ``catalog`` to provider-valid names. Returns the tool
    definitions the agent offered its model and the run's tool returns.
    """
    offered: list = []

    def model(messages: list[ModelMessage], info: AgentInfo) -> ModelResponse:
        if offered:
            return ModelResponse(parts=[TextPart("done")])
        offered.extend(info.function_tools)
        return ModelResponse(parts=[ToolCallPart(call, arguments)])

    toolset = MCPToolset(url, headers=headers).renamed(
        {provider_name(tool.name): tool.name for tool in catalog}
    )
    agent = PydanticAgent(FunctionModel(model), toolsets=[toolset])
    result = agent.run_sync("What is failing?")
    returns = [
        part
        for message in result.all_messages()
        for part in message.parts
        if isinstance(part, ToolReturnPart)
    ]
    return offered, returns


def provider_named(tool: BaseTool) -> BaseTool:
    """A copy of ADK tool ``tool`` declared to the model under its provider-valid name.

    ADK declares a tool under ``BaseTool.name`` but its MCP client calls the
    server under the listed name, so the copy still reaches the Wyrd tool.
    """
    renamed = copy.copy(tool)
    renamed.name = provider_name(tool.name)
    return renamed


class ProviderNamedMcpToolset(McpToolset):
    """An ADK MCP toolset that declares each listed tool by ``provider_named``."""

    async def get_tools(self, readonly_context=None) -> list[BaseTool]:
        """The listed tools, each renamed by ``provider_named``."""
        return [provider_named(tool) for tool in await super().get_tools(readonly_context)]


def adk_run(
    tools: list[ToolUnion],
    connection: McpToolset | MCPSessionManager,
    calls: list[tuple[str, dict]],
) -> tuple:
    """Run a Google ADK agent whose scripted model makes ``calls`` in turn, then answers.

    ``connection`` owns the MCP sessions behind ``tools`` and is closed on the
    run's event loop. Returns the function declarations the agent sent its
    model and the run's function responses.
    """
    requests: list[LlmRequest] = []

    class ScriptedModel(BaseLlm):
        async def generate_content_async(
            self, llm_request: LlmRequest, stream: bool = False
        ) -> AsyncGenerator[LlmResponse, None]:
            requests.append(llm_request)
            if len(requests) <= len(calls):
                name, args = calls[len(requests) - 1]
                part = genai.Part(function_call=genai.FunctionCall(name=name, args=args))
            else:
                part = genai.Part(text="done")
            yield LlmResponse(content=genai.Content(role="model", parts=[part]))

    async def run() -> list[genai.FunctionResponse]:
        runner = InMemoryRunner(
            agent=LlmAgent(name="on_call", model=ScriptedModel(model="scripted"), tools=tools),
            app_name="on_call",
        )
        session = await runner.session_service.create_session(app_name="on_call", user_id="on-call")
        message = genai.Content(role="user", parts=[genai.Part(text="What is failing?")])
        try:
            return [
                response
                async for event in runner.run_async(
                    user_id="on-call", session_id=session.id, new_message=message
                )
                for response in event.get_function_responses()
            ]
        finally:
            await connection.close()

    responses = asyncio.run(run())
    declared = [
        declaration
        for tool in requests[0].config.tools or []
        if isinstance(tool, genai.Tool)
        for declaration in tool.function_declarations or []
    ]
    return declared, responses


def adk_toolset_run(url: str, headers: dict[str, str], call: str, arguments: dict) -> tuple:
    """``adk_run`` over every tool Wyrd advertises to ``headers``' caller, calling ``call`` once."""
    toolset = ProviderNamedMcpToolset(
        connection_params=StreamableHTTPConnectionParams(url=url, headers=headers)
    )
    return adk_run([toolset], toolset, [(call, arguments)])


def test_openai_agents_converts_every_tool(mcp_url: str, admin_headers: dict[str, str]) -> None:
    catalog = advertised(mcp_url, admin_headers)

    async def journey() -> tuple[list[FunctionTool], Any]:
        async with openai_server(mcp_url, admin_headers) as server:
            tools = await openai_tools(server)
            (guide,) = [tool for tool in tools if tool.name == "mcp_wyrd__wyrd_guide"]
            payload = json.dumps(GUIDE_ARGUMENTS)
            context = ToolContext(
                context=None, tool_name=guide.name, tool_call_id="call-1", tool_arguments=payload
            )
            return tools, await guide.on_invoke_tool(context, payload)

    tools, output = asyncio.run(journey())

    names = [tool.name for tool in tools]
    assert len(names) == len(catalog)
    assert len(set(names)) == len(names)
    assert [name for name in names if not PROVIDER_TOOL_NAME.match(name)] == []
    assert isinstance(output, dict)
    assert json.loads(output["text"])["topic"] == "overview"


def test_pydantic_ai_agent_calls_wyrd_guide(mcp_url: str, admin_headers: dict[str, str]) -> None:
    catalog = advertised(mcp_url, admin_headers)

    offered, returns = pydantic_run(
        mcp_url, admin_headers, catalog, provider_name("wyrd.guide"), GUIDE_ARGUMENTS
    )

    names = [tool.name for tool in offered]
    assert sorted(names) == sorted(provider_name(tool.name) for tool in catalog)
    assert len(set(names)) == len(names)
    assert [name for name in names if not PROVIDER_TOOL_NAME.match(name)] == []
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

    offered, _ = pydantic_run(
        mcp_url, viewer_headers, catalog, provider_name("wyrd.guide"), GUIDE_ARGUMENTS
    )

    assert sorted(tool.name for tool in offered) == sorted(
        provider_name(tool.name) for tool in catalog
    )

    async def call_hidden() -> object:
        async with MCPToolset(mcp_url, headers=viewer_headers) as toolset:
            return await toolset.direct_call_tool(HIDDEN_WRITE, HIDDEN_WRITE_ARGUMENTS)

    # Pydantic AI hands a refused call back to the model as a retryable tool error.
    with pytest.raises(ModelRetry, match="WYRD_PERMISSION_403_DENIED_RBAC"):
        asyncio.run(call_hidden())


@pytest.mark.usefixtures("adk_environment")
def test_google_adk_declares_every_tool(mcp_url: str, admin_headers: dict[str, str]) -> None:
    catalog = advertised(mcp_url, admin_headers)

    declared, responses = adk_toolset_run(
        mcp_url, admin_headers, provider_name("wyrd.guide"), GUIDE_ARGUMENTS
    )

    names = [d.name or "" for d in declared]
    assert sorted(names) == sorted(provider_name(tool.name) for tool in catalog)
    assert len(set(names)) == len(names)
    assert [name for name in names if not PROVIDER_TOOL_NAME.match(name)] == []
    (guide,) = responses
    assert guide.response is not None
    assert json.loads(guide.response["content"][0]["text"])["topic"] == "overview"


@pytest.mark.usefixtures("adk_environment")
def test_google_adk_reader_sees_and_reaches_no_write_tool(
    mcp_url: str, admin_headers: dict[str, str], viewer_headers: dict[str, str]
) -> None:
    catalog = advertised(mcp_url, viewer_headers)
    assert all(tool.annotations and tool.annotations.read_only_hint for tool in catalog)

    declared, _ = adk_toolset_run(
        mcp_url, viewer_headers, provider_name("wyrd.guide"), GUIDE_ARGUMENTS
    )

    assert sorted(d.name for d in declared) == sorted(provider_name(tool.name) for tool in catalog)

    # A descriptor obtained by the administrator lets the reader's agent name
    # the hidden tool; calling it still runs as the reader.
    (hidden,) = [tool for tool in advertised(mcp_url, admin_headers) if tool.name == HIDDEN_WRITE]
    (guide,) = [tool for tool in catalog if tool.name == "wyrd.guide"]
    sessions = MCPSessionManager(
        StreamableHTTPConnectionParams(url=mcp_url, headers=viewer_headers)
    )
    tools: list[ToolUnion] = [
        provider_named(McpTool(mcp_tool=tool, mcp_session_manager=sessions))
        for tool in (hidden, guide)
    ]

    _, responses = adk_run(
        tools,
        sessions,
        [
            (provider_name(HIDDEN_WRITE), HIDDEN_WRITE_ARGUMENTS),
            (provider_name("wyrd.guide"), GUIDE_ARGUMENTS),
        ],
    )

    # ADK hands Wyrd's refusal back to the model as a tool error, and the same
    # sessions keep serving the reader.
    refused, read = responses
    assert refused.response is not None
    assert "WYRD_PERMISSION_403_DENIED_RBAC" in refused.response["error"]
    assert read.response is not None
    assert json.loads(read.response["content"][0]["text"])["topic"] == "overview"

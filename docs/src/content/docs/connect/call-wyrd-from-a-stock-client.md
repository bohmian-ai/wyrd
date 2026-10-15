---
title: Call Wyrd from a stock client
description: Point the OpenAI, Anthropic, Gemini, Pydantic AI, or MCP client you already use at the Wyrd gateway and /mcp, authenticated by your Wyrd client.
pillar: wyrd
section: Connect
order: 4
draft: true
status: runnable
---

Provider SDKs and MCP clients do not need a Wyrd adapter. Build one `WyrdClient`, then give its HTTP transport to the stock client: `wyrd.httpAuth` in TypeScript (a `fetch`), `wyrd.http_auth` in Python (an `httpx.Auth` that also works as an `httpx2.Auth`, sync or async).

On every request the transport removes `Authorization`, `x-api-key`, and `x-goog-api-key`, and sends the client's current access token as `X-Wyrd-Access-Token: Bearer <token>`. The provider SDK still needs some key value, so pass a placeholder such as `"wyrd"`; the transport removes it before the request leaves your process. The token is fetched for each request, so a long-running process keeps working after any single token expires.

## Prerequisites

- A Wyrd credential the client can resolve: `WYRD_ACCESS_TOKEN`, workload identity, `WYRD_API_KEY`, or a saved `wyrd auth login`. With none, the first request fails with `WYRD_CLIENT_401_NO_CREDENTIALS`.
- `WYRD_SERVER_URL` set to your server.
- In Python, the `gateway` extra: `pip install 'wyrd-ai[gateway]'`.
- A gateway deployment for each model you call. Model names use the `<provider>/<model>` form, such as `openai/gpt-4o`.

## OpenAI

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
import openai
from wyrd import WyrdClient

wyrd = WyrdClient()
client = openai.OpenAI(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",
    http_client=openai.DefaultHttpxClient(auth=wyrd.http_auth),
)
```

For `openai.AsyncOpenAI`, use `openai.DefaultAsyncHttpxClient(auth=wyrd.http_auth)`.

</LangTab>
<LangTab lang="typescript">

```typescript
import OpenAI from "openai";
import { WyrdClient } from "wyrd-ai";

const wyrd = WyrdClient.connect();
const client = new OpenAI({ baseURL: `${wyrd.serverUrl}/v1`, apiKey: "wyrd", fetch: wyrd.httpAuth });
```

</LangTab>
</LangTabs>

The OpenAI Agents SDK builds on this client, so pass it to `OpenAIResponsesModel` or `OpenAIChatCompletionsModel` as usual. To trace those agents into Wyrd as well, see [Trace an agent framework](/build-and-verify/observe/record-a-run-and-observations/#trace-an-agent-framework).

## Anthropic

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
import anthropic

client = anthropic.Anthropic(
    base_url=wyrd.server_url,
    api_key="wyrd",
    http_client=anthropic.DefaultHttpxClient(auth=wyrd.http_auth),
)
```

</LangTab>
<LangTab lang="typescript">

```typescript
import Anthropic from "@anthropic-ai/sdk";

const client = new Anthropic({ baseURL: wyrd.serverUrl, apiKey: "wyrd", fetch: wyrd.httpAuth });
```

</LangTab>
</LangTabs>

## Gemini

```python
import httpx
from google import genai
from google.genai import types

client = genai.Client(
    api_key="wyrd",
    http_options=types.HttpOptions(
        base_url=wyrd.server_url, httpx_client=httpx.Client(auth=wyrd.http_auth)
    ),
)
```

## Pydantic AI

```python
import httpx2
from pydantic_ai import Agent
from pydantic_ai.mcp import MCPToolset
from pydantic_ai.models.openai import OpenAIChatModel
from pydantic_ai.providers.openai import OpenAIProvider

provider = OpenAIProvider(
    base_url=f"{wyrd.server_url}/v1",
    api_key="wyrd",
    http_client=httpx2.AsyncClient(auth=wyrd.http_auth),
)
agent = Agent(
    OpenAIChatModel("openai/gpt-4o", provider=provider),
    toolsets=[MCPToolset(f"{wyrd.server_url}/mcp", auth=wyrd.http_auth)],
)
```

## MCP

<LangTabs langs={['python', 'typescript']}>
<LangTab lang="python">

```python
import httpx2
from mcp import Client
from mcp.client.streamable_http import streamable_http_client

async with httpx2.AsyncClient(auth=wyrd.http_auth) as http:
    async with Client(streamable_http_client(f"{wyrd.server_url}/mcp", http_client=http)) as mcp:
        tools = await mcp.list_tools()
```

</LangTab>
<LangTab lang="typescript">

```typescript
import { Client, StreamableHTTPClientTransport } from "@modelcontextprotocol/client";
import { MCPServerStreamableHttp } from "@openai/agents-core";

// The official MCP SDK
const mcp = new Client({ name: "my-agent", version: "1.0.0" }, { versionNegotiation: { mode: "auto" } });
await mcp.connect(new StreamableHTTPClientTransport(new URL("/mcp", wyrd.serverUrl), { fetch: wyrd.httpAuth }));

// The OpenAI Agents SDK
const wyrdTools = new MCPServerStreamableHttp({ url: `${wyrd.serverUrl}/mcp`, fetch: wyrd.httpAuth });
```

</LangTab>
</LangTabs>

## Without a transport hook

If a client has no transport hook but accepts a callable API key, pass the access token as the key instead, for example `apiKey: () => wyrd.accessToken()` in the TypeScript OpenAI SDK. For shells and `apiKeyHelper`-style tools, `wyrd auth access-token` prints the current token followed by a newline.

## Fallback: a static Wyrd API key

Some tools take only a base URL and a static key: environment-only setups, curl, or a language without a Wyrd SDK. The gateway's inference routes (OpenAI-compatible `/v1`, Anthropic Messages, and Gemini `GenerateContent`) also accept a Wyrd API key wherever they accept a token, so these tools need no Wyrd import:

```bash
export OPENAI_BASE_URL=$WYRD_SERVER_URL/v1
export OPENAI_API_KEY=$WYRD_API_KEY
```

Anthropic clients send the key as `x-api-key` with `ANTHROPIC_BASE_URL=$WYRD_SERVER_URL`. Gemini clients send it as `x-goog-api-key` against `$WYRD_SERVER_URL`. The server exchanges the key for the key's own principal, exactly as `wyrd auth access-token` would. Every call is still authorized against that principal's permissions, and only the operator's provider credential travels upstream.

Use the access-token paths above when you can:

- The API key is long-lived, and this fallback sends it on every request, so every proxy, log, and process that sees the traffic can see it. An access token expires in minutes.
- The server reuses each key's exchange for up to 60 seconds. A revoked key keeps working for up to 60 seconds after its last exchange.
- Only the gateway's inference routes take a key this way. `/mcp`, the `/v1` management API, and Bifrost still require an access token.

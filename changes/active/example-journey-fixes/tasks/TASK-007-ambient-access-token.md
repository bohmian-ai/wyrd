# TASK-007 Ambient access-token helpers for stock clients

Spec: `changes/active/example-journey-fixes/spec.md` r5, REQ-008 and AC-005.

## Outcome

A developer points a stock OpenAI client and a stock MCP client at Wyrd with
only the ambient environment and no `WyrdClient` in their code:

```ts
import OpenAI from "openai";
import { MCPServerStreamableHttp } from "@openai/agents";
import { accessTokenSource, wyrdFetch } from "wyrd-ai";

const gateway = new OpenAI({ baseURL: `${process.env.WYRD_SERVER_URL}/v1`, apiKey: accessTokenSource() });
const mcp = new MCPServerStreamableHttp({ url: `${process.env.WYRD_SERVER_URL}/mcp`, fetch: wyrdFetch() });
```

```python
import httpx2
from openai import OpenAI
from wyrd.gateway import GatewayAuth

gateway = OpenAI(base_url=f"{server}/v1", api_key="wyrd", http_client=httpx2.Client(auth=GatewayAuth()))
```

## Scope

- `sdks/wyrd-sdk-ts/wyrd/src/index.ts`: add `accessTokenSource` and
  `wyrdFetch`, both resolving the client once through `clientOf`. Remove
  `gatewayFetch` and migrate its callers (unit test, gateway-inference
  integration test, `tests/support/local-development.ts`, any example or doc).
- `wyrdFetch` sets `X-Wyrd-Access-Token`, which every Wyrd HTTP route and the
  gateway accept. Note the gateway refuses a request carrying two credential
  headers (`extract_gateway_access_token`), so do not combine `wyrdFetch` with
  an OpenAI `apiKey` on gateway calls; document that on the export.
- `sdks/wyrd-sdk-python/python/wyrd/gateway/_auth.py` and its stub:
  `GatewayAuth(client: WyrdClient | None = None)`, resolving the ambient
  client once at construction. Follow the stub-generation rules in AGENTS.md
  section 8 (regenerate if generated; do not hand-edit generated stubs).
- Docs that mention `gatewayFetch` or `GatewayAuth(client)` in `docs/` and
  `examples/`.
- No server change.

## Evidence

- TS unit: `accessTokenSource()` and `wyrdFetch()` each resolve the client
  once and ask it for a token on every call (replace the `gatewayFetch` unit
  test).
- TS integration journey: a stock `openai` client with
  `apiKey: accessTokenSource()` completes a gateway chat call against
  `WyrdTestServer`; the official `@modelcontextprotocol/sdk` client over
  `wyrdFetch()` lists tools on `/mcp`; both configured only from the
  environment (`WYRD_SERVER_URL`, `WYRD_API_KEY`).
- Negative: a revoked API key is refused on both (client exchange error, not a
  provider error).
- Python integration: stock OpenAI client with `GatewayAuth()` and only the
  environment completes a gateway call.
- Lanes: `mise run fmt`, TS lint/typecheck and unit lanes, the TS integration
  lane for the touched tests, `mise run py:test:unit`, `mise run py:typecheck`,
  `mise run codegen:check`, `mise run docs:check` if docs changed.

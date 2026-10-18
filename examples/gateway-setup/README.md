# Gateway setup on a local server

Four named deployments, one per built-in provider, set up with the installed
`wyrd` CLI against a local development server. `gateway.json` and
`capture-policy.json` hold no secrets: each credential in `gateway.json` names
the environment variable holding its key (`local_env`), which the CLI reads
and sends once. The server seals the keys and applies every credential and
deployment in one transaction.

The `local_dev` journey
`sdks/wyrd-sdk-python/tests/integration/test_local_development.py::test_installed_cli_gateway_setup`
runs these exact commands and this `gateway.json` against local provider
fixtures.

1. Export your provider keys:

   ```bash
   export OPENAI_API_KEY=... ANTHROPIC_API_KEY=... GOOGLE_API_KEY=... TYPESAFE_API_KEY=...
   ```

2. Start the server with the gateway document and keep it running in its own
   terminal. On first use it also asks for anything it cannot find, such as
   the PostgreSQL URL:

   ```bash
   wyrd server dev --tenant acme --gateway-file gateway.json
   ```

   Or apply the document to a server that is already running. Reapplying
   converges; a refused document, such as one naming an unset variable,
   applies nothing:

   ```bash
   wyrd gateway deployment put --file gateway.json
   ```

3. Check the deployments and retain call metadata in `vala.gateway.calls`:

   ```bash
   wyrd gateway deployment list
   wyrd gateway capture-policy put --file capture-policy.json
   ```

   Credentials can also be stored one at a time, with
   `wyrd gateway credential put --file -` and a `managed_secret` piped on
   stdin; a JSON list of deployments then refers to them by name.

4. Call a deployment by name with any SDK signed in to the saved deployment.
   The model after the deployment name reaches the provider unchanged:

   | Deployment | Route | `model` |
   |------------|-------|---------|
   | `openai-prod` | `POST /v1/chat/completions` | `openai-prod/gpt-4o` |
   | `claude-prod` | `POST /v1/messages` | `claude-prod/claude-sonnet-4-5` |
   | `gemini-team` | `POST /v1beta/models/gemini-team/gemini-2.5-flash:generateContent` | in the path |
   | `judge-pool` | `POST /v1/systemone` | `judge-pool/jev-latest` |

5. Register one Agent per provider from `agents/`, download them, and call
   each downloaded Agent's deployment from your application. Each Agent's
   `execution_route` names its deployment; its Prompt keeps the provider and
   the model the upstream receives (`acme/gpt-4o-agent`, `claude-haiku-4-5`,
   `tuned/gemini-2.5-pro`, `jev-judge`).

   ```bash
   wyrd apply agents/service.yaml
   wyrd get --kind Service --name gateway-agents --version 1.0.0 --space default \
     --output-dir gateway-agents
   ```

   The application renders each Prompt and sends it to the ingress of the
   table above, addressed as `<deployment>/<model>`, with a Wyrd access token
   and the Run and Agent Card as correlation headers:

   ```python
   from urllib.parse import quote

   import httpx
   from wyrd.client import WyrdClient
   from wyrd.state import WyrdState

   client = WyrdClient()
   state = WyrdState.from_path("gateway-agents", client)
   for alias in ("openai", "anthropic", "google", "jev"):
       deployment = state.card(alias).spec["execution_route"]["deployment"]
       prompt = state.agent(alias).prompt().prompt
       body = prompt.render(question="Is Wyrd running?").model_dump()["body"]
       path = {
           "openai": "/v1/chat/completions",
           "anthropic": "/v1/messages",
           "google": f"/v1beta/models/{deployment}/{quote(prompt.model, safe='')}:generateContent",
           "jev": "/v1/systemone",
       }[prompt.provider]
       if prompt.provider != "google":
           body["model"] = f"{deployment}/{prompt.model}"
       reply = httpx.post(
           f"{client.server_url}{path}",
           json=body,
           headers={"wyrd-run-id": state.run(alias).run_id, "wyrd-card-uid": str(state.card_ref(alias).uid)},
           auth=client.http_auth,
       )
       print(alias, reply.json())
   ```

   Every call is retained in `vala.gateway.calls` under its `run_id` and the
   Agent's `card_uid`. The Jev deployment returns keyed `answers`.

`wyrd gateway credential revoke <name>` is permanent: every deployment using
that credential stops serving, and the name cannot be put again. To recover,
put the key under a new name and point the deployment's `auth.credential` at it.

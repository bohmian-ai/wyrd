# Gateway setup on a local server

Four named deployments, one per built-in provider, set up with the installed
`wyrd` CLI against a local development server. `gateway.json` and
`capture-policy.json` hold no secrets. Provider keys go into credential files you write outside the
repository; never commit them.

The `local_dev` journey
`sdks/wyrd-sdk-python/tests/integration/test_local_development.py::test_installed_cli_gateway_setup`
runs these exact commands and this `gateway.json` against local provider
fixtures.

1. Start the server and keep it running in its own terminal:

   ```bash
   wyrd server dev --tenant acme
   ```

2. Write one credential file per provider from your own keys:

   ```bash
   umask 077
   for provider in openai anthropic gemini jev; do
     key_var="$(echo "$provider" | tr a-z A-Z)_API_KEY"
     printf '{"name":"%s-key","provider":"%s","source":{"managed_secret":{"secret":"%s"}}}\n' \
       "$provider" "$provider" "${!key_var}" > "$provider-credential.json"
   done
   ```

3. Store the credentials, sealed by the server, apply the deployments, and
   retain call metadata in `vala.gateway.calls`. Reapplying converges; a
   malformed list or a repeated name writes nothing.

   ```bash
   wyrd gateway credential put --file openai-credential.json
   wyrd gateway credential put --file anthropic-credential.json
   wyrd gateway credential put --file gemini-credential.json
   wyrd gateway credential put --file jev-credential.json
   wyrd gateway deployment put --file gateway.json
   wyrd gateway deployment list
   wyrd gateway capture-policy put --file capture-policy.json
   ```

4. Call a deployment by name with any SDK signed in to the saved deployment.
   The model after the deployment name reaches the provider unchanged:

   | Deployment | Route | `model` |
   |------------|-------|---------|
   | `openai-prod` | `POST /v1/chat/completions` | `openai-prod/gpt-4o` |
   | `claude-prod` | `POST /v1/messages` | `claude-prod/claude-sonnet-4-5` |
   | `gemini-team` | `POST /v1beta/models/gemini-team/gemini-2.5-flash:generateContent` | in the path |
   | `judge-pool` | `POST /v1/systemone` | `judge-pool/jev-latest` |

5. Register one Agent per provider from `agents/`, download them, and run
   each downloaded Agent through its deployment. Each Agent's
   `execution_route` names its deployment; its Prompt keeps the provider and
   the model the upstream receives (`acme/gpt-4o-agent`, `claude-haiku-4-5`,
   `tuned/gemini-2.5-pro`, `jev-judge`).

   ```bash
   wyrd apply agents/service.yaml
   wyrd get --kind Service --name gateway-agents --version 1.0.0 --space default \
     --output-dir gateway-agents
   ```

   ```python
   from wyrd.client import WyrdClient
   from wyrd.state import WyrdState

   state = WyrdState.from_path("gateway-agents", WyrdClient())
   for alias in ("openai", "anthropic", "google", "jev"):
       run = state.run(alias)
       print(alias, run.invoke({"question": "Is Wyrd running?"}))
   ```

   Every call is retained in `vala.gateway.calls` under its `run_id` and the
   Agent's `card_uid`. The Jev Agent returns its `answers` as JSON text and
   takes no tools.

`wyrd gateway credential revoke <name>` is permanent: every deployment using
that credential stops serving, and the name cannot be put again. To recover,
put the key under a new name and point the deployment's `auth.credential` at it.

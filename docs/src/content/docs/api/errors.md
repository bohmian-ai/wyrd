---
title: Error codes
description: Every stable Wyrd error code with its HTTP status, title, and fix.
pillar: wyrd
section: Reference
order: 21
status: runnable
---

Every Wyrd error carries a stable `code`, such as `WYRD_SPEC_400_VALIDATION`: the area, the HTTP status, and the cause. Branch on `code`, never on the message. The same code reaches you over HTTP, MCP, the CLI, and every SDK.

Over HTTP an error is an RFC 9457 `application/problem+json` body:

| Field | Meaning |
| --- | --- |
| `code` | The stable code from the tables below |
| `status` | The HTTP status |
| `title` | A fixed summary of the code |
| `detail` | What went wrong in this request |
| `remediation` | What to do about it |
| `details` | Fields specific to the error, such as `rule` and `path` for a validation failure |

This page is generated from the error definitions in the Wyrd source.

## WYRD_AGENT

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_AGENT_404_PROMPT_CARD` | 404 | Prompt Card not found | Load or register the referenced Prompt Card in the local prompt registry before resolving the agent. |
| `WYRD_AGENT_404_RUNTIME_LOCAL_TOOL_NOT_FOUND` | 404 | Runtime-local tool not found | Register the named tool in the local Skald tool registry before loading the Agent Card. |
| `WYRD_AGENT_404_TOOL` | 404 | Tool not registered for agent | Register the tool before running the agent or remove the tool call from the provider response. |
| `WYRD_AGENT_404_TOOL_NOT_IN_AGENT` | 404 | Tool not attached to agent | Attach the runtime-local tool to this agent or adjust the model response. |
| `WYRD_AGENT_409_PROVIDER_MISMATCH` | 409 | Provider mismatch | Run the agent with a prompt targeting the same provider as the agent's resolved prompt. |
| `WYRD_AGENT_412_DELEGATION_DEPTH` | 412 | Agent delegation depth exceeded | Reduce nested agent-as-tool calls or restructure the workflow so the delegation chain stays within the cap. |
| `WYRD_AGENT_422_CALLBACK_RETURN_TYPE` | 422 | Callback returned wrong type | Return None to continue, return a replacement value of the expected type, or raise an exception to abort the run. |
| `WYRD_AGENT_422_INVALID_ARGUMENT` | 422 | Invalid argument | Correct the argument value; see details for the expected type or constraint. |
| `WYRD_AGENT_422_LOOP_MESSAGE_TYPE` | 422 | Loop message type mismatch | Re-render the conversation message against the active provider; loop messages must match the provider's message schema (OpenAi / Anthropic / Gemini / Custom). |
| `WYRD_AGENT_422_MISSING_NAME` | 422 | AgentCard name is missing | Set a card name before saving or projecting the agent to a card envelope. |
| `WYRD_AGENT_422_MISSING_VERSION` | 422 | AgentCard version is missing | Set a concrete semantic version before saving or projecting the agent to a card envelope. |
| `WYRD_AGENT_422_PROMPT` | 422 | Agent prompt rendering failed | Inspect the prompt template and the variables passed to the agent run. |
| `WYRD_AGENT_422_RUNTIME_LOCAL_TOOLS_NOT_REGISTRABLE` | 422 | Runtime-local tools are not registrable | Remove runtime-local tool names before registration; local save, load, and run remain available. |
| `WYRD_AGENT_422_STRUCTURED_DECODE` | 422 | Structured response decode failed | Inspect the model output; structured-output prompts must return a JSON object. |
| `WYRD_AGENT_422_TOOL_ARGS` | 422 | Tool arguments invalid | Adjust the provider-emitted tool arguments to match the tool input schema. |
| `WYRD_AGENT_422_VALIDATION` | 422 | AgentCard validation failed | Fix the Agent Card envelope, metadata, prompt reference, tools, or run_config fields. |
| `WYRD_AGENT_499_CALLBACK_ABORTED` | 499 | Callback aborted run | Inspect the callback exception and either return None to continue or return a valid replacement value. |
| `WYRD_AGENT_500_CALLBACK_PANIC` | 500 | User-supplied callback panicked | Inspect the panic payload and fix the callback implementation; callbacks must not panic. |
| `WYRD_AGENT_500_JOURNAL` | 500 | Journal append failed | Inspect the journal backend; for the no-op journal this should never fire. |
| `WYRD_AGENT_500_MAX_ITERATIONS` | 500 | Agent exceeded max iterations | Raise the run's max_iterations or adjust the agent prompt and tools so the loop can terminate. |
| `WYRD_AGENT_502_PROVIDER` | 502 | Provider call failed | Inspect the provider backend and retry once it is healthy. |
| `WYRD_AGENT_504_TIMEOUT` | 504 | Agent run exceeded configured timeout | Increase the run timeout or reduce iteration count and tool latency. |

## WYRD_AUTH

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_AUTH_400_BAD_TOKEN_FORMAT` | 400 | Access token header malformed | Use `X-Wyrd-Access-Token: Bearer &lt;token&gt;` with a single Bearer credential. |
| `WYRD_AUTH_400_DELEGATION_DEPTH_EXCEEDED` | 400 | Delegation chain exceeds maximum depth | Reduce the delegation chain; the caller's existing `act` chain plus the requested hop must be at most 5 layers. |
| `WYRD_AUTH_400_DEVICE_AUTHORIZATION` | 400 | Device authorization did not issue a credential | Keep polling on `authorization_pending`, poll five seconds slower on `slow_down`, and start a new login on any other `details.error`. |
| `WYRD_AUTH_400_HUMAN_CONNECTION_REQUIRED` | 400 | Human login uses the tenant OIDC connection | Trusted issuers and `[[trusted_issuers]]` accept only `principal_kind = workload`. Configure human sign-in through the tenant OIDC connection API: PUT /v1/identity/oidc/candidate, POST /v1/identity/oidc/candidate/test, then POST /v1/identity/oidc/candidate/activate. |
| `WYRD_AUTH_400_INVALID_CARD_REF_VERSION` | 400 | Non-human principal CardRef.version must be an exact Pin | Re-issue the request with `card_ref.version` set to an exact `Pin`, for example `1.2.3`; version requirements are rejected for card-bound principals. |
| `WYRD_AUTH_400_INVALID_NONCE` | 400 | Invalid OIDC nonce | Restart the login flow. The ID token nonce must match the server-stored login nonce. |
| `WYRD_AUTH_400_INVALID_STATE` | 400 | Invalid OIDC state | Restart the login flow. The callback state is single-use and must match the server-stored login state. |
| `WYRD_AUTH_400_PRINCIPAL_KIND_CARD_KIND_MISMATCH` | 400 | CardRef cannot be bound to a non-human principal | Only Service and Agent cards bind to non-human principals. Re-issue the request with a `card_ref` referencing a Service or Agent card. |
| `WYRD_AUTH_400_UNSUPPORTED_CLIENT_AUTH` | 400 | Unsupported OIDC client authentication | Use `SecretBasic` or `SecretPost` with a client secret, or `Public` without one. `private_key_jwt` is not supported for tenant connections. |
| `WYRD_AUTH_400_UNSUPPORTED_GRANT_TYPE` | 400 | Unsupported grant_type | Use one of the supported grants listed in `details.supported_grant_types`. |
| `WYRD_AUTH_401_API_KEY_INVALID` | 401 | API key not found, revoked, or hash mismatch | Re-issue a key with `wyrd auth issue-key &lt;card_ref&gt;` and update the deployment secret. |
| `WYRD_AUTH_401_CREDENTIAL_REVOKED` | 401 | Credential revoked | The credential was explicitly revoked. Re-authenticate or request a new credential. |
| `WYRD_AUTH_401_DELEGATION_DEPTH_EXCEEDED` | 401 | Delegation chain in token exceeds maximum depth | The presented JWT's `act` chain exceeds the verifier's maximum delegation depth. Re-issue from a shorter chain. |
| `WYRD_AUTH_401_INVALID_CARD_REF` | 401 | Non-User token card_ref claim absent or malformed | Re-issue the token via `POST /auth/token`; ensure the bound non-human principal has a valid structured card_ref. |
| `WYRD_AUTH_401_INVALID_TOKEN` | 401 | Invalid token | Re-authenticate to obtain a valid token signed by the current issuer key. |
| `WYRD_AUTH_401_REFRESH_REUSED` | 401 | Refresh token reused | Re-authenticate. The refresh token family has been revoked because a rotated token was reused. |
| `WYRD_AUTH_401_REFRESH_REVOKED` | 401 | Refresh token revoked | Re-authenticate to obtain a new refresh token. |
| `WYRD_AUTH_401_TOKEN_EXPIRED` | 401 | Token expired | Re-authenticate to obtain a fresh token. |
| `WYRD_AUTH_401_UNAUTHENTICATED` | 401 | Not authenticated | Present a valid Wyrd token as `X-Wyrd-Access-Token: Bearer &lt;token&gt;`. |
| `WYRD_AUTH_403_ORIGIN_REFUSED` | 403 | Request origin refused | Send browser-session requests from the Wyrd application's own origin; cross-site and origin-less browser-session mutations are refused. |
| `WYRD_AUTH_403_PRINCIPAL_ORPHANED` | 403 | The backing Service or Agent card has been soft-deleted | Re-declare the card or use a different principal to obtain a token. |
| `WYRD_AUTH_403_UNREPRESENTABLE_DELEGATION` | 403 | Delegated permissions cannot be represented | The subject's and actor's shared permissions cannot be represented by token scopes, so the exchange issued no token. Exchange with grants whose exact shared authority fits the supported scopes, such as both parties holding the same explicit gateway deployment or model grant. |
| `WYRD_AUTH_404_ADMIN_NOT_FOUND` | 404 | Admin resource not found in tenant | Confirm the issuer URL (and subject for bindings) and that the resource exists in the current tenant. |
| `WYRD_AUTH_404_PRINCIPAL_NOT_FOUND` | 404 | Requested principal not found in tenant | The principal named by the `subject_token` or `actor_token` no longer exists in the current tenant. Re-authenticate both parties and retry the exchange. |
| `WYRD_AUTH_409_ADMIN_CONFLICT` | 409 | Admin resource already exists or is still referenced | A trusted issuer or workload binding with this key already exists, or the issuer still has live bindings. Use a distinct key, or delete the bindings first (or with --cascade). |
| `WYRD_AUTH_409_CONNECTION_CONFLICT` | 409 | Tenant OIDC connection conflict | Re-read GET /v1/identity/oidc/connections and retry with the current `expected_revision`. Activation also requires a valid recovery API key for a headless principal of this tenant that holds `identity_connections:write`. |
| `WYRD_AUTH_409_CONNECTION_NOT_TESTED` | 409 | Tenant OIDC candidate is not freshly tested | Run POST /v1/identity/oidc/candidate/test against this exact candidate revision, complete the provider sign-in at the returned authorization_url, and activate within 15 minutes of that successful test. |
| `WYRD_AUTH_413_CARD_SCOPE_TOO_LARGE` | 413 | Card scope too large | Reduce or split the principal's declared card-ref graph so its transitive observation-target scope fits the token size limit, or issue a narrower card-bound principal. |
| `WYRD_AUTH_503_DISCOVERY_UNAVAILABLE` | 503 | OIDC discovery unavailable | Retry with backoff. Do not re-authenticate; this is an infrastructure or issuer-discovery failure. |
| `WYRD_AUTH_503_VERIFY_UNAVAILABLE` | 503 | Auth verify backend unavailable | Retry with backoff. Do not re-authenticate; this is an infrastructure failure, not a credential failure. |

## WYRD_CFG

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_CFG_400_INVALID_TOML` | 400 | wyrd.toml failed TOML syntax parse | Check the TOML syntax in your wyrd.toml file, or the file path if the read itself failed. |
| `WYRD_CFG_400_NAME_DEFAULT_REJECTED` | 400 | wyrd.toml cannot default `name` | Remove the `name` key from [defaults] or [kind.&lt;X&gt;]. Every card must author its own name. |
| `WYRD_CFG_400_SCHEMA_MISMATCH` | 400 | wyrd.toml schema mismatch | Compare the file against the documented wyrd.toml schema in architecture/wyrd-design.md. |

## WYRD_CLIENT

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_CLIENT_400_CONFIG_INVALID` | 400 | Client configuration is invalid | Correct the client configuration field named in details and rebuild the client. |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | 401 | No client credentials available | Set WYRD_ACCESS_TOKEN, WYRD_WORKLOAD_TOKEN with WYRD_TENANT, or WYRD_API_KEY, pass an explicit client credential, or add a default api_key to the Wyrd credentials file. |
| `WYRD_CLIENT_401_SAVED_LOGIN_UNUSABLE` | 401 | Saved user login cannot be used | Run `wyrd auth login --server &lt;url&gt; --tenant &lt;tenant&gt;` again, select the intended tenant with the client tenant option or WYRD_TENANT, or pass an explicit credential. The details reason names the failure. |
| `WYRD_CLIENT_413_PAYLOAD_TOO_LARGE` | 413 | Sealed batch exceeds the client message ceiling | Reduce the row or batch size, or raise max_message_bytes within the server's accepted ceiling. |
| `WYRD_CLIENT_422_ROW_DESERIALIZATION` | 422 | Query row deserialization failed | Align the caller's row type with the columns and types the query selects. |
| `WYRD_CLIENT_429_QUEUE_FULL` | 429 | Client ingestion queue is saturated | No row of the refused record was admitted: flush or back off, then resubmit the same record. Sustained refusals mean the producer outpaces delivery; every refusal bumps the drop counter. |
| `WYRD_CLIENT_503_TRANSPORT_DOWN` | 503 | Client transport is unavailable | Verify the endpoint, DNS, TLS material, and network reachability, then retry once the transport recovers. |
| `WYRD_CLIENT_504_FLUSH_TIMEOUT` | 504 | Client flush exceeded its drain deadline | Increase flush_timeout_ms or reduce the in-flight batch size so the drain can complete. |

## WYRD_CLI

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_CLI_400_CARD_LOAD` | 400 | Card load failed | Fix the card files and local references, then retry. |
| `WYRD_CLI_400_CLIENT_CONFIG` | 400 | Client configuration failed | Pass a valid server URL and configure a Wyrd credential. |
| `WYRD_CLI_400_DELETE_SELECTOR_EXACT` | 400 | Delete selector is not exact | Pass --uid, or pass --kind, --name, and --version. |
| `WYRD_CLI_400_INVALID_ARGUMENT` | 400 | Invalid CLI argument | Fix the argument and retry. |
| `WYRD_CLI_400_MCP_HOST_SELECTION` | 400 | MCP host selection required | Pass --host for each host to connect (codex, claude-code, copilot-cli, vscode, cursor, pi, hermes); install and open the host first if none is detected. |
| `WYRD_CLI_400_QUERY_CONFIG` | 400 | Query configuration missing | Pass --server or set WYRD_SERVER_URL; the credential comes from the ambient chain. |
| `WYRD_CLI_400_SERVER_HOST_UNSUPPORTED` | 400 | Server host unsupported | Run the server on macOS on aarch64 or Linux (glibc) on x86_64 or aarch64, or use the official container image. |
| `WYRD_CLI_400_USAGE` | 400 | Invalid command usage | Correct the arguments; run the command with --help for its accepted arguments. |
| `WYRD_CLI_401_NO_CREDENTIALS` | 401 | No Wyrd credentials | Set WYRD_ACCESS_TOKEN, configure workload identity or WYRD_API_KEY, or add [default].api_key to ~/.config/wyrd/credentials.toml. |
| `WYRD_CLI_401_NO_PLATFORM_CREDENTIAL` | 401 | No platform credential | Set WYRD_PLATFORM_CREDENTIAL to a platform credential for this deployment. |
| `WYRD_CLI_401_NO_REFRESH_TOKEN` | 401 | No refresh token | Set WYRD_REFRESH_TOKEN to the refresh token to rotate. |
| `WYRD_CLI_409_MCP_HOST_INSTALL` | 409 | MCP host installation failed | Fix the problem reported for each failed host, then rerun; hosts reported as added, updated, or unchanged are already connected. |
| `WYRD_CLI_409_SERVER_INSTALL_INCOMPLETE` | 409 | Server installation incomplete | Remove the named version directory, then rerun `wyrd server install`; nothing was activated. |
| `WYRD_CLI_409_SERVER_VERSION_INCOMPATIBLE` | 409 | Server version incompatible | Upgrade the wyrd CLI to the server's version line (uv add --upgrade wyrd-ai, or pnpm add wyrd-ai@latest), then retry. |
| `WYRD_CLI_422_SERVER_RELEASE_UNVERIFIED` | 422 | Server release unverified | Retry the install; if it fails again, report the release. The current installation is unchanged. |
| `WYRD_CLI_500_IO` | 500 | CLI filesystem IO failed | Check file paths and permissions, then retry. |
| `WYRD_CLI_500_OUTPUT` | 500 | CLI output failed | Retry the command or use --format text. |
| `WYRD_CLI_500_QUERY` | 500 | Oracle query failed | Inspect the stable query error and terminal diagnostics, then retry. |
| `WYRD_CLI_503_CLIENT_TRANSPORT` | 503 | Wyrd server unavailable | Check the server URL and network connectivity, then retry. |
| `WYRD_CLI_503_MCP_PROXY` | 503 | MCP proxy failed | Check that the Wyrd server URL is reachable and serves /mcp, and that the host launches `wyrd mcp proxy` over stdio, then reconnect the host. |
| `WYRD_CLI_503_SERVER_DEV` | 503 | Local server setup failed | Follow the stage's recovery guidance and rerun with the same tenant and configuration directory; active tenant credentials are never reissued. |
| `WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE` | 503 | Server release unavailable | Check network access to github.com, then retry; the current installation is unchanged. |

## WYRD_DATA

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_DATA_400_INTERFACE_METADATA_REQUIRED` | 400 | DataCard interface metadata required | Pass an explicit DataInterface with enough metadata to describe the data source. |
| `WYRD_DATA_400_INVALID_INTERFACE_OPTION` | 400 | Invalid DataCard interface option | Use one of the supported interface option values. |
| `WYRD_DATA_400_INVALID_SPLIT_RULE` | 400 | Invalid DataCard split rule | Use a supported split operator and reference schema columns that exist. |
| `WYRD_DATA_400_TARGET_COLUMN_UNKNOWN` | 400 | Unknown DataCard target column | Declare target columns that exist in the DataCard schema. |
| `WYRD_DATA_400_UNKNOWN_DATA_TYPE` | 400 | Unknown DataCard data type | Pass a supported data object, path, or explicit DataInterface. |
| `WYRD_DATA_400_VALIDATION` | 400 | DataCard validation failed | Fix the DataCard schema, artifact, stats, or interface metadata. |

## WYRD_DRIFT

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_DRIFT_400_PROFILE_REQUIRED` | 400 | Drift profile required | Attach the method-matching DriftProfile (PsiProfile, SpcProfile, or CustomProfile). |
| `WYRD_DRIFT_400_SIGNAL_METHOD_MISMATCH` | 400 | Drift signal/method mismatch | Use a signal variant supported by the chosen DriftMethod (see DriftSpec docs). |
| `WYRD_DRIFT_400_VALIDATION` | 400 | Drift Verifier validation failed | Fix the drift implementation profile, condition, or signal fields. |

## WYRD_GATEWAY

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_GATEWAY_400_INVALID_CONFIGURATION` | 400 | Invalid gateway configuration | Correct the field named in `details.field`. Path and body names must match, credential bindings and secret backends must be configured on the server, and deployments must reference an active credential for the same provider. |
| `WYRD_GATEWAY_400_INVALID_REQUEST` | 400 | Invalid gateway request | Correct the field named in `details.field`. Models use the `&lt;provider&gt;/&lt;model&gt;` projection and a fallback override is a non-empty, duplicate-free list that excludes the requested model. |
| `WYRD_GATEWAY_404_MODEL_UNAVAILABLE` | 404 | Gateway model unavailable | Request a model the tenant has deployed for this operation, or ask a tenant administrator to add a deployment or fallback candidate. |
| `WYRD_GATEWAY_404_RESOURCE_NOT_FOUND` | 404 | Gateway resource not found in tenant | List provider credentials or deployments to confirm the name exists in the current tenant. |
| `WYRD_GATEWAY_409_RESOURCE_CONFLICT` | 409 | Gateway resource conflicts with its current state | A revoked credential cannot be replaced, a credential referenced by a deployment cannot be deleted, a pricing version's content is immutable, and an identical batch creation still pending cannot be repeated. Use a new name or version, remove the referencing deployment first, or vary the batch metadata. |
| `WYRD_GATEWAY_422_COST_UNBOUNDED` | 422 | Gateway call cost cannot be bounded | A budget applies or unknown cost is rejected, so the call needs active pricing for its model and a bounded request such as a maximum output token count. |
| `WYRD_GATEWAY_429_BUDGET_EXCEEDED` | 429 | Gateway budget exceeded | Wait for the next budget period, request a smaller bounded call, or ask a tenant administrator to raise the budget. |
| `WYRD_GATEWAY_429_LIMIT_EXCEEDED` | 429 | Gateway limit exceeded | Retry after `details.retry_after_seconds`, reduce concurrency, or ask a tenant administrator to raise the limit. |
| `WYRD_GATEWAY_502_UPSTREAM_UNAVAILABLE` | 502 | Gateway upstream unavailable | Retry with backoff. If failures persist, a tenant administrator should check the deployment's provider, credential, and fallback configuration. |
| `WYRD_GATEWAY_504_DEADLINE_EXCEEDED` | 504 | Gateway deadline exceeded | Retry with a longer deadline or a smaller request. |

## WYRD_LOADER

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_LOADER_400_CONFIG_LOAD_FAILED` | 400 | Config load failed | Check wyrd.toml syntax and structure. |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | 400 | Invalid card envelope | Ensure the card has apiVersion, kind, metadata, and spec fields. |
| `WYRD_LOADER_400_IO` | 400 | Loader IO error | Check file permissions and that all referenced files exist. |
| `WYRD_LOADER_400_PATH_ABSOLUTE_ADVISORY` | 400 | Absolute path reference breaks portability | Use relative paths for portability across environments. |
| `WYRD_LOADER_400_PATH_ESCAPE` | 400 | Path reference escapes workspace | Use relative paths within the workspace root. |
| `WYRD_LOADER_400_YAML_SYNTAX` | 400 | YAML syntax error | Fix the YAML syntax error and retry. |

## WYRD_MODEL

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_MODEL_400_CUSTOM_LOADER_INVALID` | 400 | Invalid custom loader | Set CustomMeta.loader_module and CustomMeta.loader_class to non-empty values. |
| `WYRD_MODEL_400_DTYPE_NORMALIZE_FAILED` | 400 | Could not normalize signature dtype | Provide a FieldSpec with a canonical Arrow logical dtype string. |
| `WYRD_MODEL_400_HF_REVISION_INVALID` | 400 | Invalid HuggingFace revision | Pass a 7-40 character lowercase hex revision from the HuggingFace repo. |
| `WYRD_MODEL_400_HF_TASK_MISSING` | 400 | Missing HuggingFace task | Set HuggingfaceMeta.hf_task before saving. |
| `WYRD_MODEL_400_MISSING_SIGNATURE` | 400 | Missing model signature | Pass signature fields explicitly or build a ModelSignature before declaration. |
| `WYRD_MODEL_400_SHAPE_INVALID` | 400 | Invalid model signature shape | Use Dim::Fixed(n) with n &gt; 0 or Dim::Dynamic(name) for unknown dimensions. |
| `WYRD_MODEL_400_UNKNOWN_MODEL_TYPE` | 400 | Unknown ModelCard model type | Pass a supported model object or an explicit ModelInterface. |
| `WYRD_MODEL_400_VALIDATION` | 400 | ModelCard validation failed | Fix the ModelCard signature, framework version, or interface configuration. |
| `WYRD_MODEL_501_SERIALIZER_UNAVAILABLE` | 501 | Required serializer unavailable | Install the Wyrd Python extra for the required framework serializer. |

## WYRD_OPERATOR

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_OPERATOR_400_INVALID_CONNECTION` | 400 | Invalid Operator connection | Send the provider-tagged shape matching the connection's provider with a non-empty secret and a normalized https origin. |
| `WYRD_OPERATOR_404_CONNECTION_NOT_FOUND` | 404 | Operator connection not found | List GET /v1/operator-connections in the same tenant and retry with one of those connection IDs. |
| `WYRD_OPERATOR_409_CONNECTION_CONFLICT` | 409 | Operator connection name conflict | Choose another name, or PATCH the existing connection to rotate its secret. |
| `WYRD_OPERATOR_503_KEY_UNAVAILABLE` | 503 | Operator connection key unavailable | Restore the configured Operator key provider so it can supply the active key, then retry. |

## WYRD_ORIGIN

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_ORIGIN_400_VALIDATION` | 400 | Card origin validation failed | Provide a non-empty repo and a 7-40 character lowercase hex commit; omit empty path values. |

## WYRD_PERMISSION

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_PERMISSION_401_UNAUTHENTICATED` | 401 | Authentication required | Send a valid `X-Wyrd-Access-Token: Bearer &lt;token&gt;` header before invoking permission-protected routes. |
| `WYRD_PERMISSION_403_DENIED_RBAC` | 403 | Permission denied (RBAC) | Request the required role from a workspace admin. |
| `WYRD_PERMISSION_500_ROLE_CORRUPT` | 500 | Role row's permissions JSONB is corrupt | Inspect the offending row in `wyrd.auth_roles`; restore or rewrite the permissions JSONB with a valid permission array. This is not retryable. |

## WYRD_PROMPT

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_PROMPT_400_EMPTY_MODEL` | 400 | Prompt model is empty | Set prompt.model to a non-empty provider model id. |
| `WYRD_PROMPT_400_INVALID` | 400 | Invalid Prompt field | Fix the field named in details.field: author either portable system/messages/settings/response_format or native format/body, with provider and model once. |
| `WYRD_PROMPT_400_INVALID_MEDIA_TYPE` | 400 | Invalid media type | Supply the required MIME type and a provider-supported URI. |
| `WYRD_PROMPT_400_INVALID_RESPONSE_SCHEMA` | 400 | JsonSchema response_type schema is not an object | The schema of ResponseType::JsonSchema must be a JSON object. |
| `WYRD_PROMPT_400_INVALID_VARIABLE_NAME` | 400 | Invalid variable name | Variable names must match ^[a-zA-Z_][a-zA-Z0-9_]*$. |
| `WYRD_PROMPT_400_LOADER_BAD_EXTENSION` | 400 | Loader extension not supported | Use .json, .yaml, or .yml. |
| `WYRD_PROMPT_400_MEDIA_INVALID_EXTENSION` | 400 | Media file extension is unsupported | Use a supported image or document extension. |
| `WYRD_PROMPT_400_MEDIA_IN_SYSTEM_MESSAGE` | 400 | Media placeholder in system message | Move media placeholders to user or assistant content. |
| `WYRD_PROMPT_400_MEDIA_NOT_REGULAR_FILE` | 400 | Media path is not a regular file | Pass a regular local file path. |
| `WYRD_PROMPT_400_MEDIA_TOO_LARGE` | 400 | Media file too large | Use a smaller media file or upload it to the provider and bind a file URI. |
| `WYRD_PROMPT_400_PROVIDER_MISMATCH` | 400 | Request shape did not match the dialed provider | Pick the matching provider client or dispatch through the runtime registry. |
| `WYRD_PROMPT_400_SETTINGS_DECODE` | 400 | model_settings decode failed | Check model_settings field names and types; unknown fields are allowed and pass through. |
| `WYRD_PROMPT_400_SETTINGS_PROVIDER_MISMATCH` | 400 | model_settings provider mismatch | Pass the settings type for this provider, or pass a dict. |
| `WYRD_PROMPT_400_UNSUPPORTED_MEDIA_FOR_PROVIDER` | 400 | Unsupported media for provider | Use a media source supported by the selected provider. |
| `WYRD_PROMPT_401_PROVIDER_AUTH` | 401 | Provider authentication failed | Check the provider credentials environment variables. |
| `WYRD_PROMPT_422_INVALID_OUTPUT_SCHEMA` | 422 | Invalid output schema | Pass a dict[str, type], raw JSON Schema object, ResponseFormat, or pydantic BaseModel subclass. |
| `WYRD_PROMPT_422_MEDIA_PLACEHOLDER_NOT_ISOLATED` | 422 | Media placeholder is not isolated | Ensure each `${media:name}` token occupies its own native text part before binding. |
| `WYRD_PROMPT_422_MISSING_MEDIA_VARIABLE` | 422 | Media variable missing | Bind every declared media variable before rendering the prompt. |
| `WYRD_PROMPT_422_MISSING_VARIABLE` | 422 | Render variable missing | Supply every declared variable when calling Prompt::render. |
| `WYRD_PROMPT_422_PYDANTIC_REQUIRED` | 422 | Pydantic required | Install pydantic or use the stdlib dict[str, type] output schema path. |
| `WYRD_PROMPT_422_RESPONSE_DECODE` | 422 | Response did not match requested schema | Inspect the provider response or relax the prompt response schema. |
| `WYRD_PROMPT_422_UNDECLARED_MEDIA_PLACEHOLDER` | 422 | Undeclared media placeholder in request | Bind only media placeholders the Prompt references as `${media:name}`. |
| `WYRD_PROMPT_429_PROVIDER_RATE_LIMIT` | 429 | Provider rate limit | Back off, lower concurrency, or upgrade provider tier. |
| `WYRD_PROMPT_500_LOADER_IO` | 500 | Loader IO failure | Inspect the OS error; usually missing path or permission denied. |
| `WYRD_PROMPT_500_MEDIA_IO` | 500 | Media IO failure | Inspect the OS error; usually missing path or permission denied. |
| `WYRD_PROMPT_500_SERIALIZE_REQUEST` | 500 | Serialize native request failed during validation | The native request could not be serialized to JSON. Capture the spec and file an issue. |
| `WYRD_PROMPT_501_UNSUPPORTED_HANDOFF` | 501 | Unsupported message conversion | Use a same-provider handoff or convert manually. |
| `WYRD_PROMPT_502_PROVIDER_UPSTREAM` | 502 | Provider upstream error | Retry with backoff; provider returned a 5xx. |
| `WYRD_PROMPT_504_PROVIDER_TIMEOUT` | 504 | Provider request timed out | Retry; raise the per-request timeout if this is chronic. |

## WYRD_QUERY

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_QUERY_400_INVALID_FIELD` | 400 | Invalid query field or operator | Reference a supported field and use an operator valid for its type. Ordering operators require a numeric or timestamp field. |
| `WYRD_QUERY_400_INVALID_SYNTAX` | 400 | Invalid query syntax | Fix the query string at the reported offset and retry. See the metadata query grammar. |
| `WYRD_QUERY_400_TOO_COMPLEX` | 400 | Query too complex | Reduce the number of predicates or the nesting depth and retry. |

## WYRD_RBAC

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_RBAC_409_BUILTIN_ROLE_IMMUTABLE_NAME` | 409 | Builtin role names are immutable | Builtin role names are durable identifiers used by JWT role resolution and re-seeding. Create a replacement role instead of renaming a builtin role. |

## WYRD_REGISTRY

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_REGISTRY_400_CARD_REF_UID_NOT_RESOLVABLE_HERE` | 400 | `card_ref.uid` was populated for a registry lookup that resolves by (kind, name, version) | Submit the request with `card_ref.uid = None`; the registry resolves by identity tuple. Use `get_card_by_uid` if you have a `card_uid`. |
| `WYRD_REGISTRY_400_CURSOR_MISMATCH` | 400 | Cursor does not match filters | Discard the cursor and restart pagination with the current filters. |
| `WYRD_REGISTRY_400_DEPENDENCY_CYCLE` | 400 | Card dependency cycle | Remove the cyclic CardRef dependency and submit an acyclic card graph. |
| `WYRD_REGISTRY_400_HEAVY_ARTIFACT_NOT_SOLE_SUBMISSION` | 400 | Heavy artifact submission must be standalone | Register the artifact-bearing card alone, then reference it from a later composite request. |
| `WYRD_REGISTRY_400_IDEMPOTENCY_KEY_INVALID` | 400 | Invalid idempotency key | Provide an 8-256 character Idempotency-Key without whitespace. |
| `WYRD_REGISTRY_400_IDEMPOTENCY_KEY_REQUIRED` | 400 | Idempotency key required | Provide a stable Idempotency-Key header and retry the declaration. |
| `WYRD_REGISTRY_400_INVALID_ARTIFACT_PATH` | 400 | Invalid artifact path | Use a relative forward-slash path without empty or parent-traversal segments. |
| `WYRD_REGISTRY_400_INVALID_CARD_SPEC` | 400 | Card spec failed type-driven deserialization | Verify the kind/spec field combination matches the documented Wyrd v1 schema for that kind. |
| `WYRD_REGISTRY_400_INVALID_VERSION_BLOCK` | 400 | The version block is invalid for this operation | Check the error message for the specific constraint: Service and Agent cards require an exact semver pin; version components must fit i64; a scoped bump must stay within the authored range. |
| `WYRD_REGISTRY_400_LIST_LIMIT_OUT_OF_RANGE` | 400 | List `limit` is zero or exceeds the per-tenant cap | Pass a `limit` in `1..=LIST_LIMIT_MAX` (currently 200). |
| `WYRD_REGISTRY_400_MANIFEST_HASH_MISMATCH` | 400 | Artifact manifest hash mismatch | Recompute the JCS manifest hash and retry. |
| `WYRD_REGISTRY_400_REQUIREMENT_NOT_RESOLVABLE_HERE` | 400 | `card_ref.version` is a Requirement; this endpoint accepts a Pin | Resolve the Requirement to a Pin via the future `resolve_card_ref` endpoint, or pass a Pin directly. |
| `WYRD_REGISTRY_400_SPEC_TOO_LARGE` | 400 | Card spec exceeds MAX_SPEC_BYTES (256 KiB) | Reduce the spec size or split into multiple cards. If artifacts are inlined, move them to an Artifact card with object_store backing. |
| `WYRD_REGISTRY_400_UNRESOLVED_INLINE_REFERENCE` | 400 | Unresolved inline CardRef | Resolve nested inline file references in the loader before submitting the card. |
| `WYRD_REGISTRY_400_UNRESOLVED_PATH_REF` | 400 | Unresolved path CardRef | Expand path references in the loader before submitting the card. |
| `WYRD_REGISTRY_400_UPLOAD_INTERRUPTED` | 400 | Upload interrupted | Retry the upload while the declaration operation remains pending. |
| `WYRD_REGISTRY_400_VERSION_REQUIRED` | 400 | `metadata.version` is present but empty | Provide a valid semver string (e.g. `"1.0.0"`) or omit `metadata.version` entirely to let the server auto-assign the next version. |
| `WYRD_REGISTRY_404_CARD_NOT_FOUND` | 404 | No card matches the supplied identity | Check (kind, name, version) or the `card_uid` is correct and the card is declared in the current tenant. |
| `WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT` | 409 | Idempotency key conflict | Use the original request with this key or choose a new key for different content. |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | 409 | Re-declare with the same identity but a different spec_hash; same-version cards are immutable | Bump `metadata.version` to publish a new spec, or revert your spec to match the declared version. |
| `WYRD_REGISTRY_410_OPERATION_EXPIRED` | 410 | Declaration operation expired | Start a new declaration with a new idempotency key. |
| `WYRD_REGISTRY_422_UNRESOLVED_DEPENDENCY` | 422 | Card reference could not be resolved | Register the referenced card in the current tenant before submitting this card. |
| `WYRD_REGISTRY_500_VERSION_CONFLICT` | 500 | Card uid mismatch for same identity | Internal invariant violation; report with the request id and audit_id. |
| `WYRD_REGISTRY_503_REGISTRY_UNAVAILABLE` | 503 | Transient Postgres or RLS misconfiguration; retry with backoff | Retry with exponential backoff (60s cap). If persistent, check Postgres connectivity, the tenant row, and the RLS role bindings. |
| `WYRD_REGISTRY_507_ARTIFACT_VERIFY_FAILED` | 507 | Artifact verification failed | Re-upload the artifact and retry finalization. |

## WYRD_RUNTIME

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_RUNTIME_404_PROVIDER` | 404 | Provider not registered in the runtime | Register a provider client for the requested provider before dispatching the request. |
| `WYRD_RUNTIME_422_RESPONSE_DECODE` | 422 | Provider response did not match the prompt schema | Inspect the provider output against the prompt's declared response schema. |

## WYRD_SDK

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_SDK_400_BIFROST_NOT_STARTED` | 400 | WyrdState Bifrost writer is not started | Call start_bifrost on the state before opening a run and emitting observations. |
| `WYRD_SDK_400_CARD_KIND_MISMATCH` | 400 | WyrdState Card kind mismatch | Use an alias for a Card of the expected kind. |
| `WYRD_SDK_400_INVALID_OBSERVATION` | 400 | Invalid observation input | Emit a flat object of boolean, integer, finite float, or string values, with valid feature names and media bindings. |
| `WYRD_SDK_400_INVALID_STATE_BUNDLE` | 400 | Invalid WyrdState bundle | Pull the intended root Card again with wyrd pull --output-dir &lt;dir&gt;, then load that complete directory. |
| `WYRD_SDK_400_RUNTIME_HYDRATION_FAILED` | 400 | Runtime Card hydration failed | Provide the required custom interface/load arguments or regenerate a complete compatible bundle. |
| `WYRD_SDK_400_UNHYDRATED_ARTIFACT` | 400 | Card bundle is missing artifact payloads | Run wyrd pull for the intended root Card with --output-dir &lt;dir&gt; and without --metadata-only, then load that directory. |
| `WYRD_SDK_404_UNKNOWN_ALIAS` | 404 | Unknown WyrdState alias | Use an alias returned by the loaded WyrdState bundle. |
| `WYRD_SDK_404_UNKNOWN_VERIFIER` | 404 | Verifier is not bound to this subject | Name a Verifier bound in `verified_by` to the view's subject Card, or open the view on the Card that binds it. |
| `WYRD_SDK_409_BIFROST_ALREADY_STARTED` | 409 | WyrdState Bifrost writer is already started | Reuse the state's existing writer, or shut it down and create a new WyrdState to start again. |
| `WYRD_SDK_409_BIFROST_CLOSED` | 409 | WyrdState Bifrost writer is closed | Create a new WyrdState and start its Bifrost writer; a shut-down state stays closed to writes. |
| `WYRD_SDK_409_TELEMETRY_PROVIDER_EXISTS` | 409 | A tracer provider is already installed | Add the Wyrd span exporter to the application's existing provider instead of calling start_telemetry. |
| `WYRD_SDK_422_VERIFICATION_DATA_INVALID` | 422 | Verification data does not match the requested type | Match the requested type to the check's declared data schema, use a union or decoder for mixed shapes, or read the result without a type. |
| `WYRD_SDK_503_TELEMETRY_FLUSH_FAILED` | 503 | Telemetry flush failed | Check the OpenTelemetry exporter's logs and the state's credentials, then retry flush or shutdown. |
| `WYRD_SDK_504_TELEMETRY_FLUSH_TIMEOUT` | 504 | Telemetry flush timed out | Check that the Wyrd server's OTLP endpoint is reachable, raise the OpenTelemetry SDK export timeout if spans are large, and retry flush or shutdown. |
| `WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT` | 504 | Verification execution did not settle in time | Wait again with the same execution ID, or read its status; a submission lost before queue acceptance never settles and must be submitted again. |

## WYRD_SERVER

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_SERVER_503_NOT_READY` | 503 | Server not ready | Wait for readiness probes to recover; check the `details.checks` object for the failing component and reason code. |
| `WYRD_SERVER_503_SERVICE_UNAVAILABLE` | 503 | Service unavailable | Retry with exponential backoff; the server is shedding load to protect inflight requests. |
| `WYRD_SERVER_504_REQUEST_TIMEOUT` | 504 | Request timeout | Reduce the request size or split the operation; the server's per-request timeout was exceeded. |

## WYRD_SESSION

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_SESSION_500_APPEND` | 500 | Session memory append failed | Inspect the session backend and ensure the caller has write permission for the session. |
| `WYRD_SESSION_500_RECENT` | 500 | Session memory recent fetch failed | Inspect the session backend; ensure it is reachable and the session id exists. |

## WYRD_SOURCE

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_SOURCE_400_VALIDATION` | 400 | Source card validation failed | Fix Source card connection coordinates or auth env-var names and retry. |

## WYRD_SPEC

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_SPEC_400_DUPLICATE_BINDING_OPERATOR` | 400 | Duplicate binding Operator | Remove the duplicate on_failure Operator from this binding. |
| `WYRD_SPEC_400_DUPLICATE_SUBMISSION` | 400 | Duplicate card submission | Submit each (kind, name, version) identity at most once per request. |
| `WYRD_SPEC_400_DUPLICATE_VERIFICATION_BINDING` | 400 | Duplicate verification binding | Remove the duplicate verified_by entry; one subject occurrence binds each Verifier version once. |
| `WYRD_SPEC_400_INVALID_BINDING_REF_KIND` | 400 | Invalid verification binding reference kind | Set runs_on to a Trigger CardRef or inline TriggerSpec, and each on_failure entry to an Operator CardRef or inline OperatorSpec. |
| `WYRD_SPEC_400_INVALID_OPERATOR` | 400 | Invalid Operator | Reference only failure-context fields in `{{field}}` templates, keep the URL origin literal https, and do not author server-owned headers. |
| `WYRD_SPEC_400_INVALID_SERVICE_COMPONENT_KIND` | 400 | Invalid Service component kind | Move Verifier, Trigger, Operator, Audit, and Source cards out of Service.components; attach a Verifier with verified_by. |
| `WYRD_SPEC_400_INVALID_VERIFIER_REF_KIND` | 400 | Invalid Verifier reference kind | Set verified_by[..].verifier to a Verifier CardRef. |
| `WYRD_SPEC_400_OPERATOR_CONNECTION_UNAVAILABLE` | 400 | Operator connection unavailable | Create or re-enable an active connection of the Operator's provider with this name in the tenant, whose HTTP origin and auth scheme match the Operator. |
| `WYRD_SPEC_400_TRIGGER_ACTIVATION_MISMATCH` | 400 | Trigger activation does not match Verifier | Attach a schedule Trigger only to a Verifier whose samples slots declare the recorded features it judges; attach a group_end Trigger only to a Verifier with source inputs, reacting through on_verdict_change; supply other evidence through verify or submit. |
| `WYRD_SPEC_400_UNBOUND_VERIFIER_PEER` | 400 | Unbound Verifier peer | Add the Verifier CardRef to the Service, a Service component, or a submitted Agent verified_by list, or declare the shared Verifier separately. |
| `WYRD_SPEC_400_VALIDATION` | 400 | Validation failed | Check the submitted Wyrd request fields against the published schema and retry. |
| `WYRD_SPEC_404_NOT_FOUND` | 404 | Resource not found | Check that the referenced Wyrd resource exists. |
| `WYRD_SPEC_409_CONFLICT` | 409 | Conflict | Refresh the resource and retry with the current version or idempotency key. |
| `WYRD_SPEC_413_PAYLOAD_TOO_LARGE` | 413 | Payload too large | Reduce the request body size and retry. |
| `WYRD_SPEC_500_INTERNAL` | 500 | Internal error | Retry later or inspect server logs using the request ID. |
| `WYRD_SPEC_502_UPSTREAM_FAILURE` | 502 | Upstream dependency failed | Check the upstream dependency health and retry policy. |
| `WYRD_SPEC_504_TIMEOUT` | 504 | Timeout | Retry with a longer timeout or reduce the request scope. |

## WYRD_SQL

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_SQL_403_RLS_DENIED` | 403 | Row-level security policy violation | The current tenant binding does not permit this row. Verify the request's tenant context matches the row's data_tenant_id. |
| `WYRD_SQL_404_NO_ROWS` | 404 | Row not found | Confirm the identifier and re-query. |
| `WYRD_SQL_409_CHECK_VIOLATION` | 409 | Check constraint violated | Inspect the column values and adjust. |
| `WYRD_SQL_409_CONFLICT` | 409 | SQL operation conflict | Reload the affected row and retry only if the row is still in the expected state. |
| `WYRD_SQL_409_FK_VIOLATION` | 409 | Foreign key constraint violated | Confirm the referenced row exists before retrying. |
| `WYRD_SQL_409_TRIGGER_EXCEPTION` | 409 | Trigger exception raised | The operation violated a trigger-enforced invariant; inspect the constraint name for details. |
| `WYRD_SQL_409_UNIQUE_VIOLATION` | 409 | Unique constraint violated | Change the conflicting identifier and retry. |
| `WYRD_SQL_500_CONNECT` | 500 | Database connection failed | Check WYRD_DATABASE_URL and verify the database is reachable. |
| `WYRD_SQL_500_INSUFFICIENT_PRIVILEGE` | 500 | Insufficient database privileges | Run `wyrd-server migrate` as the platform login (WYRD_PLATFORM_DATABASE_URL, else WYRD_DATABASE_URL), which must own the database or hold CREATE on it; serving processes never run DDL. |
| `WYRD_SQL_500_INVALID_TENANT_ID` | 500 | Stored tenant identifier is invalid | Inspect platform.tenants and repair rows whose data_tenant_id does not satisfy Wyrd's UUIDv7 contract. |
| `WYRD_SQL_500_INVARIANT_VIOLATION` | 500 | SQL invariant violation | Inspect the stored row and repair data that no longer matches Wyrd's schema invariants. |
| `WYRD_SQL_500_MIGRATE` | 500 | Database migration failed | Inspect the failing migration SQL and check server logs. |
| `WYRD_SQL_500_MIGRATE_CHECKSUM` | 500 | Migration checksum drift | A previously-applied migration was modified. Restore the original or write a new migration. |
| `WYRD_SQL_500_QUERY` | 500 | Database query failed | Check server logs for the SQL error. |
| `WYRD_SQL_500_TX_FAILED` | 500 | Transaction failed | Check server logs; the transaction has been rolled back. |
| `WYRD_SQL_503_SCHEMA_NOT_READY` | 503 | Database schema is not ready for serving | Run `wyrd-server migrate` as the platform login. Serve with ordinary logins that are neither superuser nor BYPASSRLS: WYRD_DATABASE_URL alone, or a WYRD_PLATFORM_DATABASE_URL login that owns Wyrd's objects plus a WYRD_DATABASE_URL login that does not. |

## WYRD_STORAGE

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_STORAGE_400_ARTIFACT_TOO_LARGE` | 400 | Artifact size exceeds backend capacity | Split the artifact into multiple cards or use a backend with higher per-object limits. |
| `WYRD_STORAGE_400_INVALID_UPLOAD_ID` | 400 | Upload id not recognised | Use the upload_id returned by POST /v1/cards/upload/init. |
| `WYRD_STORAGE_400_INVALID_URI` | 400 | Source URI failed to parse | Pass an absolute URI with a scheme and authority recognised by the configured backend. |
| `WYRD_STORAGE_400_SHA256_INVALID` | 400 | Invalid SHA-256 in upload init | Recompute SHA-256 of the artifact and base64-encode the 32-byte digest. |
| `WYRD_STORAGE_400_SHA256_MISMATCH` | 400 | Stored object SHA-256 does not match expected_sha256 | Recompute the SHA-256 of the artifact bytes and retry. |
| `WYRD_STORAGE_400_SIZE_INVALID` | 400 | Invalid expected_size_bytes | Pass a u64 byte length that matches the actual file size. |
| `WYRD_STORAGE_400_SIZE_MISMATCH` | 400 | Stored object byte length does not match expected_size_bytes | Verify the file size you reported in upload_init matches the bytes you uploaded. |
| `WYRD_STORAGE_400_TENANT_PATH_MISMATCH` | 400 | Tenant path validation rejected upload init | Ensure relative_path contains no `..`, no leading `/`, and only [A-Za-z0-9._-] characters. |
| `WYRD_STORAGE_400_TENANT_PREFIX_INVALID` | 400 | Source URI tenant prefix is not a UUIDv7 | Object URIs must start with the caller's data_tenant_id. |
| `WYRD_STORAGE_403_UPLOAD_FOREIGN_TENANT` | 403 | Upload belongs to a different tenant | Authenticate with a token whose data_tenant_id matches the upload's tenant prefix. |
| `WYRD_STORAGE_404_OBJECT_NOT_FOUND` | 404 | Object not found in backend | Verify card_uid and relative_path; re-upload if the object was deleted. |
| `WYRD_STORAGE_404_UPLOAD_NOT_FOUND` | 404 | Upload id not found | Use the upload_id returned by POST /v1/cards/upload/init. |
| `WYRD_STORAGE_409_ENCRYPTION_MISSING` | 409 | Server-side encryption verification failed | Configure backend server-side encryption and retry. |
| `WYRD_STORAGE_409_UPLOAD_NOT_PENDING` | 409 | Cannot complete or abort a terminal upload | Inspect upload status; only pending uploads can be completed or aborted. |
| `WYRD_STORAGE_412_PRECONDITION` | 412 | Object changed during download | Delete the partial file and re-issue download init for a fresh URL. |
| `WYRD_STORAGE_416_RANGE_NOT_SATISFIABLE` | 416 | Source changed mid-download | Delete the partial file and sidecar, then re-download. |
| `WYRD_STORAGE_500_BACKEND` | 500 | Backend storage operation failed | Retry the operation. If failures persist, check backend dashboard and storage server logs. |
| `WYRD_STORAGE_500_CONFIG_INVALID` | 500 | Storage settings invalid | Inspect server startup logs; fix the named env var and restart. |
| `WYRD_STORAGE_500_CREDENTIAL_CHAIN` | 500 | Backend credential chain failed | Verify backend credentials are reachable in the server environment. |
| `WYRD_STORAGE_500_LIFECYCLE_MISSING` | 500 | S3 lifecycle missing required rule | Apply the recommended lifecycle policy for incomplete multipart uploads. |
| `WYRD_STORAGE_503_BACKEND_UNAVAILABLE` | 503 | Backend transiently unavailable | Retry with exponential backoff. |
| `WYRD_STORAGE_503_PRESIGN_EXPIRED` | 503 | Presigned URL expired | Call upload part-url or download init to obtain a fresh URL and retry. |

## WYRD_TOOL

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_TOOL_400_INVALID_SCHEMA` | 400 | Tool declaration malformed | Inspect the tool declaration name, description, and JSON input schema. |
| `WYRD_TOOL_404_NOT_REGISTERED` | 404 | Tool name not registered | Register the tool before resolving it, or check the name against the registry's available tools. |
| `WYRD_TOOL_409_NAME_TAKEN` | 409 | Tool name already registered | Pick a unique tool name or clear the registry before registering the replacement. |
| `WYRD_TOOL_422_INPUT` | 422 | Tool invocation input did not match the declared input schema | Adjust the tool call arguments to match the schema the tool declares. |
| `WYRD_TOOL_500_CALL` | 500 | Tool invocation failed during execution | Check the tool's underlying cause and fix the tool implementation or its inputs. |
| `WYRD_TOOL_500_OUTPUT` | 500 | Tool output could not be serialized to JSON | Ensure the tool output type serializes to a JSON-compatible value. |

## WYRD_TS

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_TS_501_HEAVY_UPLOAD_DEFERRED` | 501 | TypeScript heavy upload is deferred | Use the CLI or Python SDK for heavy artifact uploads. |

## WYRD_VALA

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_VALA_400_BIFROST_INVALID_COMPACTION_TARGET` | 400 | Invalid Bifrost compaction target | Omit compaction_target_file_size_bytes to follow the deployment default, or supply at least 134217728 bytes. |
| `WYRD_VALA_400_BIFROST_INVALID_PHYSICAL_LAYOUT` | 400 | Invalid Bifrost physical layout | Declare hour or day granularity, an optional non-null timestamp time column, at most four unique schema columns as sort keys, and supported sort/null values. |
| `WYRD_VALA_400_BIFROST_RESERVED_COLUMN` | 400 | Reserved system column name | Rename the column; run_id, card_uid, principal_id, wyrd_request_id, and wyrd_ingested_at are reserved. |
| `WYRD_VALA_400_BIFROST_TENANT_ISOLATION_COLUMN_MISSING` | 400 | Tenant isolation column missing | Add the server-managed data_tenant_id Utf8 column to the physical table schema. |
| `WYRD_VALA_400_BIFROST_UNDECLARED_FIELD` | 400 | Undeclared Bifrost field | Remove the key or declare it on the table; store open data in a Variant field. |
| `WYRD_VALA_400_BIFROST_UNSUPPORTED_TYPE` | 400 | Unsupported Bifrost type | Use a supported Bifrost column type; store open data in a Variant field. |
| `WYRD_VALA_400_FILTER_INVALID` | 400 | Query filter invalid | Correct the filter value; see the error detail for the offending field. |
| `WYRD_VALA_400_INGEST_PROTO` | 400 | Invalid ingest request | Fix the ingest request fields and retry with a valid Wyrd ingest payload. |
| `WYRD_VALA_400_OTLP_REQUEST_MALFORMED` | 400 | OTLP request body malformed | The OTLP request body could not be decoded. Verify the Content-Type matches the encoding (application/x-protobuf or application/json) and that the payload is a valid OTLP ExportRequest. |
| `WYRD_VALA_400_PAGE_TOKEN_INVALID` | 400 | Page token invalid | Restart the query from page 1 without a page_token. |
| `WYRD_VALA_400_QUERY_INVALID_SQL` | 400 | Invalid or unsupported query SQL | Submit a single SELECT statement; DDL/DML and unsupported constructs are rejected. |
| `WYRD_VALA_400_SCHEMA_PARSE` | 400 | Bifrost schema parse failed | Correct the column type or the row value so it satisfies the table's declared DataTypeSpec; declare a Variant field for open data. |
| `WYRD_VALA_400_VARIANT_INVALID_JSON` | 400 | Invalid Variant JSON | Supply valid JSON text or a valid arrow.parquet.variant value for the Variant field; use try_parse_json in SQL to map invalid text to null. |
| `WYRD_VALA_400_VARIANT_NUMERIC_OUT_OF_RANGE` | 400 | Variant numeric value out of range | Send integers within the signed 64-bit or Variant decimal range, or send the number as a string. |
| `WYRD_VALA_400_VARIANT_TOO_DEEP` | 400 | Variant value too deep | Flatten the value so it nests at most 64 levels, counting the root container as one. |
| `WYRD_VALA_400_WINDOW_REQUIRED` | 400 | Query time window required | Supply `since` and/or `until` on the query request. |
| `WYRD_VALA_401_INGEST_AUTH` | 401 | Ingest authentication failed | Provide a valid Wyrd access token with permission to write the target ingest surface. |
| `WYRD_VALA_401_PRINCIPAL_UNRESOLVED` | 401 | Ingest principal unresolved | Authenticate with a token containing a valid principal identity and retry. |
| `WYRD_VALA_403_BIFROST_CARD_SCOPE` | 403 | Card outside principal card scope | Attribute the write or call to a Card the authenticated principal is authorized to tag, or add the target Card to the Service card's components. |
| `WYRD_VALA_403_BIFROST_RESERVED_BUILTIN_WRITE` | 403 | Reserved built-in write denied | Write through the supported observation or audit API instead of directly targeting a reserved built-in table. |
| `WYRD_VALA_403_BIFROST_TENANT_BINDING_MISSING` | 403 | No tenant binding for OLAP operation | Acquire a TenantConn for a valid tenant before writing or querying Bifrost tables. |
| `WYRD_VALA_403_CARD_UNRESOLVED` | 403 | Card reference unresolved | Use a card_ref that resolves to a declared card in the current tenant. |
| `WYRD_VALA_403_QUERY_FORBIDDEN` | 403 | Query forbidden | The principal lacks bifrost_query:read for this table. |
| `WYRD_VALA_403_QUERY_PEER_SECURITY` | 403 | Query peer security validation failed | Reject the peer attempt and investigate credential/fence/replay state. |
| `WYRD_VALA_404_BIFROST_TABLE_NOT_FOUND` | 404 | Bifrost table not found | Create the table via the catalog API before writing or querying. |
| `WYRD_VALA_404_RUNNING_QUERY_NOT_FOUND` | 404 | Running query not found | Verify the request identity or list currently running queries for the authenticated tenant. |
| `WYRD_VALA_404_TRACE_NOT_FOUND` | 404 | Trace not found | Confirm the trace_id and that it falls within the queried time window. |
| `WYRD_VALA_409_BIFROST_COMMIT_CONFLICT` | 409 | Concurrent OLAP commit conflict | Retry the write; another writer committed to the same table concurrently. |
| `WYRD_VALA_409_BIFROST_COMPACTION_TARGET_MISMATCH` | 409 | Bifrost compaction target mismatch | Retry with the table's registered compaction target, omit it, or register a different table. |
| `WYRD_VALA_409_BIFROST_COMPACTION_TYPE_MISMATCH` | 409 | Bifrost compaction type mismatch | Retry with the table's registered compaction type, omit it, or register a different table. |
| `WYRD_VALA_409_BIFROST_DUPLICATE_FAILED_BATCH` | 409 | Duplicate batch previously failed | This batch_id was committed but the write failed. Use a new batch_id to retry. |
| `WYRD_VALA_409_BIFROST_FINGERPRINT_MISMATCH` | 409 | Schema fingerprint mismatch | The Arrow schema does not match the registered table schema. Evolve the schema explicitly. |
| `WYRD_VALA_409_BIFROST_LAYOUT_MISMATCH` | 409 | Bifrost physical layout mismatch | Retry with the table's registered physical layout or register a different table. |
| `WYRD_VALA_409_RUNNING_QUERY_CONFLICT` | 409 | Running query ownership conflict | Do not retry cancellation automatically; investigate the contradictory Oracle owners. |
| `WYRD_VALA_409_STREAM_MISMATCH` | 409 | Live-tail stream identity mismatch | Refresh the Scribe stream identity from the cluster catalog and retry the tail-fetch against the current writer stream. |
| `WYRD_VALA_410_PAGE_SNAPSHOT_EXPIRED` | 410 | Page snapshot expired | restart the query from page 1 |
| `WYRD_VALA_412_NO_ACTIVE_TABLE` | 412 | No active Bifrost table | Bind a table with use_table or use_table_by_name before inserting rows. |
| `WYRD_VALA_413_PAYLOAD_TOO_LARGE` | 413 | Ingest payload too large | Reduce the request below the enforced limit reported in this problem and retry. |
| `WYRD_VALA_413_QUERY_RESULT_TOO_LARGE` | 413 | Query result too large | Add a LIMIT or narrower filters for large result sets. |
| `WYRD_VALA_413_VARIANT_TOO_LARGE` | 413 | Variant value too large | Keep each Variant value at most 8388608 encoded bytes (metadata plus value); split large payloads across rows. |
| `WYRD_VALA_422_QUERY_MEMORY_REQUEST_TOO_LARGE` | 422 | Query memory request too large | Reduce the requested range or query memory footprint; retrying unchanged will not succeed. |
| `WYRD_VALA_429_INGEST_BUSY` | 429 | Ingest writer busy | The per-table ingest buffer is full. Back off and retry the full request; no rows were written. |
| `WYRD_VALA_429_QUERY_ADMISSION_REJECTED` | 429 | Query admission rejected | Retry after the supplied bounded delay or reduce demand. |
| `WYRD_VALA_429_QUERY_QUEUE_FULL` | 429 | Query queue full | Retry after capacity becomes available. |
| `WYRD_VALA_500_BIFROST_INTERNAL` | 500 | Internal Bifrost failure | Check server logs for details and contact support if the issue persists. |
| `WYRD_VALA_500_BIFROST_METADATA_MISMATCH` | 500 | Bifrost table metadata mismatch | Investigate table metadata integrity; possible mid-flight schema-evolution race. |
| `WYRD_VALA_500_ICEBERG_MISSING` | 500 | Domain table Iceberg table missing | The Iceberg table for a pre-declared domain table is missing. Run the restore runbook. |
| `WYRD_VALA_500_PHYSICAL_DRIFT` | 500 | Domain table physical schema drift | The Iceberg table physical schema drifted from the declared schema. Run the repair runbook. |
| `WYRD_VALA_500_QUERY_EXECUTION_FAILED` | 500 | Query execution failed | Inspect the scrubbed terminal error and server diagnostics before retrying. |
| `WYRD_VALA_500_QUERY_RECONCILIATION_INVARIANT` | 500 | Query reconciliation invariant violated | Stop and investigate cross-tier row corruption. |
| `WYRD_VALA_500_QUERY_TENANT_INVARIANT` | 500 | Query tenant invariant violated | Stop and investigate tenant isolation; never retry as caller input. |
| `WYRD_VALA_500_REDACTION_FAILED` | 500 | Write-path redaction failed | The redaction classifier failed on this batch. Retry; if it persists, check the classifier installation. |
| `WYRD_VALA_500_SCHEMA_DRIFT` | 500 | Domain table schema fingerprint drift | A domain table schema changed without a coordinated migration. Restore the previous schema or run the migration runbook. |
| `WYRD_VALA_502_QUERY_STREAM_INCOMPLETE` | 502 | Query stream incomplete | Discard partial output; retry because no terminal was observed. |
| `WYRD_VALA_502_QUERY_STREAM_PROTOCOL` | 502 | Query stream protocol violation | Discard partial output and retry through a healthy server. |
| `WYRD_VALA_503_BIFROST_CATALOG_UNREACHABLE` | 503 | OLAP catalog unreachable | Check the catalog database connection and retry. |
| `WYRD_VALA_503_BIFROST_STORAGE_UNREACHABLE` | 503 | OLAP object storage unreachable | Check the object storage configuration and credentials, then retry. |
| `WYRD_VALA_503_BIFROST_WRITER_UNAVAILABLE` | 503 | Bifrost writer unavailable | The writer actor for this table is not running. Restart the write operation. |
| `WYRD_VALA_503_ORACLE_ROLE_UNAVAILABLE` | 503 | Oracle role unavailable | Retry through an Oracle-ready query endpoint. |
| `WYRD_VALA_503_QUERY_RESOURCES_EXHAUSTED` | 503 | Query resources exhausted | Narrow the query or add capacity; retrying unchanged will not succeed. |
| `WYRD_VALA_503_QUERY_VISIBILITY_UNAVAILABLE` | 503 | Query visibility unavailable | Retry when the published query source is available. |
| `WYRD_VALA_503_RUNNING_QUERY_CONTROL_UNAVAILABLE` | 503 | Running query control unavailable | Restore every current-ready Oracle lifecycle endpoint before retrying the logical operation. |
| `WYRD_VALA_503_SCRIBE_ROLE_UNAVAILABLE` | 503 | Scribe role unavailable | Retry ingest through a WAL-ready Scribe endpoint. |
| `WYRD_VALA_504_QUERY_TIMEOUT` | 504 | Query execution timed out | Narrow the query by adding filters or reducing scanned partitions. |
| `WYRD_VALA_507_WAL_DISK_FULL` | 507 | WAL disk full | The Scribe WAL disk is at capacity. Expand storage or wait for seal to free space, then retry. |

## WYRD_VALIDATION

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_VALIDATION_400_MISSING_REQUIRED_FIELD` | 400 | Required request field is missing | Include the named field in the request; see the route's OpenAPI schema for the full required set. |

## WYRD_VERIFICATION

| Code | Status | Title | Fix |
| --- | ---: | --- | --- |
| `WYRD_VERIFICATION_400_INPUT_INVALID` | 400 | Invalid verification input | Send `{ verifier_card_uid, card_uid, inputs }` where inputs maps each declared Verifier slot to a json, file, artifact, or trace value; run_id and execution_route_overrides are optional. |
| `WYRD_VERIFICATION_400_INVALID_TARGET` | 400 | Invalid verification run target | Target an existing binding, or an active Verifier Card UID plus an active subject Card UID in this tenant, whose implementation accepts the requested input. |
| `WYRD_VERIFICATION_400_INVALID_WINDOW` | 400 | Invalid Drift window | Send a UTC half-open drift_window with start before end spanning at most 31 days. |
| `WYRD_VERIFICATION_404_BINDING_NOT_FOUND` | 404 | Verification binding not found | Read the owner Card's card.status.verification.binding_ids and retry with one of those binding IDs. |
| `WYRD_VERIFICATION_404_EXECUTION_NOT_FOUND` | 404 | Verification execution not found | Use the execution_id returned by POST /v1/verification/enqueue in the same tenant; a just-accepted execution may take a moment to become visible. |
| `WYRD_VERIFICATION_404_TARGET_NOT_FOUND` | 404 | Verification target not found | Use an active Verifier Card UID and an active subject Card UID declared in the caller's tenant. |
| `WYRD_VERIFICATION_409_BASELINE_LEGACY` | 409 | Verification baseline legacy | Register a new Verifier version so its baseline is refitted under the current format. |
| `WYRD_VERIFICATION_409_BASELINE_NOT_READY` | 409 | Verification baseline not ready | Poll the Verifier Card's card.status.verification.baseline until it is ready, then retry. |
| `WYRD_VERIFICATION_409_SNAPSHOT_INCOMPLETE` | 409 | Verification snapshot incomplete | Check Bifrost publication for the named source executions, then submit a new turn to the group. |
| `WYRD_VERIFICATION_409_VERIFIER_NOT_READY` | 409 | Verifier not ready | Poll the Verifier Card's card.status.verification.baseline until it is ready, then retry. |
| `WYRD_VERIFICATION_413_INPUT_TOO_LARGE` | 413 | Verification input too large | Keep the request body and each evidence file within the Deployment's verification limits, or upload large files through verification.evidence and pass them by digest. |
| `WYRD_VERIFICATION_422_INPUT_INCOMPATIBLE` | 422 | Verification input incompatible | Supply every feature the Verifier scores, with the value type its baseline or profile expects. |
| `WYRD_VERIFICATION_422_INPUT_UNSUPPORTED` | 422 | Verification input unsupported | Execute and enqueue run request-scoped Verifiers; a Verifier that judges a scheduled window runs only on its binding schedule. |
| `WYRD_VERIFICATION_429_STAGING_FULL` | 429 | Verification staging is full | Nothing of the refused call was staged: retry the identical call after `details.retry_after_seconds`, or lower the request rate. |
| `WYRD_VERIFICATION_502_DEPENDENCY_FAILED` | 502 | Verification dependency failed | The agent check's provider failed after its own retries; retry the request deliberately once the provider recovers. |
| `WYRD_VERIFICATION_504_EXECUTION_TIMED_OUT` | 504 | Verification execution timed out | Reduce the supplied input or the Verifier's judge work so one execution completes within 60 seconds. |

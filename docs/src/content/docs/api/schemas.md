---
title: Schemas
description: Generated inventory of Wyrd JSON Schemas.
---

# Schemas

These schemas are generated from the Wyrd spec crate and checked into the repository for clients, docs, and agents. `architecture/wyrd-design.md` remains the design authority; this inventory may include implementation drift while contracts are being reconciled.

| Schema | Title |
| --- | --- |
| `crates/wyrd-spec/schemas/abort_response.json` | AbortResponse |
| `crates/wyrd-spec/schemas/agent_spec.json` | AgentSpec |
| `crates/wyrd-spec/schemas/artifact_spec.json` | ArtifactSpec |
| `crates/wyrd-spec/schemas/audit_spec.json` | AuditSpec |
| `crates/wyrd-spec/schemas/auth_callback_query.json` | CallbackQuery |
| `crates/wyrd-spec/schemas/auth_issuer_url.json` | IssuerUrl |
| `crates/wyrd-spec/schemas/auth_login_init_response.json` | LoginInitResponse |
| `crates/wyrd-spec/schemas/auth_principal_kind.json` | PrincipalKind |
| `crates/wyrd-spec/schemas/auth_revoke_principal_request.json` | RevokePrincipalRequest |
| `crates/wyrd-spec/schemas/auth_revoke_principal_response.json` | RevokePrincipalResponse |
| `crates/wyrd-spec/schemas/auth_token_request.json` | TokenRequest |
| `crates/wyrd-spec/schemas/auth_token_response.json` | TokenResponse |
| `crates/wyrd-spec/schemas/auth_url.json` | AbsoluteUrl |
| `crates/wyrd-spec/schemas/card.json` | Card |
| `crates/wyrd-spec/schemas/card_kind.json` | CardKind |
| `crates/wyrd-spec/schemas/card_ref.json` | CardRef |
| `crates/wyrd-spec/schemas/data_interface.json` | DataInterface |
| `crates/wyrd-spec/schemas/data_schema.json` | DataSchema |
| `crates/wyrd-spec/schemas/data_spec.json` | DataSpec |
| `crates/wyrd-spec/schemas/data_split.json` | DataSplit |
| `crates/wyrd-spec/schemas/data_stats.json` | DataStats |
| `crates/wyrd-spec/schemas/download_init_request.json` | DownloadInitRequest |
| `crates/wyrd-spec/schemas/download_init_response.json` | DownloadInitResponse |
| `crates/wyrd-spec/schemas/download_plan.json` | DownloadPlan |
| `crates/wyrd-spec/schemas/drift_spec.json` | DriftSpec |
| `crates/wyrd-spec/schemas/eval_spec.json` | EvalSpec |
| `crates/wyrd-spec/schemas/experiment_spec.json` | ExperimentSpec |
| `crates/wyrd-spec/schemas/field_spec.json` | FieldSpec |
| `crates/wyrd-spec/schemas/framework_adapter_ref.json` | FrameworkAdapterRef |
| `crates/wyrd-spec/schemas/hugging_face_task.json` | HuggingFaceTask |
| `crates/wyrd-spec/schemas/invoke_context.json` | InvokeContext |
| `crates/wyrd-spec/schemas/invoke_outcome.json` | InvokeOutcome |
| `crates/wyrd-spec/schemas/local_blob_upload_response.json` | LocalBlobUploadResponse |
| `crates/wyrd-spec/schemas/locked_component.json` | LockedComponent |
| `crates/wyrd-spec/schemas/log_connection.json` | LogConnection |
| `crates/wyrd-spec/schemas/mcp_spec.json` | McpSpec |
| `crates/wyrd-spec/schemas/metrics_connection.json` | MetricsConnection |
| `crates/wyrd-spec/schemas/model_interface.json` | ModelInterface |
| `crates/wyrd-spec/schemas/model_signature.json` | ModelSignature |
| `crates/wyrd-spec/schemas/model_spec.json` | ModelSpec |
| `crates/wyrd-spec/schemas/operator_budget.json` | OperatorBudget |
| `crates/wyrd-spec/schemas/operator_input.json` | OperatorInput |
| `crates/wyrd-spec/schemas/operator_spec.json` | OperatorSpec |
| `crates/wyrd-spec/schemas/parameter_name.json` | String |
| `crates/wyrd-spec/schemas/part_url_response.json` | PartUrlResponse |
| `crates/wyrd-spec/schemas/policy_decision.json` | PolicyDecision |
| `crates/wyrd-spec/schemas/policy_spec.json` | PolicySpec |
| `crates/wyrd-spec/schemas/prompt_ref.json` | PromptRef |
| `crates/wyrd-spec/schemas/prompt_spec.json` | PromptSpec |
| `crates/wyrd-spec/schemas/run_kind.json` | RunKind |
| `crates/wyrd-spec/schemas/run_ref.json` | RunRef |
| `crates/wyrd-spec/schemas/sample_input.json` | SampleInput |
| `crates/wyrd-spec/schemas/sample_input_kind.json` | SampleInputKind |
| `crates/wyrd-spec/schemas/security_secret_ref.json` | SecretRef |
| `crates/wyrd-spec/schemas/security_tls_config.json` | TlsConfig |
| `crates/wyrd-spec/schemas/service_lock.json` | ServiceLock |
| `crates/wyrd-spec/schemas/service_runtime.json` | ServiceRuntime |
| `crates/wyrd-spec/schemas/service_runtime_kind.json` | ServiceRuntimeKind |
| `crates/wyrd-spec/schemas/service_runtime_mode.json` | ServiceRuntimeMode |
| `crates/wyrd-spec/schemas/service_runtime_policy.json` | ServiceRuntimePolicy |
| `crates/wyrd-spec/schemas/service_spec.json` | ServiceSpec |
| `crates/wyrd-spec/schemas/source_auth.json` | SourceAuth |
| `crates/wyrd-spec/schemas/source_kind.json` | SourceKind |
| `crates/wyrd-spec/schemas/source_spec.json` | SourceSpec |
| `crates/wyrd-spec/schemas/split_strategy.json` | SplitStrategy |
| `crates/wyrd-spec/schemas/sql_connection.json` | SqlConnection |
| `crates/wyrd-spec/schemas/sql_logic.json` | SqlLogic |
| `crates/wyrd-spec/schemas/task_type.json` | TaskType |
| `crates/wyrd-spec/schemas/tf_save_format.json` | TfSaveFormat |
| `crates/wyrd-spec/schemas/torch_save_format.json` | TorchSaveFormat |
| `crates/wyrd-spec/schemas/trace_connection.json` | TraceConnection |
| `crates/wyrd-spec/schemas/trigger_source.json` | TriggerSource |
| `crates/wyrd-spec/schemas/trigger_spec.json` | TriggerSpec |
| `crates/wyrd-spec/schemas/upload_complete_request.json` | UploadCompleteRequest |
| `crates/wyrd-spec/schemas/upload_complete_response.json` | UploadCompleteResponse |
| `crates/wyrd-spec/schemas/upload_init_request.json` | UploadInitRequest |
| `crates/wyrd-spec/schemas/upload_init_response.json` | UploadInitResponse |
| `crates/wyrd-spec/schemas/upload_plan.json` | UploadPlan |
| `crates/wyrd-spec/schemas/verification_guarantee.json` | VerificationGuarantee |
| `crates/wyrd-spec/schemas/wire_protocol.json` | WireProtocol |
| `crates/wyrd-spec/schemas/workflow_spec.json` | WorkflowSpec |

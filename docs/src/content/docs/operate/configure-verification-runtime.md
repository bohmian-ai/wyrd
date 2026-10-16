---
title: Configure the verification runtime
description: Approved startup settings for Python workers, verification deadlines, file extraction, and scratch workspaces.
pillar: wyrd
section: Operate
order: 4
draft: true
status: partial
---

This reference records the approved configuration for the verifier building
blocks change. **These environment variables are planned; they are not yet
implemented server settings.** Availability and parity must be checked against
the server configuration when the change ships.

## Startup settings

| Environment variable | Default | Meaning |
| --- | ---: | --- |
| `WYRD_VERIFICATION_MONTY_MIN_WORKERS` | 2 | Prewarmed worker processes. |
| `WYRD_VERIFICATION_MONTY_MAX_WORKERS` | 8 | Maximum live worker processes. |
| `WYRD_VERIFICATION_MONTY_MAX_WAITERS` | 16 | Maximum in-process operations waiting for a worker. |
| `WYRD_VERIFICATION_MONTY_MEMORY_MIB` | 256 | Monty allocation budget per Python operation. |
| `WYRD_VERIFICATION_MONTY_PROCESS_MEMORY_MIB` | 384 | Hard memory ceiling for one worker process, including overhead. |
| `WYRD_VERIFICATION_MONTY_POOL_MEMORY_MIB` | 2048 | Hard combined memory ceiling for all worker processes. |
| `WYRD_VERIFICATION_CODE_TIMEOUT_MS` | 10000 | Complete Python operation, including checkout wait, compilation, execution, and parent-serviced filesystem calls. |
| `WYRD_VERIFICATION_AGENT_CODE_BUDGET_MS` | 60000 | Cumulative Python-operation wall time within one Agent invocation. |
| `WYRD_VERIFICATION_FILE_MAX_MIB` | 20 | Maximum bytes in one input file. |
| `WYRD_VERIFICATION_FILES_MAX_MIB` | 100 | Maximum combined input-file bytes per execution. |
| `WYRD_VERIFICATION_ARCHIVE_MAX_MIB` | 200 | Maximum decompressed bytes per archive. |
| `WYRD_VERIFICATION_ARCHIVE_MAX_ENTRIES` | 10000 | Maximum entries per archive. |
| `WYRD_VERIFICATION_ARCHIVE_MAX_RATIO` | 100 | Maximum decompressed/compressed size ratio. |
| `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` | 500 | Maximum PDF pages or PPTX slides per extraction. |
| `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB` | 10 | Maximum extracted text bytes. |
| `WYRD_VERIFICATION_SCRATCH_MAX_MIB` | 250 | Maximum scratch bytes per invocation. |
| `WYRD_VERIFICATION_WORK_WRITE_MAX_MIB` | 50 | Cumulative writable /work budget per invocation. |
| `WYRD_VERIFICATION_TOOL_RESULT_MAX_KIB` | 64 | Maximum tool-result bytes, bounded while collecting output. |
| `WYRD_VERIFICATION_MAX_WORKSPACES` | 16 | Maximum concurrent scratch workspaces per server process. |
| `WYRD_VERIFICATION_TIMEOUT_MS` | 60000 | Overall immediate verify deadline. |

## Capacity, deadlines, and isolation

Read these variables once during server startup. Unset variables use the
defaults; changing a value requires a server restart. Values are positive
integers; `_MIB` and `_KIB` use binary units and `_MS` uses milliseconds.
Refuse startup for invalid/overflowing values or inconsistent settings:
minimum workers above maximum, allocation budget at or above the process
ceiling, pool ceiling below the process ceiling, file limit above combined
file limit, or writable-work budget above scratch limit. Report the offending
variable clearly; never silently disable a limit. Slot limits may lower, never
raise, the configured deployment limits. Agent/step/request deadlines still
apply; the earliest deadline wins.

One Monty pool belongs to each verification-runtime server process, shared
across tenants without sharing interpreter state. Only `code` actions/checks
and explicitly enabled Agent-action `code.run` calls acquire workers. Agent
checks, model waits, and non-Python tools do not reserve a worker. Each Python
operation uses a fresh interpreter session; workspace files may persist only
within the owning invocation. Discard a session after any resource-limit
breach; kill/replace crashed, hung, or compromised workers.

Worker checkout waits within the operation's remaining deadline, also bounded
by the enclosing caller deadline. There is no independent one-second checkout
timeout. Bound waiting operations with `MONTY_MAX_WAITERS`; when full, refuse
additional in-process admission through the existing capacity/error path.
Background executions remain in the existing queue until runtime capacity is
available rather than occupying an unbounded in-memory waiting list. Preserve
leases/fencing and cancellation while coordinating capacity; add no second
queue or scheduler. Python waiting/execution budgets do not reset across tool
calls or retries within the enclosing invocation.

The default 2 GiB worker-group ceiling remains in force with up to eight
workers. It does not promise eight workers can each consume their full
individual allocation budget simultaneously. Enforce aggregate and process
memory independently of Monty's allocator accounting; count idle workers too.
Server, extraction, model-response, and parent-side filesystem/output buffers
are outside the worker-group ceiling and need separate deployment headroom.
Worker termination under memory pressure must fail the affected operation
safely, never take down the parent server or fabricate a passing verdict.

Security remains mandatory: a separate unprivileged worker identity, empty
credential environment, OS-enforced network denial, per-invocation read-only
`/inputs` and bounded writable `/work`, parent-serviced capability-scoped file
access, no arbitrary host callbacks/subprocesses/host Python packages, and no
cross-invocation interpreter state. Environment settings cannot disable these
protections. The implementation must prove the selected runtime and host enforcement can
provide them; unsupported isolation is not silently accepted.

For database and server setup, see [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/).

## Built-in Agent tools

The approved hosted Agent harness resolves `AgentSpec.tool_names` automatically
at runtime. Authors do not upload callables, register SDK-local functions, or
configure an MCP connection to use these built-ins:

| Tools | Capability |
| --- | --- |
| `cards.list`, `cards.get`, `cards.get_by_ref` | Discover Cards and read their exact versions. |
| `bifrost.list_tables`, `bifrost.describe_table`, `bifrost.query` | Discover schemas and run bounded read-only queries. |
| `fs.list`, `fs.read`, `fs.search` | Read the invocation's declared workspace. |
| `code.run` | Run Python against that workspace using the shared Monty pool. |

Only explicitly declared tools attach to a preparation Agent. Unknown names
and missing required workspaces fail explicitly. Agent checks have no tools.
Built-in tool names can be saved on registered Agents; arbitrary client-local
callables cannot be uploaded as server tools.

Tool calls use existing authorized Rust operations directly. A tool's global
availability grants no resource permission: each protected invocation uses the
verification's accepted original subject or valid delegated permissions from
that subject. Delegation cannot widen its accepted authority. Background work
preserves those permissions through retry/restart; SYSTEM execution changes
attribution, not permission. Tool arguments cannot override identity. Caller
context and scratch stay isolated between invocations; credentials never enter
model prompts or Python workers. Permission checks and audit remain in the
existing capability owners.

This hosted tool wiring is planned with the verifier change and has not yet
been implemented.

### Tool invocation and resource permissions

Wyrd uses the same rule for every resource: **global or exact scope
permission**. Existing scope coverage applies, including a Bifrost schema
grant covering its tables. Tools use this permission model too.

Every call needs invoke permission on its tool resource, including `code.run`
and `fs.search`. For example, grant global invocation permission:

```json
{"resource":"tool","action":"invoke","scope":"all"}
```

Or permission scoped to one built-in:

```json
{"resource":"tool","action":"invoke","scope":{"tool":"bifrost.query"}}
```

This adds a resource scope to the existing permission model, not a separate
authorization rule. The tool
scope accepts an exact built-in name. A declaration in `tool_names` grants no
permission. After the tool gate allows execution, the underlying operation
still checks resource access. For example:

- Tool grant plus read access to the queried tables: the query can proceed.
- Bifrost read access without a tool grant: the tool call is denied.
- Tool grant without Bifrost read access: the query is denied.
- Global tool access plus read access to one table: other tables remain denied,
  including when a query joins an allowed table to a forbidden one.

Both checks use the accepted original subject or its valid delegated
permissions. They retain existing tenant isolation, scoped grant intersection,
and permission audit. A tool denial prevents execution; a resource denial
returns no partial data. Code and filesystem grants do not expand their
sandbox mounts or resource limits. These grants and checks are planned with
the hosted tool implementation, not available server configuration today.

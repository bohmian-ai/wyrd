# Wyrd

**Open-source verification and assurance infrastructure for AI systems.**

Wyrd verifies that models, prompts, agents, workflows, and AI services behave
as intended from development through production.

Teams define expected behavior with typed, versioned contracts. Wyrd observes
real execution, evaluates it with reusable Verifiers, and preserves the
resulting evidence through lineage, policy, and audit. It works alongside your
applications and infrastructure; it does not replace them.

[Getting started](docs/src/content/docs/get-started/index.svx) ·
[Architecture](architecture/wyrd-design.md) ·
[Release tracker](https://github.com/orgs/bohmian-ai/projects/1) ·
[Contributing](CONTRIBUTING.md)

## Why Wyrd exists

An AI service can remain available and return valid responses while its quality,
safety, or intended behavior quietly degrades. Its behavior depends on changing
data, model versions, prompts, tools, providers, and runtime context, so testing
the application once is not enough.

Teams often respond with separate evaluation scripts, telemetry pipelines,
drift monitors, registries, policy checks, and audit systems. Each represents
the system differently, making verification inconsistent, difficult to
reproduce, and expensive to maintain.

Wyrd makes verification a shared infrastructure capability instead of something
every team rebuilds.

## How Wyrd verifies AI systems

```text
Declare the system and its expected behavior
        ↓
Observe real executions
        ↓
Evaluate behavior with Verifiers
        ↓
Record evidence and lineage
        ↓
Enforce policy or trigger action
```

- **Cards** identify the exact data, models, prompts, agents, workflows, and
  services being verified.
- **Verifiers** define reusable Drift and Agent Eval checks.
- **Runs and Observations** record what happened during execution.
- **Lineage** connects verification evidence to exact component versions.
- **Bifrost** stores and queries the evidence produced by verification.
- **Policy and Audit** govern consequential decisions and record
  accountability.
- **Operators** respond when verification fails.

## How the platform fits together

- **Wyrd** is the control plane for Cards, registry, lineage, identity, policy,
  and audit.
- **Vala** owns observations, Drift and Agent Eval execution, and the Bifrost
  warehouse.
- **Skald** owns LLM providers, prompts, tools, agents, and workflows.

## Try the current source

Public packages and container images will be published with `v0.1.0`. Until
then, work from a repository checkout. You need Git, Python 3.10 or newer, and
[`mise`](https://mise.jdx.dev/).

```bash
git clone https://github.com/bohmian-ai/wyrd.git
cd wyrd
mise install
mise run examples:python:datacard
```

Expected output:

```text
saved_card_json: true
saved_data_file: data/data.parquet
loaded_rows: 3
loaded_columns: customer_id,churned,segment
labels: domain=customer,stage=example
annotation_source: examples/python/datacard_local_workflow.py
```

This credential-free example creates a `DataCard` from a pandas DataFrame,
saves it locally, and loads it again. It demonstrates Card authoring; it does
not start the server or run continuous verification.

To run the current server locally:

```bash
mkdir -p .wyrd-dev/storage
export WYRD_STORAGE_URL="file://$PWD/.wyrd-dev/storage"
export WYRD_PUBLIC_BASE_URL="http://localhost:8080"
mise run dev:backend
```

The development server uses embedded Postgres, local object storage, and an
ephemeral signing key. See
[Self-hosting](docs/src/content/docs/self-hosting/index.svx) for health checks,
initialization, and production configuration.

## Release status

> **Pre-release:** Wyrd `v0.1.0` is targeted for September 25, 2026. It will
> be the first public release. Wyrd will remain pre-1.0, so APIs and persisted
> schemas may change between minor versions; `v0.1.0` will not include upgrade
> migrations.

Today, a source checkout supports local Card authoring and a development server.
Public packages, container images, and the complete end-to-end verification
path will ship with `v0.1.0`.

`v0.1.0` is a self-hosted, headless release. Its release gates cover the
capabilities described above as one end-to-end system. The release also
includes platform and tenant administration, published Rust, Python, and
TypeScript SDKs, and `bohmianai/wyrd` Docker images for Linux `amd64` and
`arm64`.

Python and TypeScript packages will support Linux and macOS on `x86_64` and
`arm64`. Windows is not part of `v0.1.0`. Work that misses a release gate
moves to `v0.1.1`.

Track progress in the
[`v0.1.0` milestone](https://github.com/bohmian-ai/wyrd/milestone/1) and
[release project](https://github.com/orgs/bohmian-ai/projects/1).

`v0.2.0` adds the production UI, Wyrd-operated SaaS, tenant OIDC
federation, and stronger compatibility and migration guarantees. It will still
be pre-1.0.

## Documentation

- [Get started](docs/src/content/docs/get-started/index.svx)
- [Wyrd architecture](architecture/wyrd-design.md)
- [Bifrost architecture](architecture/bifrost-design.md)
- [Self-hosting](docs/src/content/docs/self-hosting/index.svx)
- [Security policy](SECURITY.md)

## Contributing

Issues and pull requests are welcome. Read [CONTRIBUTING.md](CONTRIBUTING.md)
before starting a substantial change so maintainers can coordinate overlapping
work.

Report vulnerabilities privately through [SECURITY.md](SECURITY.md), not
through a public issue.

## License

Wyrd is licensed under the [Apache License 2.0](LICENSE.md).

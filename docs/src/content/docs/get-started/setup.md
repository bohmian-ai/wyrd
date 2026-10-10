---
title: Set up Wyrd
description: Run a Wyrd server on your machine, create its first credential, and make an authenticated request.
pillar: wyrd
section: Get started
order: 2
draft: true
status: partial
---

This guide runs one Wyrd server on your machine against a local PostgreSQL database and a local storage directory. At the end you have an admin credential and a client that can call the server. Everything you create survives a restart.

## Prerequisites

- macOS, or Linux with glibc, on x86_64 or arm64.
- Docker, for PostgreSQL 16.
- For the Rust SDK, or until the first Wyrd release, a checkout of the Wyrd repository with [`mise`](https://mise.jdx.dev/) installed. It pins the Rust toolchain.

## 1. Install Wyrd

Install the client for your language. The Python and TypeScript packages include the `wyrd` CLI.

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

```bash
pip install wyrd
```

</LangTab>
<LangTab lang="rust">

```bash
mise exec -- cargo add wyrd-sdk-rust
mise exec -- cargo build -p wyrd-cli   # the CLI, at ./target/debug/wyrd
```

</LangTab>
<LangTab lang="typescript">

```bash
npm install @wyrd/sdk
```

</LangTab>
</LangTabs>

Then install the server:

```bash
wyrd server install
```

`wyrd server install` downloads the newest stable release for your machine, checks its signature and SHA-256, and prints `wyrd-server <version> installed at <directory>`. The server is then at `~/.config/wyrd/server/current/wyrd-server`, or under `WYRD_CONFIG_HOME` or `XDG_CONFIG_HOME` when you set one. Run the command again to update. If an update fails, the server you already have keeps working.

No Wyrd release is published yet, so for now the command stops with `WYRD_CLI_503_SERVER_RELEASE_UNAVAILABLE`. Until the first release, build the server from the checkout and use `./target/debug/wyrd-server` wherever steps 3 and 4 name the installed server:

```bash
mise exec -- cargo build -p wyrd-server
```

## 2. Prepare PostgreSQL

The server runs as an ordinary PostgreSQL login, never a superuser: PostgreSQL exempts superusers from the row-level security that keeps tenants apart. The container's `postgres` superuser only creates that login. A team deployment uses two logins instead; see [Deploy Wyrd for a team](/operate/deploy-wyrd-for-a-team/).

Start PostgreSQL, then create a login and a database it owns:

```bash
docker run -d --name wyrd-dev-pg -p 127.0.0.1:5432:5432 \
  -e POSTGRES_PASSWORD=admin-dev-pw -v wyrd-dev-pg:/var/lib/postgresql/data postgres:16
until docker exec wyrd-dev-pg pg_isready -U postgres >/dev/null 2>&1; do sleep 1; done
docker exec wyrd-dev-pg psql -U postgres \
  -c "CREATE ROLE wyrd LOGIN PASSWORD 'wyrd-dev-pw'" -c "CREATE DATABASE wyrd OWNER wyrd"
```

Set the server's environment:

```bash
export WYRD_DATABASE_URL="postgresql://wyrd:wyrd-dev-pw@127.0.0.1:5432/wyrd"
export WYRD_BIFROST_MEMORY_LIMIT_BYTES=$(sysctl -n hw.memsize)   # macOS only
```

The server keeps its local data in `.wyrd/` under the directory you start it from, and creates it on first start. On Linux it detects memory itself; on macOS it refuses to start without the limit.

## 3. Start the server

Migrate the database, then start the server:

```bash
~/.config/wyrd/server/current/wyrd-server migrate
~/.config/wyrd/server/current/wyrd-server
```

`migrate` prints `Wyrd and Vala schemas are migrated and valid.` Run it again after you update Wyrd; it is safe to rerun.

The server listens for HTTP on port 8080 and gRPC on port 50051. Check it from a second terminal:

```bash
curl -s http://127.0.0.1:8080/healthz   # ok
curl -s http://127.0.0.1:8080/readyz    # 200 once the database, storage, and Bifrost are ready
```

## 4. Create the first credential

In the second terminal, export the same `WYRD_DATABASE_URL` as step 2, then run:

```bash
~/.config/wyrd/server/current/wyrd-server setup --tenant acme
```

On a fresh database, `setup` creates the platform root and the tenant `acme`, and prints two credentials:

- the platform root credential, `wyrd_global_…`, which creates and manages tenants;
- the tenant administrator's `admin_credential`, `wyrd_sk_…`, which you use in the next step.

Each credential is shown once and never written to the server log. Store both in your password manager now. Running `setup` again for the same tenant changes nothing and prints no credential.

## 5. Set your credentials

```bash
export WYRD_SERVER_URL=http://127.0.0.1:8080
export WYRD_API_KEY=wyrd_sk_…   # the admin_credential from setup
```

The CLI and every SDK read these two variables. The admin credential holds the `admin` role, so it can do everything in the quickstart. See [Identity and authorization](/understand/identity-and-authorization/) for how to give a deployed service its own credential instead.

## 6. Make a request

```bash
wyrd list
```

The tenant is empty, so the list is empty. An authentication error here means the credential is wrong or unset. Your server is ready for the quickstart.

## What can go wrong

| Symptom | Cause | Fix |
| --- | --- | --- |
| `wyrd server install` fails with a `WYRD_CLI_*_SERVER_*` code | The release could not be downloaded, verified, or run on this machine | See [Troubleshoot a deployment](/operate/troubleshoot-a-deployment/#server-installation) |
| The server exits with `WYRD_SQL_503_SCHEMA_NOT_READY` | The database is not migrated, or was migrated by a different build | Run `migrate` from step 3, then start the server again |
| The `WYRD_SQL_503_SCHEMA_NOT_READY` message says the login `is a superuser` | `WYRD_DATABASE_URL` names the container's `postgres` login | Use the `wyrd` login from step 2 |
| The server exits naming `WYRD_DATABASE_URL` | It is unset in this terminal | Export it and start again |
| The server exits with `host memory probe is unavailable` | `WYRD_BIFROST_MEMORY_LIMIT_BYTES` is unset on macOS | Set it as in step 2 |
| `migrate` reports a conflict | Another `migrate` is running against the same database | Wait for it to finish, then rerun |
| `setup` asks for `WYRD_PLATFORM_CREDENTIAL` | The platform root already exists | Export the stored `wyrd_global_…` credential. If it is lost, run `wyrd-server recover-root` |
| `wyrd list` fails with `WYRD_CLIENT_401_NO_CREDENTIALS` | `WYRD_API_KEY` is unset in this terminal | Export it as in step 5 |

## Start over

Stop the server, then delete the database and local data:

```bash
docker rm -f wyrd-dev-pg && docker volume rm wyrd-dev-pg
rm -rf .wyrd
```

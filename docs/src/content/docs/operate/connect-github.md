---
title: Connect GitHub
description: Register the deployment's GitHub App once, then connect a tenant to one GitHub installation and check which repositories it can reach.
pillar: wyrd
section: Operate
order: 6
---

Connect a tenant to GitHub so Wyrd can read its repositories and report on pull requests. The deployment operator registers one GitHub App and gives it to Wyrd. Each tenant administrator then connects their tenant to one installation of that App.

Wyrd stores no GitHub key or token. It mints a short-lived installation token, narrowed to one repository, each time it needs one.

## Before you start

- A running Wyrd server reachable at a public HTTPS address, from [Deploy for a team](/operate/deploy-wyrd-for-a-team/).
- Permission to create a GitHub App for your organization, on github.com or your GitHub Enterprise Server.
- A tenant administrator who can sign in to the Wyrd web app, and the `wyrd` CLI.

## 1. Register the GitHub App

In GitHub, open **Settings**, then **Developer settings**, then **GitHub Apps**, then **New GitHub App**. Replace `https://wyrd.example.com` with your server's public address.

| Field | Value |
| --- | --- |
| Callback URL | `https://wyrd.example.com/v1/integrations/github/callback` |
| Request user authorization (OAuth) during installation | Off |
| Setup URL | `https://wyrd.example.com/v1/integrations/github/setup` |
| Redirect on update | On |
| Webhook URL | `https://wyrd.example.com/v1/integrations/github/webhook` |
| Webhook secret | A long random string; keep it for step 2 |

Under **Permissions**, set repository permissions to exactly these:

| Permission | Access |
| --- | --- |
| Contents | Read-only |
| Pull requests | Read and write |
| Checks | Read and write |

Under **Subscribe to events**, select **Pull request** and **Push**. GitHub always sends every App its **Installation** and **Installation repositories** events, which Wyrd uses to follow suspension and repository removal.

Create the App. On its page, copy the **App ID**, the app's URL name (the slug in `https://github.com/apps/<slug>`) and the **Client ID**, generate a client secret, and generate and download a private key.

## 2. Give the App to Wyrd

Set these on every `wyrd-server` process and restart it. Secrets come only from the environment or a file, never from the server TOML file.

```bash
export WYRD_GITHUB_APP_ID=<App ID>
export WYRD_GITHUB_APP_SLUG=<slug>
export WYRD_GITHUB_PUBLIC_URL=https://wyrd.example.com
export WYRD_GITHUB_OAUTH_CLIENT_ID=<Client ID>
export WYRD_GITHUB_OAUTH_CLIENT_SECRET_FILE=/run/secrets/github-client-secret
export WYRD_GITHUB_WEBHOOK_SECRET_FILE=/run/secrets/github-webhook-secret
export WYRD_GITHUB_APP_PRIVATE_KEY_FILE=/run/secrets/github-app.pem
```

For GitHub Enterprise Server, also set `WYRD_GITHUB_WEB_ORIGIN=https://github.example.com` and `WYRD_GITHUB_API_ORIGIN=https://github.example.com/api/v3`. Without any `WYRD_GITHUB_*` setting the server starts with GitHub unavailable; with some but not all, it refuses to start and names the missing one.

## 3. Connect the tenant

As the tenant administrator, begin the connection:

```bash
export WYRD_SERVER_URL=https://wyrd.example.com
export WYRD_API_KEY=<admin_credential>
wyrd github connect
```

```text
{"authorization_url":"https://github.com/apps/wyrd-reviews/installations/new?state=...","expires_at":"..."}
```

Within 5 minutes, open `authorization_url` in a browser where you are signed in to the Wyrd web app as the same account. GitHub asks you to choose the repositories to install the App on, then to authorize the App. The browser ends on the connection's JSON with `"state":"active"`.

You can also start from GitHub: install the App from `https://github.com/apps/<slug>` while signed in to the Wyrd web app. Either way, Wyrd connects only after GitHub confirms the installation and that your GitHub account can reach it.

## 4. Check the connection

```bash
wyrd github status
```

```text
{"host":"github.com","app":{"id":4242,"slug":"wyrd-reviews"},"state":"active","installation_id":101,"repositories":[{"repository_id":"1","full_name":"acme/api"}],"connected_at":"...","updated_at":"..."}
```

`repositories` lists the repositories selected for the installation that your GitHub account could reach when you connected. `state` is `suspended` while the installation is suspended on GitHub. To change the selection, edit the installation on GitHub; Wyrd drops a removed repository as soon as GitHub reports it.

To release the installation, run `wyrd github disconnect`. Wyrd stops starting GitHub work for the tenant before it releases the installation, which another tenant may then connect. The same operations are `GithubConnection` in the Rust, Python, and TypeScript SDKs and the [`github_connection` MCP tools](/reference/mcp-tools/#github-connection).

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_GITHUB_503_NOT_CONFIGURED` | Set every variable in step 2 on every server process and restart |
| `WYRD_GITHUB_403_CONNECTION_REFUSED` with `transaction_invalid` | Run `wyrd github connect` again and open the new URL within 5 minutes, signed in to Wyrd as the account that ran it |
| `WYRD_GITHUB_403_CONNECTION_REFUSED` with `installation_inaccessible` | Authorize with a GitHub account that can see the installation, or install the App on that account's organization |
| `WYRD_GITHUB_409_CONNECTION_CONFLICT` with `installation_claimed` | The installation belongs to another tenant; disconnect it there first |

## Next

- [Grant and inspect permissions](/operate/grant-and-inspect-permissions/): connecting needs `identity_connections:write`, which `admin` holds.

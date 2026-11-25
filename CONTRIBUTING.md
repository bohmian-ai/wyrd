# Contributing to Wyrd

## Welcome

Thanks for contributing to Wyrd. This guide covers the local setup, validation
commands, and review expectations for this repository.

## Table of Contents

- [Contributing to Wyrd](#contributing-to-wyrd)
  - [Welcome](#welcome)
  - [Table of Contents](#table-of-contents)
  - [Environment Setup](#environment-setup)
  - [Contributing Changes](#contributing-changes)
  - [Coding Conventions](#coding-conventions)
  - [Git](#git)

## Environment Setup

Wyrd uses a Rust workspace, a SvelteKit frontend, and a future Python package
stub. Tooling is managed through `mise`.

```console
mise install
mise run check
mise run dev:full
```

If `mise install` fails, ensure mise itself is installed:
<https://mise.jdx.dev>.

The local dev stack starts:

- UI: <http://localhost:3000>
- wyrd-server: <http://localhost:8080/healthz>

All developer tasks live in `mise.toml`. There is no `Makefile` and no
`justfile`. Run `mise tasks` to list available commands.

### Pull request review in `dev:full`

`dev:full` also declares the demo app's reusable `code-review` Verifier. Once
the dev tenant's GitHub connection covers `bohmian-ai/wyrd-example-py`, it
registers that repository through the Cards API, bound to the Verifier, so each
non-draft pull request gets a `wyrd/code-review` Check and a `COMMENT` review.
Model answers come from MockServer; everything else is the real integration.
Without GitHub settings it prints `review       off: this server has no GitHub
App` and the rest of the stack runs unchanged.

1. Create your own GitHub App as in
   [Connect GitHub](docs/src/content/docs/operate/connect-github.md), with the
   same permissions (Contents read-only, Pull requests and Checks read and
   write) and events (Pull request, Push), installed on
   `bohmian-ai/wyrd-example-py`. Use the loopback server for the browser
   routes:

   | Field | Value |
   | --- | --- |
   | Callback URL | `http://127.0.0.1:8080/v1/integrations/github/callback` |
   | Setup URL | `http://127.0.0.1:8080/v1/integrations/github/setup` |
   | Webhook URL | your relay channel, such as `https://smee.io/<channel>` |

2. Forward the App's webhooks to the server with a relay, as
   [GitHub's webhook testing guide](https://docs.github.com/en/webhooks/testing-and-troubleshooting-webhooks/testing-webhooks)
   describes. Keep it running beside `dev:full`:

   ```console
   pnpm dlx smee-client --url https://smee.io/<channel> --target http://127.0.0.1:8080/v1/integrations/github/webhook
   ```

3. Export the App's settings in the shell that runs `dev:full`. Keep the three
   secrets in files outside the repository; never commit them:

   ```console
   export WYRD_GITHUB_APP_ID="<App ID>"
   export WYRD_GITHUB_APP_SLUG="<slug>"
   export WYRD_GITHUB_PUBLIC_URL="http://127.0.0.1:8080"
   export WYRD_GITHUB_OAUTH_CLIENT_ID="<Client ID>"
   export WYRD_GITHUB_OAUTH_CLIENT_SECRET_FILE="$HOME/.config/wyrd-dev-github/client-secret"
   export WYRD_GITHUB_WEBHOOK_SECRET_FILE="$HOME/.config/wyrd-dev-github/webhook-secret"
   export WYRD_GITHUB_APP_PRIVATE_KEY_FILE="$HOME/.config/wyrd-dev-github/app.pem"
   mise run dev:full
   ```

4. While it prints `review       waiting: GitHub connection is not_connected`,
   sign in at <http://127.0.0.1:3000/t/dev/login> as `development` /
   `development` (use `127.0.0.1`, not `localhost`, throughout), open
   <http://127.0.0.1:3000/t/dev/settings/integrations/github> and select
   **Connect GitHub**. Install
   the App on the demo repository and authorize it. The browser ends on the
   connection's JSON; go back to the UI yourself.

5. Wait for `review       registered bohmian-ai/wyrd-example-py as
   Repository/wyrd-example-py@<version>`. `review       FAILED` names the
   refusal; `does not cover` means the installation lacks the demo repository.

6. Open a non-draft pull request against `bohmian-ai/wyrd-example-py`. Its
   `wyrd/code-review` Check and `COMMENT` review appear on GitHub, and the
   Repository and its execution appear under **Repositories** and **Verify**.
   Admission alone is not success: wait for the Check. A title starting with
   `[wyrd-demo-fail]` gets a scripted high-severity finding and a failed
   Check. Further pull requests run the same way while the stack runs.

Every `dev:full` start uses a fresh database, so connect GitHub again after
each restart; the harness waits and registers the repository again.

## Contributing Changes

1. Create a branch for your change.
2. Make the smallest coherent change that satisfies the phase or issue.
3. Run the required checks from the repository root:

```console
mise run pre-pr
```

4. If your change touches the UI, also run:

```console
cd crates/wyrd/wyrd-server/wyrd-ui
pnpm run check
pnpm run build
```

5. Open a pull request after local validation passes.

Both CI workflows (`lints-test`, `codegen-check`) must be green before merge.

## Coding Conventions

- Rust edition `2024`. Workspace deps pin exact major.minor minimum; pre-1.0
  deps pin exact `x.y.z`. No wildcards.
- Library errors use `thiserror`. Binary errors use `anyhow`.
- Public Spec enums and structs are `#[non_exhaustive]`.
- Every wire type derives `schemars::JsonSchema`.
- PyO3 lives only in `sdks/wyrd-sdk-python` and an explicit allowlist.
- Client-tier crates may not depend on `sqlx`, `datafusion`, `deltalake`, or
  cloud SDKs.

### Configuration Secrets

Any `Config` type that carries secrets uses `secrecy::SecretString` with a
custom `Debug` impl that redacts the secret. Plain `String` for secrets is
forbidden.

### Error Code Format

All structured errors carry a stable code in the form
`WYRD_<DOMAIN>_<STATUS>_<SLUG>` (for example `WYRD_REGISTRY_404_CARD_NOT_FOUND`).

## Git

- Identity: `Thorrester <sjforrester32@gmail.com>`.
- `Co-Authored-By:` trailers are allowed when an agentic harness requires
  them; the author and committer stay the identity above.
- Branch names: `<short-slug>` for feature branches.

---
title: Deploy a Service from CI
description: Give CI and each Service its own key, declare and pull the Service in CI, and start the application from the bundle.
pillar: wyrd
section: Operate
order: 2
---

Ship a Service with two keys: one for CI, which declares the Service and pulls its bundle, and one for the running application, which loads that bundle. The bundle names the exact release, so the application needs nothing but its key.

| Who | Account | Role | Does |
| --- | --- | --- | --- |
| CI | `ci-deploy` | `editor` | Declares the Service and pulls its bundle |
| The application | `checkout-runtime` | `workload` | Loads the bundle, records Runs and observations, runs Verifiers |

## Before you start

- A Wyrd server, and a key with the `admin` role, such as the tenant administrator key.
- A Service declaration, such as the `service/` directory from [Define a Service](/build-and-verify/declare/define-a-service/). Its Service is named `checkout` here, with an exact `version` such as `1.2.0` in its `metadata`.

## 1. Create the CI account

Once per tenant, as an administrator:

```bash
wyrd principal credential create-principal --name ci-deploy --role editor
```

```text
principal_id: <uuid>
credential: <key>
This is the only time the credential is shown. Store it now.
```

Save the credential as the CI secret `WYRD_API_KEY`, and the server URL as `WYRD_SERVER_URL`. `editor` can declare Cards for every Service in the tenant, so one CI account covers all your repositories.

## 2. Create the Service's account

Once per Service, not once per release:

```bash
wyrd principal credential create-principal --name checkout-runtime --role workload --workload-name checkout
```

Store this credential where your deployment reads secrets. `workload` records evidence and runs Verifiers, but cannot change a Card, so a leaked runtime key cannot change what your Service is verified against. `--workload-name checkout` limits the key to releases of `checkout`, and makes it the account that scheduled Verifiers for `checkout` run as.

## 3. Declare and pull in CI

```bash
wyrd declare service/
wyrd pull --kind Service --name checkout --version 1.2.0 --output-dir bundle
```

`pull` downloads that release with every Card and file it uses. The version in your YAML is the release: declaring the same files again returns the same Card, and changed files under an unchanged version are refused with `WYRD_REGISTRY_409_SPEC_DRIFT`, so bump the version with every change you ship.

Copy `bundle/` into the application image.

## 4. Start the application from the bundle

Give the container the runtime key as `WYRD_API_KEY`, and `WYRD_SERVER_URL`. The application loads the bundle at startup:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/support-desk-app/python/desk.py" region="init" lang="python" title="desk.py" />

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/support-desk-app/rust/lib.rs" region="init" lang="rust" title="lib.rs" />

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/support-desk-app/typescript/server.ts" region="startup" lang="typescript" title="server.ts" />

</LangTab>
</LangTabs>

Loading is offline. From then on, every call sends the runtime key with the release named in the bundle, and the server records both on each row. You never set a release ID yourself.

`configure` fixes the Service name, version, and deployment environment that every span, log, metric, gateway call, and verification carries. Name and version come from the bundle; pass the environment your deployment runs in, such as `production`. Call it once, before the state's first use: a second call fails with `WYRD_SDK_409_STATE_CONFIG_FROZEN`. The environment is a label and grants nothing.

## 5. Rotate a key

Issue a second key on the same account, deploy it, then revoke the old one. Both keys work in between.

```bash
wyrd principal credential issue --principal <principal-id>
wyrd principal credential revoke --principal <principal-id> --credential-id <old-credential-id>
```

`wyrd principal credential list --principal <principal-id>` shows each key's ID and when it was last used.

## If it fails

| You see | Fix |
| --- | --- |
| `WYRD_AUTH_409_WORKLOAD_BINDING_CONFLICT` in step 2 | `checkout` already has an account. Issue another key on it instead of creating a second account |
| `WYRD_REGISTRY_409_SPEC_DRIFT` from `wyrd declare` | The files changed but `version` did not. Bump `version` in the Service YAML |
| `WYRD_PERMISSION_403_DENIED_RBAC` from `wyrd declare` | CI is using a key without `editor`, such as the runtime key |
| `WYRD_AUTH_403_WORKLOAD_CONTEXT_REQUIRED` from the application | It is not using `WyrdState` from the bundle. Load the bundle, or set `WYRD_WORKLOAD_UID` to the release UID |
| `WYRD_AUTH_401_WORKLOAD_UID_REFUSED` from the application | The bundle is from another Service than the key's `--workload-name`. Pair each Service's bundle with its own key |

## Next

[Identity and access](/understand/identity-and-authorization/) explains accounts, roles, and how a key and a bundle together identify a release.

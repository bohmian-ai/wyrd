---
title: Add people
description: Connect your identity provider, such as Okta or Keycloak, so people sign in to Wyrd, then grant them roles.
pillar: wyrd
section: Operate
order: 3
---

People sign in to Wyrd with a local username and password, or through your organization's identity provider, such as Okta or Keycloak. This guide connects a provider: then a person's account is created the first time they sign in, and it holds no role until you grant one. To add people without a provider, see [Without an identity provider](#without-an-identity-provider).

You connect the provider once per tenant. Wyrd only trusts a connection after a real test sign-in succeeds, so a typo in the issuer or client ID cannot lock your team out.

## Before you start

- A tenant and its `admin_credential`, from [Deploy for a team](/operate/deploy-wyrd-for-a-team/#4-create-the-first-tenant).
- The server's sealing key, from [step 3 of that guide](/operate/deploy-wyrd-for-a-team/#3-start-the-server-and-check-readiness). Wyrd encrypts the provider's client secret with it and refuses to test a connection without it.
- Administrator access to your identity provider.
- The `wyrd` CLI and `curl`.

## 1. Get the callback URL

Use the address people will open Wyrd at, not an internal one. Wyrd builds the callback URL from the address the request arrives at.

```bash
export WYRD_SERVER_URL=https://wyrd.example.com
export WYRD_API_KEY=<admin_credential>
TOKEN="$(wyrd auth access-token)"
curl -s -H "Authorization: Bearer $TOKEN" "$WYRD_SERVER_URL/v1/identity/oidc/connections"
```

```json
{"active":null,"candidate":null,"callback_url":"https://wyrd.example.com/auth/callback"}
```

The token lasts 5 minutes. If a later call returns `401`, run the `TOKEN=` line again.

## 2. Register Wyrd with your provider

Create an OpenID Connect application for Wyrd that uses the authorization code flow with PKCE, with the `callback_url` from step 1 as its only redirect URI. Wyrd requests the `openid`, `profile`, and `email` scopes.

<Tabs>
<Tab label="Keycloak">

1. In your realm, open **Clients**, then **Create client**. Choose **OpenID Connect** and set the client ID to `wyrd`.
2. Turn **Client authentication** off and leave **Standard flow** on. Wyrd then signs in as a public client with PKCE, so there is no secret to store.
3. Set **Valid redirect URIs** to the `callback_url`.
4. On the client's **Advanced** tab, set **Proof Key for Code Exchange Code Challenge Method** to `S256`.

The issuer is `https://<keycloak-host>/realms/<realm>`.

</Tab>
<Tab label="Okta">

1. Open **Applications**, then **Create App Integration**. Choose **OIDC - OpenID Connect** and **Web Application**.
2. Leave **Authorization Code** as the grant type, and set **Sign-in redirect URIs** to the `callback_url`.
3. Assign the app to the people or groups who should sign in to Wyrd, and copy the client ID and client secret.

The issuer is `https://<your-org>.okta.com`.

</Tab>
</Tabs>

## 3. Stage the connection

Save the connection as `connection.json`:

<Tabs>
<Tab label="Keycloak">

```json
{
  "issuer": "https://keycloak.example.com/realms/acme",
  "client_id": "wyrd",
  "client_auth": "Public",
  "claim_mapping": { "subject": "sub", "email": "email" }
}
```

</Tab>
<Tab label="Okta">

```json
{
  "issuer": "https://acme.okta.com",
  "client_id": "<client-id>",
  "client_auth": "SecretBasic",
  "client_secret": "<client-secret>",
  "claim_mapping": { "subject": "sub", "email": "email" }
}
```

</Tab>
</Tabs>

`subject` must be `sub`: Wyrd identifies a person by the provider's permanent ID, never by an email that can change. `email` is what `wyrd principal list --email` searches.

Stage it:

```bash
curl -s -X PUT -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  --data @connection.json "$WYRD_SERVER_URL/v1/identity/oidc/candidate"
```

The response shows the connection with `"state":"Candidate"` and a `revision`, such as `1`. Wyrd encrypts the client secret and never returns it. Delete `connection.json` once it is staged if it holds a secret.

## 4. Test it with a real sign-in

```bash
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d '{"expected_revision": 1}' "$WYRD_SERVER_URL/v1/identity/oidc/candidate/test"
```

```json
{"authorization_url":"https://keycloak.example.com/realms/acme/protocol/openid-connect/auth?..."}
```

Wyrd has already checked that the issuer answers and publishes signing keys. Open the `authorization_url` in a browser and sign in at your provider. The page ends with `Connection test complete.` The test creates no account.

## 5. Activate it

Within 15 minutes of the test:

```bash
curl -s -X POST -H "Authorization: Bearer $TOKEN" -H "Content-Type: application/json" \
  -d "{\"expected_revision\": 1, \"recovery_api_key\": \"$WYRD_API_KEY\"}" \
  "$WYRD_SERVER_URL/v1/identity/oidc/candidate/activate"
```

The response shows `"state":"Active"`. The recovery key proves an administrator can still get in with a key if the provider ever breaks; Wyrd checks it and does not store it. Keep the `admin_credential` in your secret manager for that reason.

## 6. Sign in and grant access

Each person signs in once from a terminal:

```bash
wyrd auth login --server https://wyrd.example.com --tenant acme
```

The browser opens at your provider; they sign in and approve the code the CLI shows. After sign-in, the CLI and every SDK on that machine use the saved login. That first sign-in creates their account, which holds no grant yet. As the administrator, find it and bind a role at a scope:

```bash
wyrd principal list --kind user --email alice@example.com
wyrd principal access grant --principal <principal-id> --kind user --grant editor:all
```

`editor:all` covers the whole tenant; `viewer:Repository/payments` would cover one Repository family. The grant reaches their current session within about 5 seconds; they do not sign in again. [Roles](/understand/identity-and-authorization/#roles) lists what each role allows.

## Map provider groups to roles

If your team already manages access with groups in the provider, Wyrd can take roles from them instead, so adding someone to a group is all it takes.

Wyrd reads the groups from a `groups` claim in the ID token. Add it at the provider:

<Tabs>
<Tab label="Keycloak">

Open **Client scopes**, then `wyrd-dedicated`, and add a **Group Membership** mapper. Set **Token Claim Name** to `groups`, turn **Full group path** off, and turn **Add to ID token** on.

</Tab>
<Tab label="Okta">

On the app's **Sign On** tab, edit **OpenID Connect ID Token**. Set **Groups claim type** to **Filter**, the claim name to `groups`, and a filter such as **Starts with** `wyrd-`.

</Tab>
</Tabs>

Then add `groups` to `claim_mapping` and a `group_role_map` to `connection.json`:

```json
"claim_mapping": { "subject": "sub", "email": "email", "groups": "groups" },
"group_role_map": {
  "wyrd-admins": ["admin"],
  "wyrd-editors": ["editor"],
  "wyrd-viewers": ["viewer"]
}
```

Repeat steps 3 to 5 with the new `revision` the stage call returns. Only a connection that passes its own test replaces the active one, so sign-in keeps working until then.

Wyrd recomputes a person's group roles at every sign-in. Remove someone from `wyrd-editors` and they lose `editor` the next time they sign in. Groups not in the map grant nothing, and a role name that does not exist in the tenant is ignored. Bindings you grant with `wyrd principal access grant` are separate: sign-in never adds or removes them.

## Without an identity provider

Wyrd keeps local Users of its own, so a team can sign in before or without connecting a provider. As the administrator, invite a person with the grants their account receives:

```bash
wyrd principal user invite --grant viewer:Repository/payments
```

The command prints an `invitation_code` and a `signup_url`, and shows the code only this once. Deliver the code separately. The person opens the signup URL, enters the code, and chooses a username and password. Each code works once and expires.

You can also create the User yourself. The password is read from standard input, so it never enters the command line or your shell history:

```bash
wyrd principal user create --username alice --grant editor:all < alice-password.txt
```

Either way, the account holds exactly the grants you named, and each `--grant` names its scope. The person then signs in from a terminal with `wyrd auth login --server https://wyrd.example.com --tenant acme`, signs in with their password in the browser, and approves the code the CLI shows. Revoke the account with `wyrd principal revoke` when they leave.

## If it fails

| You see | Fix |
| --- | --- |
| The provider says the redirect URI is invalid | Register the exact `callback_url` from step 1. Call the API at the public address, not an internal one |
| `WYRD_AUTH_409_CONNECTION_NOT_TESTED` from activate | The test expired after 15 minutes, or you staged a new revision since. Rerun step 4 |
| `WYRD_AUTH_409_CONNECTION_CONFLICT` | `expected_revision` is stale, or the recovery key is not an admin key of this tenant. Read the current revision with the step 1 call |
| A person signs in but every call returns `WYRD_PERMISSION_403_DENIED_RBAC` | They hold no grant yet. Bind one as in step 6, or check the `groups` claim and `group_role_map` if you map groups |

## Next

[Permissions](/operate/grant-and-inspect-permissions/) shows how to grant, revoke, and audit roles.

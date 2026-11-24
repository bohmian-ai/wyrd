---
title: Add people
browserTitle: Add people with single sign-on | Wyrd
description: Connect your identity provider, such as Okta or Keycloak, so people sign in to Wyrd, then grant them roles.
pillar: wyrd
section: Operate
order: 3
---

People sign in to Wyrd with a local username and password, or through your organization's identity provider, such as Okta or Keycloak. This guide connects a provider: then a person's account is created the first time they sign in, and it holds no role until you grant one. To add people without a provider, see [Without an identity provider](#without-an-identity-provider).

You connect the provider once per tenant. Wyrd only trusts a connection after a real test sign-in succeeds, so a typo in the issuer or client ID cannot lock your team out.

## Before you start

- A tenant and its `admin_credential`, from [Deploy for a team](/operate/deploy-wyrd-for-a-team/#4-create-the-first-tenant).
- The server's `[kms]` key, from [step 3 of that guide](/operate/deploy-wyrd-for-a-team/#3-start-the-server-and-check-readiness). Wyrd encrypts the provider's client secret with it and refuses to test a connection without it.
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

## Require company sign-in

Once the connection is active, you can make it the only way people sign in to the tenant. Password and Google or GitHub sign-in then stop, and so do the sessions and CLI sign-ins they started. API keys keep working.

1. Sign in to the UI through your company provider, as an account that holds `admin`.
2. Open **Settings**. **Require company SSO** shows how many active accounts cannot sign in through your company.
3. Check **Require company SSO for every person** and select **Enable**.

The page then reads `✓ Required`, and the sign-in page offers only your company provider. **Disable** works the same way. Disabling lets password and social sign-in start again, but every session and CLI sign-in that enabling ended stays ended.

<Aside type="caution" title="Wyrd never links accounts">
A password, Google or GitHub account and a company account are separate accounts, even with the same email. Requiring company sign-in moves no roles, keys or history to the company account. Grant the company accounts their roles before you enable it. The warning count tells you how many accounts this leaves out.
</Aside>

Enabling and disabling need a company sign-in from the last five minutes. An API key or a recovery session cannot change the policy. If your provider is down while company sign-in is required, people cannot sign in until it returns or you activate a replacement connection with the recovery key, as in step 5.

## Without an identity provider

Wyrd keeps local Users of its own, so a team can sign in before or without connecting a provider. As the administrator, invite a person with the grants their account receives:

```bash
wyrd principal user invite --recipient alice@example.com --recipient bob@example.com \
  --grant viewer:Repository/payments
```

Each recipient gets a separate `invitation_link`, printed only this once. Send each person their link. Until an operator configures a mail relay (see [Email invitations](#email-invitations)), Wyrd does not email links, and `delivery` reads `not_emailed`. The person opens the link and chooses a username and password. Each link works once, for 24 hours. Anyone holding a live link can use it, so revoke one that reached the wrong person. `GET /v1/principals/users/invitations` lists pending invitations without their links. `POST /v1/principals/users/invitations/{id}/resend` replaces a link, and the old one stops working. `DELETE /v1/principals/users/invitations/{id}` revokes it.

You can also create the User yourself. The password is read from standard input, so it never enters the command line or your shell history:

```bash
wyrd principal user create --username alice --grant editor:all < alice-password.txt
```

When a platform operator has enabled Google sign-in for the deployment, the invitation page also offers **Join with Google**. The person's account is then tied to that Google account, whatever its email address, and they return with **Continue with Google** on the sign-in page. A Google account that already has an account in the tenant cannot use a second link; the sign-in page tells them to sign in instead.

In the UI, go to **Settings**, then **Access**, then **Users**. Paste the email addresses and choose a role and a scope, and the page shows each link once. **Pending invitations** lists every live invitation, with **Resend** and **Revoke**.

Either way, the account holds exactly the grants you named, and each `--grant` names its scope. The person then signs in from a terminal with `wyrd auth login --server https://wyrd.example.com --tenant acme`, signs in with their password in the browser, and approves the code the CLI shows. The approval page opens at the address they use for the Wyrd UI when the server runs behind it. Revoke the account with `wyrd principal revoke` when they leave.

## Recovery email and password reset

A person with a password resets it on the sign-in page with **Reset your password**. With a mail relay, they enter their username and Wyrd emails a one-time code to the account's verified recovery email; the page answers every username the same, so it never says whether an account exists. Without a relay, or when the account has no verified recovery email, an administrator issues the code with `POST /v1/principals/users/{principal_id}/password-reset` and gives it to the person. A code works once, for fifteen minutes. Setting the new password signs the person out everywhere and ends their CLI sign-ins; API keys keep working.

Each person verifies their own recovery email under **Settings**, then **Account**, then **Recovery email**. The section appears only when the deployment has a mail relay, needs a sign-in from the last five minutes, and never shows the current address. Wyrd emails a link to the new address; opening it in the same browser while signed in finishes the change, and the previous address is told. A verified recovery email is also what **Find my workspaces** uses to list the workspaces that address can sign in to; an unverified profile email is never used for either.

## Email invitations

Wyrd can also email each invitation link through an SMTP relay you already use: your company's mail server or a transactional mail service such as Resend. Set up the sending account and domain with that service first. As a platform administrator, store the relay. The password is sealed and never returned, and tenant administrators can neither read nor change it:

```bash
curl -X PUT https://wyrd.example.com/platform/mail \
  -H "X-Wyrd-Access-Token: Bearer $PLATFORM_TOKEN" -H 'Content-Type: application/json' \
  -d '{"host":"smtp.example.com","port":587,"tls":"starttls","username":"wyrd","password":"…","sender":"invites@example.com"}'
```

`tls` is `starttls` (usually port 587), `tls` (usually 465) or `none` for a relay on a trusted local network. Leave out `username` and `password` for a relay that does not authenticate. Check it with `POST /platform/mail/test` and `{"recipient":"you@example.com"}`, which answers `accepted` or the relay's error.

From then on, Wyrd emails each new or resent link separately to its recipient and still returns it to you. `delivery` reads `accepted` when the relay took the message. That is not proof it reached an inbox. It reads `failed` when the relay refused it or could not be reached. The invitation stays pending, so copy the link or resend once the relay works. Wyrd does not queue or retry mail.

## If it fails

| You see | Fix |
| --- | --- |
| The provider says the redirect URI is invalid | Register the exact `callback_url` from step 1. Call the API at the public address, not an internal one |
| `WYRD_AUTH_409_CONNECTION_NOT_TESTED` from activate | The test expired after 15 minutes, or you staged a new revision since. Rerun step 4 |
| `WYRD_AUTH_409_CONNECTION_CONFLICT` | `expected_revision` is stale, or the recovery key is not an admin key of this tenant. Read the current revision with the step 1 call |
| `WYRD_AUTH_403_RECENT_LOGIN_REQUIRED` when you enable or disable company sign-in | Sign in through your company again, return to **Settings**, and confirm again |
| A person signs in but every call returns `WYRD_PERMISSION_403_DENIED_RBAC` | They hold no grant yet. Bind one as in step 6, or check the `groups` claim and `group_role_map` if you map groups |

## Next

[Permissions](/operate/grant-and-inspect-permissions/) shows how to grant, revoke, and audit roles.

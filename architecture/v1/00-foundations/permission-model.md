# Permission Model

Wyrd API authorization uses one typed vocabulary:

```rust
Permission { resource: Resource, action: Action }
```

`Permission` is the runtime RBAC tuple used to decide whether a verified
principal may call a Wyrd API surface. It is distinct from the Policy plane:
RBAC gates Wyrd API calls, while Policy gates card state and cross-service
runtime invokes.

There is no `Scope` model in the v1 runtime permission vocabulary. Handler
authorization should require a typed `Permission`, not a flat string.

## Vocabulary

`Resource` is a closed enum. Current resources are:

- `Cards`
- `Services`
- `Operators`
- `Evals`
- `Drift`
- `Artifacts`
- `Audit`
- `Policy`
- `Triggers`
- `ServiceAccounts`
- `Users`
- `Delegation`
- `AnyOf(Vec<Resource>)`
- `Wildcard`

`Action` is also a closed enum. Current actions are:

- `Read`
- `Write`
- `Delete`
- `Invoke`
- `Install`
- `Lock`
- `Run`
- `Issue`
- `AnyOf(Vec<Action>)`
- `Wildcard`

Adding a resource or action variant is a reviewed Rust contract change. Wyrd
does not support custom runtime permission strings or an `Other(String)`
escape hatch.

## Subsumption

`Resource::Wildcard` covers every resource. `Action::Wildcard` covers every
action. `AnyOf` covers any member it contains. Otherwise, resources and actions
cover only the identical variant.

A `Permission` covers a required permission only when both axes cover:

```rust
self.resource.covers(&required.resource)
    && self.action.covers(&required.action)
```

Examples:

- `{ Wildcard, Read }` covers `{ Cards, Read }`, but not `{ Cards, Write }`.
- `{ Cards, Wildcard }` covers `{ Cards, Read }` and `{ Cards, Write }`.
- `{ Wildcard, Wildcard }` covers every permission.
- `{ AnyOf([Operators, Evals]), Invoke }` covers operator and eval invokes,
  but not card invokes.

## PermissionSet

`PermissionSet` is a subsumption-aware collection of `Permission` values. On
insert, it keeps the set minimal:

- if an existing permission already covers the inserted permission, insertion
  is a no-op
- if the inserted permission covers existing entries, those entries are removed
- otherwise, the inserted permission is appended

All authorization checks should go through `PermissionSet::contains`. Builtin
administrative behavior is represented by normal wildcard permissions, not by
role-name bypasses in check code.

## JSONB Shape

Roles persist permissions as a JSONB array of permission objects in
`wyrd.auth_roles.permissions`. The JSON shape is the direct serde projection of
`Permission`:

```json
[
  {"resource": "cards", "action": "write"},
  {"resource": "cards", "action": "read"},
  {"resource": {"any_of": ["operators", "evals"]}, "action": "invoke"},
  {"resource": "delegation", "action": "issue"},
  {"resource": "wildcard", "action": "wildcard"}
]
```

The decode path reads this JSONB value into `Vec<Permission>` and builds a
`PermissionSet` for runtime checks. The JSON shape may evolve by adding new
closed enum variants or optional fields, but existing field names and variant
meanings must not change.

## Delegation

`Resource::Delegation` and `Action::Issue` model RFC 8693 token exchange. A
caller needs:

```rust
Permission { resource: Delegation, action: Issue }
```

to call `POST /auth/token` with
`grant_type=urn:ietf:params:oauth:grant-type:token-exchange`.

This keeps delegated-token issuance inside the same typed RBAC model as every
other Wyrd API call. The `runtime_admin` builtin role carries this permission;
broader admin roles may cover it through `{ Wildcard, Wildcard }`.

# Permission Model

Wyrd API authorization uses one typed vocabulary:

```rust
Permission { resource: Resource, action: Action, scope: PermissionScope }
```

Wyrd RBAC is the standard operation/object model. `resource` and `action` name
the *operation*; `scope` names the *objects* that operation may reach. A static
role-derived grant over a named object is still RBAC — it is not ABAC and it
does not enter the Policy plane. RBAC gates Wyrd API calls and the objects
those calls touch; Policy gates card state and cross-service runtime invokes.

Handler authorization requires a typed `Permission`, never a flat string. The
object axis has no string spelling: `resource:action` names an operation only.

## Scope

`PermissionScope` is a closed, domain-tagged union. Its initial variants are:

- `All` — every object of this permission's own resource and action. It never
  widens the resource or the action.
- `Bifrost(BifrostPermissionScope)` — one Bifrost schema or one Bifrost table.

`BifrostPermissionScope` is itself closed:

- `Schema(BifrostSchemaScope { catalog, schema })` covers every table currently
  or later resolved beneath that exact logical catalog and schema.
- `Table(BifrostTableScope { catalog, schema, table_uid })` covers exactly the
  registered table UID. A grant does not follow a dropped-and-recreated table,
  and the catalog and schema ride alongside the UID so the synchronous checker
  can evaluate schema containment without a catalog lookup.

Catalog and schema are the *logical* names a caller writes in SQL — catalog
`vala`, schema `logs` — not the flattened internal namespace `vala.logs` and not
the physical Iceberg catalog name. Both are validated as identifier segments;
an empty, over-long, or dotted value is rejected.

Scope is domain-tagged so a future Wyrd domain that needs object scope adds its
own typed variant rather than overloading someone else's identity spelling.

Bifrost object scope is valid only on Bifrost query reads. Attaching it to any
other resource — including `AnyOf` and `Wildcard` — is rejected at decode.

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

Scope coverage is independent of the other two axes. `All` covers every scope.
A schema scope covers itself and any table scope in that exact catalog/schema
pair. A table scope covers only its own UID. No Bifrost scope covers `All`,
because "every object" is strictly wider than any one schema or table.

A `Permission` covers a required permission only when all three axes cover:

```rust
self.resource.covers(&required.resource)
    && self.action.covers(&required.action)
    && self.scope.covers(&required.scope)
```

Examples:

- `{ Wildcard, Read, All }` covers `{ Cards, Read, All }`, but not
  `{ Cards, Write, All }`.
- `{ Cards, Wildcard, All }` covers `{ Cards, Read, All }` and
  `{ Cards, Write, All }`.
- `{ Wildcard, Wildcard, All }` covers every permission.
- `{ AnyOf([Operators, Evals]), Invoke, All }` covers operator and eval
  invokes, but not card invokes.
- `{ BifrostQuery, Read, Bifrost(Schema(vala, logs)) }` covers a required
  `{ BifrostQuery, Read, Bifrost(Table(vala, logs, <any uid>)) }`, but not a
  table in `vala.traces` and not the object-wide
  `{ BifrostQuery, Read, All }`.
- A wildcard grant reaches objects only because its scope is `All`. A wildcard
  or multi-resource grant cannot carry a Bifrost object scope at all: decode
  rejects it, so no narrow wildcard grant exists.

Grants are additive and absence is denial. There is no explicit-deny
precedence, no ownership, and no grant-option semantics.

## PermissionSet

`PermissionSet` is a subsumption-aware collection of `Permission` values. On
insert, it keeps the set minimal:

- if an existing permission already covers the inserted permission, insertion
  is a no-op
- if the inserted permission covers existing entries, those entries are removed
- otherwise, the inserted permission is appended

All authorization decisions go through `PermissionSet::contains`, which
requires all three axes to cover. Builtin administrative behavior is
represented by normal wildcard permissions, not by role-name bypasses in check
code.

`PermissionSet::covers_operation` answers the separate, coarser question "does
this principal hold this operation under *some* scope". It exists for public
routes that must admit a request before the objects it touches are known — a
Bifrost SQL route cannot resolve its tables until Oracle pins them. It is
admission, never authorization: the object decision is always
`contains` against the resolved object.

## JSONB Shape

Roles persist permissions as a JSONB array of permission objects in
`wyrd.auth_roles.permissions`. The JSON shape is the direct serde projection of
`Permission`:

```json
[
  {"resource": "cards", "action": "write", "scope": "all"},
  {"resource": "cards", "action": "read", "scope": "all"},
  {"resource": {"any_of": ["operators", "evals"]}, "action": "invoke", "scope": "all"},
  {"resource": "delegation", "action": "issue", "scope": "all"},
  {"resource": "wildcard", "action": "wildcard", "scope": "all"}
]
```

Object-scoped Bifrost grants use the same array:

```json
[
  {"resource": "bifrost_query", "action": "read", "scope": "all"},
  {"resource": "bifrost_query", "action": "read",
   "scope": {"bifrost": {"schema": {"catalog": "vala", "schema": "logs"}}}},
  {"resource": "bifrost_query", "action": "read",
   "scope": {"bifrost": {"table": {"catalog": "vala", "schema": "traces",
                                   "table_uid": "0199f0c1-2f7e-7c2a-9a6f-1c9b2f4d5e60"}}}}
]
```

`scope` is required on every permission. A two-field object is rejected rather
than promoted to `all`; there is no compatibility decoder, because this
contract has not shipped. Malformed catalog, schema, or table identities,
Bifrost scope on a non-Bifrost resource, and Bifrost scope on any action other
than exactly `read` are rejected at decode, so a corrupt role row surfaces by
name instead of resolving into effective authority.

The decode path reads this JSONB value into `Vec<Permission>` and builds a
`PermissionSet` for runtime checks. Role permissions remain the sole static
grant model: there is no second grant table, policy lookup, or SQL-style
`GRANT` language. The JSON shape may evolve by adding new closed enum variants
or optional fields, but existing field names and variant meanings must not
change.

## Delegation

`Resource::Delegation` and `Action::Issue` model RFC 8693 token exchange. A
caller needs:

```rust
Permission { resource: Delegation, action: Issue, scope: All }
```

to call `POST /auth/token` with
`grant_type=urn:ietf:params:oauth:grant-type:token-exchange`.

This keeps delegated-token issuance inside the same typed RBAC model as every
other Wyrd API call. The `runtime_admin` builtin role carries this permission;
broader admin roles may cover it through `{ Wildcard, Wildcard, All }`.

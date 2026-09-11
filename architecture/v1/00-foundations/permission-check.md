# Permission Check

`PermissionCheck` is the runtime authorization chokepoint for Wyrd API RBAC.
Handlers ask one checker whether a verified `Principal` has a required typed
`Permission`:

```rust
state
    .permission_check
    .check(&principal, &Permission::card_write())
    .into_result()?;
```

Handlers should not add `Principal::require_permission` or other shortcut
helpers. Routing all checks through the checker keeps audit, tracing, and
handler-boundary policy at one call site.

## Contract

`PermissionCheck::check` is synchronous and infallible:

```rust
fn check(&self, principal: &Principal, permission: &Permission) -> PermissionVerdict;
```

Authorization denial is a `PermissionVerdict::Deny`, not a Rust error. The
handler boundary maps `PermissionDenyReason::Rbac` into the public Wyrd error
catalog.

The trait takes only `&Principal` and `&Permission`, and that signature is
stable. There is no `TargetRef` parameter because the required object is
already carried *inside* `Permission`, as its `PermissionScope`. A caller that
needs to authorize one Bifrost table builds the required permission at that
table's scope and passes it here; the checker still decides by containment over
the principal's role-derived `effective_permissions`.

This is object-scoped RBAC, not ABAC. The object is a static, typed identity
named by a role grant, so the decision stays a pure in-memory function with no
query-path database or network lookup. Attribute-dependent decisions remain in
the Policy plane.

## RbacCheck

`RbacCheck` is the RBAC `PermissionCheck` implementation. It is
stateless and returns `Allow` when:

```rust
principal.effective_permissions.contains(permission)
```

The check uses the same three-axis `PermissionSet` subsumption rules documented
in `permission-model.md`, including wildcard resource/action coverage and
`PermissionScope` containment. A grant covers a requirement only when its
resource, action, *and* scope all cover. Builtin administrative
behavior is represented by normal permissions, not by role-name bypasses inside
the checker.

## Coarse Route Admission

A public route that cannot yet name the objects a request will touch — a
Bifrost SQL query, whose tables are known only after Oracle pins them — admits
on `PermissionSet::covers_operation`, which ignores scope. That is admission,
not authorization: the authoritative decision is taken through this checker
against each resolved object before any data is read. No surface may substitute
coarse admission for the object decision.

## Policy Plane

CEL and ABAC do not live behind this trait. Policy evaluation belongs to the
Policy plane and its `/v1/authz/check` surface, which has its own decision
shape. `PermissionDenyReason` intentionally has no ABAC variant.

## Delegation Issuance Check

Token exchange uses the same RBAC chokepoint before issuing a delegated token:

```rust
state
    .permission_check
    .check(&caller_principal, &Permission::delegation_issue())
    .into_result()?;
```

The `runtime_admin` builtin role carries `Permission::delegation_issue()`, and
broader administrators may cover it through `Permission::wildcard()`.

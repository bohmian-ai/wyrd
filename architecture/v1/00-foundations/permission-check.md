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
future handler-boundary behavior at one call site.

## Contract

`PermissionCheck::check` is synchronous and infallible:

```rust
fn check(&self, principal: &Principal, permission: &Permission) -> PermissionVerdict;
```

Authorization denial is a `PermissionVerdict::Deny`, not a Rust error. The
handler boundary maps `PermissionDenyReason::Rbac` into the public Wyrd error
catalog.

The trait takes only `&Principal` and `&Permission`. There is no `TargetRef`
because this foundation is RBAC only: it checks the principal's
role-derived `effective_permissions`.

## RbacCheck

`RbacCheck` is the stage's only `PermissionCheck` implementation. It is
stateless and returns `Allow` when:

```rust
principal.effective_permissions.contains(permission)
```

The check uses the same `PermissionSet` subsumption rules documented in
`permission-model.md`, including wildcard coverage. Builtin administrative
behavior is represented by normal permissions, not by role-name bypasses inside
the checker.

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

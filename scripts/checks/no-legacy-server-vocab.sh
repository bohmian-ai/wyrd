#!/usr/bin/env bash
# WHY THIS FILE EXISTS: server architectural conventions — how routes are
# mounted, how auth headers flow, how errors serialize, how middleware logs —
# are easy to violate accidentally and hard to audit in review. A scattered
# route prefix breaks the router contract silently. A second IntoResponse impl
# produces inconsistent HTTP error bodies depending on import resolution. An
# orphan From<WyrdError> for tonic::Status blocks the single authorized
# conversion path. Auth headers consumed in the wrong layer bypass the
# middleware stack. These aren't caught by the compiler; they're caught here.
#
# WHAT IT CHECKS: named anti-patterns across wyrd-server and wyrd-auth source —
# route prefix discipline, auth header layering, JWT claim encoding, error
# serialization uniqueness, Rust orphan-rule compliance, and middleware logging
# hygiene. Predecessor vocabulary is also excluded to prevent migration drift
# from re-entering active code.
set -e

# 1. Route prefix lock: /api/v1 may only appear in the router mount points, not scattered handlers
! rg -n --no-heading -e '/api/v1' crates/wyrd/wyrd-server/src/ \
    --glob '!http/router.rs'

# 2. Identity vocabulary (server source + server-tier auth domain crate)
! rg -n --no-heading -e '\bActor\b' crates/wyrd/wyrd-server/src/ crates/wyrd/wyrd-auth/src/
! rg -n --no-heading -e '\bScope\b' crates/wyrd/wyrd-server/src/ crates/wyrd/wyrd-auth/src/

# 3. JWT claim leak
! rg -n --no-heading -e '"scopes"' crates/wyrd/wyrd-server/src/ crates/wyrd/wyrd-auth/src/

# 4. Predecessor names
! rg -n --no-heading -e 'opsml|scouter' crates/wyrd/wyrd-server/src/ crates/wyrd/wyrd-auth/src/

# 5. Local plan docs, when present: predecessor names are allowed only in audit
#    and verification files. `.dev/` is intentionally ignored and is not
#    available in every clean worktree, so its absence is not an audit error.
if [[ -d .dev/plan/foundations/07-server-skeleton ]]; then
    ! rg -n --no-heading -e 'opsml|scouter' .dev/plan/foundations/07-server-skeleton \
        --glob '!11-parity-audit.md' --glob '!10-verification.md'
fi

# 6. Predecessor domain nouns
! rg -n --no-heading -e '\b(SubAgent|Skill|RecoveryCode|ScouterApiClient)\b' \
    crates/wyrd/wyrd-server/src/ crates/wyrd/wyrd-auth/src/

# 7. Single IntoResponse impl for Wyrd errors
COUNT=$(rg -n --no-heading -e 'impl[[:space:]]+IntoResponse[[:space:]]+for[[:space:]]+Wyrd' crates/ | wc -l)
test "$COUNT" -eq 1

# 8. Orphan rule: no From<WyrdError> for tonic::Status (impl blocks only, not comments)
! rg -n --no-heading -e 'impl[[:space:]]+From<WyrdError>[[:space:]]+for[[:space:]]+([a-zA-Z_:]+::)?Status\b' crates/
! rg -n --no-heading -e '^[[:space:]]*impl[^/]*From<WyrdError>[[:space:]]+for[[:space:]]+\S*tonic\S*::' crates/

# 9. Authorization header not consumed by auth/route handlers
! rg -n --no-heading -e 'header::AUTHORIZATION' \
    crates/wyrd/wyrd-server/src/auth/ \
    crates/wyrd/wyrd-server/src/components/auth/ \
    crates/wyrd/wyrd-server/src/components/admin/ \
    crates/wyrd/wyrd-server/src/components/authz/

# 10. Wyrd-Caller-Identity not consumed
! rg -n --no-heading -e 'Wyrd-Caller-Identity' crates/wyrd/wyrd-server/src/ \
    | rg -n -e '\.extract|\.get'

# 11. tower-http body-limit layer absent from server source (F-02 closeout).
#     LengthLimitError from http_body_util is permitted in the custom body limiter.
! rg -n --no-heading -e 'RequestBodyLimitLayer' \
    crates/wyrd/wyrd-server/src/

# 12. Tower middleware logs do not format raw inner errors (F-05 closeout).
#     Scoped to middleware only — domain error functions in error.rs log raw
#     sqlx/storage errors deliberately and are excluded from this gate.
! rg -n --no-heading -e 'error\s*=\s*%error\b' \
    crates/wyrd/wyrd-server/src/http/middleware/

# Lead direction: FIND-TASK-004-11 withdrawn

Decided by: the wyrd-run lead, 2026-10-02, under the human owner's standing
direction to follow conventional practice. The human owner may overrule.

`20260925000001_auth_login_state_binding.sql` and
`20261001000002_auth_connection_test_state.sql` were created inside this
unmerged change (TASK-002 and TASK-003). Neither exists on `main`, and no
release or deployment has applied them. Editing an unreleased migration
before merge is normal practice. Migration immutability applies once a
migration has shipped. Adding an ALTER migration here would only rewrite a
table this same change creates.

Do not implement FIND-TASK-004-11. Keep the edited migrations as they are.
Every other finding in `TASK-004-R2-production-readiness-fixes.md` stands.

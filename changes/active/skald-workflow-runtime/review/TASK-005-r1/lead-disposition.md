# TASK-005 R1 Lead Disposition

Human direction: remediate only critical bugs on the primary path; otherwise
proceed to the next step.

| Finding | Class | Disposition | Reason |
|---|---|---|---|
| `FIND-TASK-005-1` | DRIFT | Deferred | `wyrd workflow run --file` with `--server` is refused with a usage error; the configured or ambient endpoint works and is journey-proven. Allowing the explicit override needs a spec decision on the shared-client composition surface; follow-up, not a blocker. |
| `FIND-TASK-005-2` | VIOLATION | Deferred | Input is read before an invalid mode combination is refused; error path ordering only. |
| `FIND-TASK-005-3` | VIOLATION | Deferred | Test-helper import placement only. |
| `FIND-TASK-005-4` | VIOLATION | Deferred | Failure to install the Ctrl-C listener is reported as an interruption; an OS failure that is not on the primary path. |

TASK-005 is accepted with these findings open. They remain visible to
`$wyrd-change-review`.

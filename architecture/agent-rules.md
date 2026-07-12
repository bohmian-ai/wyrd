# Agent Rules

The following rules are explicitly defined for the agents in the system. These rules govern the behavior, interactions, and responsibilities of agents to ensure a consistent and efficient operation within the architecture.

- Cargo features must be earned. Creating new cargo features can invalidate compilation caches. Cargo features and mise tasks are designed around minimizing re-compilation across task suites. This is important. Wyrd is a big system.
- Raw PgPools are not allowed. Wyrd sql makes use of OperatorPool (admin work) and TenantConn (tenant work). All queries are validated through these pools before executing to check permissions and ensure scoped access. Using raw PgPools bypasses this validation and can lead to security vulnerabilities and inconsistent behavior.
"""Tenant Operator connections: Slack, PagerDuty, and HTTP credentials.

The server encrypts every secret, authorizes and audits each request, and only
ever returns redacted metadata; this handle only calls it::

    connections = OperatorConnections()
    view = connections.create(
        {"provider": "slack", "name": "ops-slack", "workspace_id": "T0001",
         "bot_token": token}
    )
    connections.update(view["connection_id"], {"provider": "slack", "bot_token": rotated})
    connections.disable(view["connection_id"])

Operator Cards name a connection by provider and name; rotating its secret
never requires a Card revision.
"""

from .._wyrd.operators import OperatorConnections

__all__ = ["OperatorConnections"]

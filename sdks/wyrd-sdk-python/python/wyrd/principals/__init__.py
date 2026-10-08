"""Tenant principals: discovery, credentials, and direct Role assignment.

The server authorizes and audits every call; this handle only calls it::

    principals = Principals()
    page = principals.list(email="ada@example.com")
    user = page["principals"][0]["principal_id"]
    principals.grant_role(user, "editor")

A Role change reaches the principal at its next token. Identity-provider
assignments are owned by login; ``revoke_role`` removes only direct grants.
"""

from .._wyrd.principals import Principals

__all__ = ["Principals"]

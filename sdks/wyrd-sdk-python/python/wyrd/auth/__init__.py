"""Tenant principals: discovery, credentials, and scoped role bindings.

The server authorizes and audits every call; this handle only calls it::

    principals = Principals()
    page = principals.list(email="ada@example.com")
    user = page["principals"][0]["principal_id"]
    principals.bind({
        "subject": {"principal": {"id": user, "kind": "user"}},
        "role": "editor",
        "scope": "all",
    })

A binding change reaches the account at its next request. Identity-provider
bindings are owned by login; ``unbind`` removes only direct bindings.
"""

from .._wyrd.auth import Principals

__all__ = ["Principals"]

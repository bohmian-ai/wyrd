"""The tenant GitHub connection: connect, read, and disconnect the deployment's GitHub App.

The server verifies the installation with GitHub, authorizes and audits each
request, and only ever returns redacted metadata; this handle only calls it::

    github = GithubConnection()
    receipt = github.begin()
    # open receipt["authorization_url"] in a browser signed in to Wyrd
    view = github.get()
    github.disconnect()
"""

from .._wyrd.github import GithubConnection

__all__ = ["GithubConnection"]

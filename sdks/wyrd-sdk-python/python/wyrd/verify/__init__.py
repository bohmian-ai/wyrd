"""Evidence for Verifiers and the executions that judge it.

``File`` and ``ArtifactRef`` name media evidence for ``Observe.verify`` and
``Observe.submit``. ``submit`` returns an ``Execution`` for Run evidence, which
any process can rebuild from its ID and wait on::

    execution = state.run("agent").observe.submit("policy-check", answer=text)
    result = wyrd.verify.Execution.connect(execution.id).wait(60)

Window samples return ``Recorded``: the Verifier's schedule judges them later.
"""

from .._wyrd.verify import ArtifactRef, Execution, File, Recorded, VerificationResult

__all__ = [
    "ArtifactRef",
    "Execution",
    "File",
    "Recorded",
    "VerificationResult",
]

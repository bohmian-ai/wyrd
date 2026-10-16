import os
from typing import Any, Literal

from ..cards import CardRef
from ..client import WyrdClient

class File:
    """A local file to verify, read only when ``verify`` or ``submit`` uses it.

    Constructing a ``File`` performs no IO and registers nothing. During the
    call the client hashes it with SHA-256, asks the server which digests it
    lacks, and uploads only those straight to tenant storage. An uploaded file
    is evidence pinned by its digest, never an Artifact Card.
    """

    def __init__(
        self,
        source: str | os.PathLike[str] | bytes,
        *,
        format: str | None = None,
        filename: str | None = None,
    ) -> None:
        """Name a file at a path, or in-memory bytes.

        Args:
            source: a path, opened only during the call, or the file's bytes.
            format: the canonical format, such as ``"pdf"`` or ``"pptx"``.
                Omitted, it is inferred from the filename extension, else the
                slot's single accepted format.
            filename: a display name; defaults to the path's file name.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a ``source`` of
                another type or an unknown ``format``.
        """
        ...

class ArtifactRef:
    """A registered Artifact's file, verified with its normal authorization."""

    def __init__(self, artifact: CardRef, path: str | None = None) -> None:
        """Select ``artifact``'s only file, or ``path`` within it.

        Args:
            artifact: the exact Artifact Card reference.
            path: the file inside the Artifact, when it holds several.
        """
        ...

class Recorded:
    """``submit``'s answer for scheduled-window samples.

    The samples were recorded; the Verifier's schedule judges the window
    later, so there is no execution to wait for.
    """

class VerificationResult:
    """The attributable result of one Verifier execution.

    A ``failed`` verdict is an ordinary return value, not an exception.
    """

    @property
    def execution_id(self) -> str:
        """Server-issued execution ID."""
        ...

    @property
    def run_id(self) -> str | None:
        """The caller's application Run, when there is one."""
        ...

    @property
    def verifier(self) -> CardRef:
        """The exact Verifier executed."""
        ...

    @property
    def subject(self) -> CardRef:
        """The exact subject judged."""
        ...

    @property
    def verdict(self) -> Literal["passed", "failed", "inconclusive"]:
        """The ``decide`` rule applied to every applicable requirement."""
        ...

    @property
    def requirements(self) -> list[dict[str, Any]]:
        """Every requirement's judgment as its wire mapping, in plan order."""
        ...

    @property
    def steps(self) -> list[dict[str, Any]]:
        """Every step's result as its wire mapping, in plan order."""
        ...

    @property
    def evidence(self) -> dict[str, dict[str, Any]]:
        """The pinned original evidence by slot, as wire declarations."""
        ...

class Execution:
    """One queued Verifier execution, addressed by its server-issued ID.

    Returned by ``Observe.submit`` for Run evidence, or rebuilt in any process
    with ``connect``. The queue, retries, and results stay server-owned.
    """

    @staticmethod
    def connect(id: str, client: WyrdClient | None = None) -> Execution:
        """Rebuild the handle of execution ``id``.

        Args:
            id: the execution ID ``submit`` returned.
            client: the client every status read is sent as. Omitted, the
                ambient chain resolves as ``WyrdClient`` does.

        Returns:
            The connected handle.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for an ``id`` that is not
                an execution ID, and the client's configuration or credential
                error.
        """
        ...

    @property
    def id(self) -> str:
        """The execution ID."""
        ...

    def status(self) -> dict[str, Any]:
        """Read the execution's status, terminal error, and visible result.

        Returns:
            The wire mapping with ``execution_id``, ``status``, and, when
            present, ``error`` and ``result``.

        Raises:
            WyrdError: the server's not-found error before the execution is
                visible, and its authorization or transport error.
        """
        ...

    def wait(self, timeout: float) -> VerificationResult:
        """Poll until the execution yields a result, fails, or ``timeout`` passes.

        Polls back off from 100 ms to at most 2 s. A not-yet-visible
        execution and a completion whose result is not visible yet keep
        being polled; a result is never invented from a status.

        Args:
            timeout: seconds to keep polling.

        Returns:
            The execution's result.

        Raises:
            WyrdError: the execution's own error when it settled as errored,
                cancelled, or timed out, including a delayed
                ``WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT``;
                ``WYRD_SDK_504_VERIFICATION_WAIT_TIMEOUT`` when ``timeout``
                passes first; ``WYRD_SPEC_400_VALIDATION`` for a negative
                ``timeout``; and any other status-read error.
        """
        ...

__all__ = [
    "ArtifactRef",
    "Execution",
    "File",
    "Recorded",
    "VerificationResult",
]

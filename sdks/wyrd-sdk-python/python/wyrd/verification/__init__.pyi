# AUTO-GENERATED STUB FILE. DO NOT EDIT.
# pylint: disable=redefined-builtin, invalid-name, dangerous-default-value
#### begin imports ####
from collections.abc import Mapping
from typing import Any

#### end of imports ####

class Verification:
    """Tenant-scoped Verification control-plane handle.

    Every call blocks with the GIL released until the server answers. A started
    run is durably enqueued, not finished; poll ``get_run``.
    """

    def __init__(self, server_url: str | None = None, credential: str | None = None) -> None:
        """Build a handle; omitted arguments fall through the client configuration.

        No network call happens here.

        Raises:
            WyrdError: When the server URL or credential cannot be resolved.
        """
        ...

    def get_binding(self, binding_id: str) -> dict[str, Any]:
        """Read one binding's identities, activity, readiness, and cursor.

        Returns the ``VerificationBindingStatus`` wire object: ``binding_id``,
        ``owner_card_uid``, ``subject_card_uid``, ``verifier_uid``, ``active``,
        ``readiness``, ``next_run_at``, ``last_activated_at``, and ``last_run_id``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a non-UUID ID,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``cards:read``, and
                ``WYRD_VERIFICATION_404_BINDING_NOT_FOUND`` for an unknown
                binding.
        """
        ...

    def start_run(self, request: Mapping[str, Any], idempotency_key: str | None = None) -> str:
        """Durably enqueue one manual Drift run and return its run ID.

        ``request`` is the ``StartVerificationRunRequest`` wire object: a
        ``target`` of ``{"kind": "binding", "binding_id": ...}`` or
        ``{"kind": "verifier", "verifier_uid": ..., "subject_card_uid": ...}``
        and an ``input`` of ``{"kind": "drift_window", "start": ..., "end": ...}``.
        A retry with the same ``idempotency_key`` and request returns the same
        run ID.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a malformed request,
                ``WYRD_VERIFICATION_400_INVALID_WINDOW`` or
                ``WYRD_VERIFICATION_400_INVALID_TARGET`` for an unusable run,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``evals:run`` or
                subject scope, ``WYRD_VERIFICATION_404_BINDING_NOT_FOUND``,
                ``WYRD_VERIFICATION_409_VERIFIER_NOT_READY``, and
                ``WYRD_REGISTRY_409_IDEMPOTENCY_CONFLICT`` when the key was used
                for a different request.
        """
        ...

    def get_run(self, run_id: str) -> dict[str, Any]:
        """Read one run's status, requester, result pointer, and dispatches.

        Returns the ``VerificationRunStatus`` wire object: ``run_id``,
        ``status``, ``requested_by_principal_id``, ``result_id``, ``error``,
        and ``dispatches``.

        Raises:
            WyrdError: ``WYRD_SPEC_400_VALIDATION`` for a non-UUID ID,
                ``WYRD_PERMISSION_403_DENIED_RBAC`` without ``cards:read``, and
                ``WYRD_VERIFICATION_404_RUN_NOT_FOUND`` for an unknown run.
        """
        ...

__all__ = ["Verification"]

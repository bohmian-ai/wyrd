"""Verification binding status, manual Verifier runs, and run status.

The server decides readiness, authorizes and audits each request, and enqueues
runs; this handle only calls it::

    verification = Verification()
    binding = verification.get_binding(binding_id)
    run_id = verification.start_run(
        {
            "target": {"kind": "binding", "binding_id": binding_id},
            "input": {"kind": "drift_window", "start": start, "end": end},
        },
        idempotency_key="nightly-2026-09-17",
    )
    run = verification.get_run(run_id)

Binding IDs come from a Card's ``status.verification.binding_ids``. Verdicts
and Drift details are read from Bifrost by the run's ``result_id``.
"""

from .._wyrd.verification import Verification

__all__ = ["Verification"]

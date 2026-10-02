"""Verification binding status, manual Verifier runs, run status, and direct
execution.

The server decides readiness, authorizes and audits each request, and enqueues
runs or judges supplied input inline; this handle only calls it::

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
    judgment = verification.execute(
        {
            "verifier_uid": verifier_uid,
            "subject_card_uid": subject_uid,
            "input": {"kind": "eval_record", "context": {"answer": "yes"}},
        }
    )

Binding IDs come from a Card's ``status.verification.binding_ids``. Queued
verdicts and Drift details are read from Bifrost by the run's ``result_id``;
a direct execution returns its verdict and detail in the response.
"""

from .._wyrd.verification import Verification

__all__ = ["Verification"]

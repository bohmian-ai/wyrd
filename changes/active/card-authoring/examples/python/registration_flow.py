"""Proposed flow: declare once, receive ready state, verify multiple requests.

For local Artifacts, the SDK uses ArtifactUpload::prepare and existing upload
machinery to publish each first as a sole submission; then it declares the graph
with exact registered references. Existing Artifact CardRefs cause no upload.
If graph declaration fails afterward, publication remains; no rollback is promised.
The call to run.observe.verify and its result properties are existing APIs.
"""

from evidence import permitted_request, prohibited_identifier_request
from incremental_style import local_policy_service
from wyrd import WyrdError, WyrdState


def main() -> None:
    service = local_policy_service()

    # Declaration includes the captured dependencies of the Service.
    state = WyrdState.declare(service)

    for request in (permitted_request(), prohibited_identifier_request()):
        run = state.run()
        try:
            result = run.observe.verify(
                "sensitive-report-release",
                report=request.report,
                recipient=request.recipient,
                purpose=request.purpose,
                destination=request.destination,
            )
        except WyrdError as error:
            print(f"hold for review: verification request error {error.code}")
            continue

        decision = "release" if result.verdict == "passed" else "hold for review"
        print(f"{decision}: {result.verdict}")
        print(f"subject: {result.subject}; verifier: {result.verifier}")
        for requirement in result.requirements:
            print(
                requirement["id"],
                requirement["outcome"],
                requirement["reason"],
                requirement.get("citations", []),
            )
        print("pinned evidence:", result.evidence)


if __name__ == "__main__":
    main()

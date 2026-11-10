"""Compose and verify a two-component Service from its component modules."""

from components.release_reviewer.agent import agent as release_reviewer
from components.release_reviewer.verifier import Release
from components.release_reviewer.verifier import verifier as release_controls
from components.report_writer.agent import agent as report_writer
from components.report_writer.verifier import Report
from components.report_writer.verifier import verifier as report_quality
from wyrd import WyrdState
from wyrd.verify import Component, Service


def main() -> None:
    service = Service(
        "finance-release",
        components=[
            Component(
                "report-writer",
                report_writer,
                verified_by=[report_quality],
            ),
            Component(
                "release-reviewer",
                release_reviewer,
                verified_by=[release_controls],
            ),
        ],
    )
    state = WyrdState.declare(service)

    try:
        report = Report(
            text="Quarterly revenue was $120, up from $100 in the prior quarter.",
            source="Current-quarter revenue: $120. Prior-quarter revenue: $100.",
        )
        with state.run("report-writer") as run:
            report_result = run.observe.verify("report-quality", report=report)
            print(f"Report quality: {report_result.verdict}")

        release = Release(
            report=report.text,
            recipient_authorized=True,
            destination_private=True,
            purpose="Provide the quarterly revenue report to the appointed financial auditor.",
        )
        with state.run("release-reviewer") as run:
            release_result = run.observe.verify("release-controls", release=release)
            print(f"Release controls: {release_result.verdict}")
    finally:
        state.shutdown()


if __name__ == "__main__":
    main()

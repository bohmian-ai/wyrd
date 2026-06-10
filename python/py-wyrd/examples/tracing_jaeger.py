"""
Wyrd + Jaeger tracing example.

Prerequisites:
    docker compose -f docker/jaeger/docker-compose.yaml up -d
    export OPENAI_API_KEY=...
    pip install opentelemetry-sdk opentelemetry-exporter-otlp-proto-grpc

Usage:
    python python/py-wyrd/examples/tracing_jaeger.py

View traces at http://localhost:16686 - search for service "wyrd".
"""

from opentelemetry import trace
from opentelemetry.exporter.otlp.proto.grpc.trace_exporter import OTLPSpanExporter
from opentelemetry.sdk.resources import Resource
from opentelemetry.sdk.trace import TracerProvider
from opentelemetry.sdk.trace.export import BatchSpanProcessor

from wyrd import Agent, OtelObserver, Workflow
from wyrd.prompt import Prompt


def configure_otel() -> None:
    resource = Resource.create({"service.name": "wyrd"})
    exporter = OTLPSpanExporter(endpoint="http://localhost:4317", insecure=True)
    provider = TracerProvider(resource=resource)
    provider.add_span_processor(BatchSpanProcessor(exporter))
    trace.set_tracer_provider(provider)


def main() -> None:
    configure_otel()

    agent = Agent(
        name="greeter",
        prompt=Prompt(
            provider="openai",
            model="gpt-4o-mini",
            system="You are a concise greeter.",
            messages=[{"role": "user", "content": "Say hello to {{name}}."}],
            variables=["name"],
        ),
    )

    workflow = Workflow.sequential("jaeger-greeting", agent, observers=[OtelObserver()])
    result = workflow.run({"name": "world"})
    print(result.final_output)
    print("View trace at http://localhost:16686")


if __name__ == "__main__":
    main()

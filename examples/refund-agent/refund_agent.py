from opentelemetry import trace
from wyrd import WyrdState
from wyrd.verify import Check, ComparisonOperator, InputSlot, Requirement, Service, Verifier

# Synthetic order records the agent's lookup tool reads.
ORDERS = {"1042": {"delivered": "2 May", "refundable": True}}

# ANCHOR: verifier
refund_workflow = Verifier(
    "refund-workflow",
    inputs={"reply": str, "trace": InputSlot.trace()},
    requirements=[
        # Judges the answer only: a reply that skipped the lookup passes it too.
        Requirement(
            "reply-names-order",
            "The reply names the customer's order.",
            Check.assert_("inputs.reply", ComparisonOperator.Contains, "Order 1042"),
        ),
        # Judges what ran: the agent's trace must hold a look-up-order span.
        Requirement(
            "looked-up-order",
            "The agent looked up the order before replying.",
            Check.trace("inputs.trace", "$.spans[*].name", ComparisonOperator.IsSuperset, ["look-up-order"]),
        ),
    ],
)
# ANCHOR_END: verifier

state = WyrdState.declare(Service("refund-agent", verified_by=[refund_workflow]))
state.start_telemetry()  # export the agent's spans to Wyrd
state.start_bifrost()  # so flush can wait until they are stored
tracer = trace.get_tracer("refund-agent")


# ANCHOR: agent
def look_up_order(order_id: str) -> dict:
    with tracer.start_as_current_span("look-up-order", attributes={"order_id": order_id}):
        return ORDERS[order_id]


def refund_agent(order_id: str, remembered: dict | None = None) -> tuple[str, str]:
    """Answer a refund request; return the reply and the trace ID of the work behind it."""
    with tracer.start_as_current_span("answer-refund") as span:
        # The bug: an order remembered from earlier in the conversation skips the lookup.
        order = remembered or look_up_order(order_id)
        with tracer.start_as_current_span("draft-reply"):
            reply = f"Order {order_id} was delivered on {order['delivered']}, so it qualifies for a refund."
    return reply, format(span.get_span_context().trace_id, "032x")
# ANCHOR_END: agent


# ANCHOR: verify
complete = refund_agent("1042")
skipped = refund_agent("1042", remembered=ORDERS["1042"])
state.flush()  # store the spans before they are checked

cases = [
    ("complete", {"reply": complete[0], "trace": complete[1]}),
    ("skipped", {"reply": skipped[0], "trace": skipped[1]}),
    # The same reply sent without its trace: nothing shows what ran.
    ("no trace", {"reply": complete[0]}),
]
for label, evidence in cases:
    with state.run() as run:
        result = run.observe.verify("refund-workflow", **evidence)
    print(f"{label}: {result.verdict} · {evidence['reply']}")
    for requirement in result.requirements:
        print(f"  {requirement['id']}: {requirement['outcome']}")
state.shutdown()
# ANCHOR_END: verify

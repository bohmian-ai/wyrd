from wyrd import WyrdState, verify

state = WyrdState.from_path(".wyrd-bundle")
state.start_bifrost()

def letter_writer(order_id: str) -> str:  # mock agent call
    return f"# Refund decision\n\nOrder ID: {order_id}\n\nYour $40 refund is approved.\n"

letter = letter_writer("A-1001")

run = state.run()
agent = run.for_card("agent")

# Verify now and wait for the verdict
result = agent.observe.verify("no-refund-promise", letter=letter)
print(result.verdict)  # passed

# Submit the document check to run in the background
execution = agent.observe.submit("names-order", letter=verify.File(letter.encode(), filename="letter.md"))

# Record the letter to the Service's Bifrost table
run.observe.record("vala.datasets.letters", {"order_id": "A-1001", "letter": letter})

state.shutdown()  # flush everything recorded

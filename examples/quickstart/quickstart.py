from wyrd import WyrdState

# Declare the Service and the Verifier it references, and get a ready state.
state = WyrdState.declare("support-reply.yaml")
run = state.run()

for answer in ("Your refund request is approved.", "You have a guaranteed refund."):
    result = run.observe.verify("no-refund-promise", answer=answer)
    print(f"{result.verdict}: {result.requirements[0]['reason']}")
    print(f"  subject:  {result.subject}")
    print(f"  verifier: {result.verifier}")

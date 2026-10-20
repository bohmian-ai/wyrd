from wyrd import Cards, WyrdState

# Declare the Service and the Verifier it references, then pull both into a local bundle.
cards = Cards()
receipt = cards.declare_from_path("support-reply.yaml")
cards.pull(receipt.root, ".wyrd-bundle")

state = WyrdState.from_path(".wyrd-bundle")
run = state.run()

for reply in ({"refund": 40}, {"refund": 250}):
    result = run.observe.verify("refund-limit", reply)
    print(f"{result.verdict}: {result.requirements[0]['reason']}")
    print(f"  subject:  {result.subject}")
    print(f"  verifier: {result.verifier}")

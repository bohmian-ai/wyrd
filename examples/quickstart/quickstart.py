from wyrd import Cards, WyrdState

# Declare both Cards, then pull the Service and its Verifier into a local bundle.
cards = Cards()
receipt = cards.declare_from_path("quickstart.yaml")
cards.pull(receipt.root, ".wyrd-bundle")

state = WyrdState.from_path(".wyrd-bundle")
run = state.run()

# Supplied JSON evidence: one reply within the refund limit, one over it.
results = [run.observe.verify("refund-limit", reply) for reply in ({"refund": 40}, {"refund": 250})]

for result in results:
    [requirement] = result.requirements
    print(f"{result.verdict}: {requirement['reason']}")
    print(f"  subject:  {result.subject}")
    print(f"  verifier: {result.verifier}")

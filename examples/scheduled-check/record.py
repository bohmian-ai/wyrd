import sys

from wyrd import WyrdState

# The checkout Service, pulled at deploy time.
state = WyrdState.from_path(".wyrd-bundle")
state.start_bifrost()  # samples are sent to Bifrost, where the schedule reads them

# Record five checkout requests that each took the given number of milliseconds.
latency = float(sys.argv[1])
with state.run() as run:
    for _ in range(5):
        run.observe.submit("latency-budget", latency=latency)
state.shutdown()  # send the samples before the program exits
print(f"recorded 5 requests at {latency:g} ms")

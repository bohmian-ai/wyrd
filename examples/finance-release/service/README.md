# Compose a Service from Python modules

The finance release split across Python modules: two components, each with its
own Agent and Verifier, composed into one Service.

```text
service/
  main.py
  components/
    report_writer/
      agent.py
      verifier.py
    release_reviewer/
      agent.py
      verifier.py
```

Each `agent.py` exports an Agent definition. Each `verifier.py` exports a
Pydantic evidence model and a Verifier with three requirements: two assertions
and one Agent judge. Judge Agents live with their Verifiers; they are
dependencies, not Service components. Importing a module builds definitions
without registering or running anything.

[main.py](main.py) imports those definitions, binds each Verifier at its Service
component, and declares the Service once with `WyrdState`. No Card names a
version, so the server selects each one. Each component is verified inside its
own `with state.run("component-alias") as run:` context, which selects that
component as the subject.

Model classes define input schemas and model instances supply evidence. The
report Verifier uses `inputs={"report": Report}`; the release Verifier uses
`inputs=Inputs`, where the Pydantic `Inputs` model declares `release: Release`.
The release judge adds typed custom data to the canonical Verdict with
`ResponseFormat.verdict(data=JudgeData)`; the report judge uses plain
`ResponseFormat.verdict()`.

Run `python main.py` from this directory against a configured Wyrd server with
an `openai` gateway deployment. The evidence is synthetic observed output: the
example verifies it rather than invoking the two subject Agents, and the judge
checks call the configured provider. The authorization and destination
booleans are supplied evidence, not access-control checks.

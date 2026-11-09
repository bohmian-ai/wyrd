# Python one-grammar authoring sketches

These files follow [approved revision 6](../../spec.md). The SDK authoring
exports are pending implementation; importing these files is not a runtime
acceptance check. Start with [multi_component.py](multi_component.py).

Constructors take name/version plus YAML-named keyword fields. Fluent
snake_case setters replace a field and return self; add methods append/insert
and return self. Service.add_component takes a Component value.
Component.verified_by binds its subject; Service.verified_by binds the Service.
The earlier single-binding constructor/incremental examples illustrate that
second placement without replacing the two-component primary story.

Verifier.add_input declares evidence; Action.bind_input and Check.bind_input
bind an executable argument to a reference. Input maps accept canonical slots,
plain JSON Schemas, supported Python scalar classes and optional Pydantic
models. No slot has a required field: omitted evidence leaves the checks that
need it inconclusive, and nullability belongs to the schema.

Execution references are literal YAML strings. Check.assert_ uses Python's
necessary reserved-word escape; its kind remains assert. CodeSource.file
captures local source at validation/snapshot preparation, not at the setter.
Artifact.from_path records a source, extract selects existing preparation,
and CardRef.artifact(...).file(path) reuses an exact registered selection.

validate() and declare() report native domain errors; edits are local and
infallible for correctly typed arguments. Adding children captures their current
values. Replacing a field replaces its captured dependencies too. Full-field
replacement deliberately resets the field, while duplicate add attempts remain
errors for validation.

Cards().declare(service) returns the normal receipt. WyrdState.declare(service)
uses the same publication then declaration owners and existing pull/hydrate.
Local Artifacts publish separately first; existing references skip upload.
A graph refused afterward leaves completed Artifacts registered.

The policy PDF exists at fixtures/cards/card_authoring/company-sharing-policy.pdf.
policy_versions.py takes an explicit revised-source path; it does not pretend
a second checked-in policy exists. Evidence is synthetic.

Run the local syntax/evidence check:

```bash
mise exec -- python -B changes/active/card-authoring/examples/python/check_local_examples.py
```

This imports real evidence/check helpers only, not the future SDK surface.
No declaration, upload, model-quality or concurrency claim follows from it.
The language implementation must prove those with real-server journeys.

// Offline Card definitions through the public `verify` namespace: construction,
// edits, value capture, schema conveniences and every native family. Nothing
// here reaches a server; validation is the shared Rust validation.
import { fileURLToPath } from "node:url";

import { expect, test } from "vitest";

import { type CardTypes, verify, WyrdError } from "wyrd-ai";

const {
  Action,
  Agent,
  Artifact,
  ArtifactFile,
  Audit,
  Binding,
  CardRef,
  Check,
  CodeSource,
  Component,
  Data,
  Experiment,
  InputSlot,
  Mcp,
  Model,
  Operator,
  Prompt,
  Requirement,
  Service,
  Step,
  Trigger,
  Verifier,
} = verify;
type Agent = verify.Agent;
type AuthoredCard = verify.AuthoredCard;
type CardRef = verify.CardRef;
type Data = verify.Data;
type Step = verify.Step;

const POLICY = fileURLToPath(new URL("../../../../../fixtures/cards/card_authoring/", import.meta.url));
const SHARING_POLICY = `${POLICY}company-sharing-policy.pdf`;
const STRING = { type: "string" } as const;

function ref(kind: CardTypes.CardKind, name: string, version = "1.0.0"): CardTypes.CardRef {
  return { kind, name, version };
}

function judge(name = "judge"): Agent {
  const prompt = Prompt.openaiChat("gpt-4o", { messages: "Judge {{report}}", responseFormat: { type: "verdict" } });
  return new Agent(name, {
    version: "1.0.0",
    prompt,
    execution_route: { kind: "wyrd_gateway", deployment: "openai" },
    tool_names: ["fs.read"],
  });
}

function checked(id: string, dependsOn: readonly string[] = []): Step {
  const step = new Step(id, Action.code(CodeSource.inline("def f(**_):\n    return {}\n"), "f"));
  for (const prerequisite of dependsOn) {
    step.addDependsOn(prerequisite);
  }
  return step.addRequirement(
    new Requirement(`${id}-ok`, "Fine", Check.assert(`steps.${id}.output.ok`, "equals", true)),
  );
}

function refusal(card: AuthoredCard): WyrdError {
  try {
    card.validate();
  } catch (error) {
    expect(error).toBeInstanceOf(WyrdError);
    return error as WyrdError;
  }
  throw new Error("validation passed");
}

function invalid(card: AuthoredCard): string {
  const error = refusal(card);
  expect(error.code).toBe("WYRD_LOADER_400_INVALID_ENVELOPE");
  const { diagnostics } = error.details as { readonly diagnostics: readonly { readonly message: string }[] };
  return diagnostics.map(({ message }) => message).join(" ");
}

function spec(card: AuthoredCard): Record<string, unknown> {
  return card.spec() as Record<string, unknown>;
}

test("definitions_edit_offline", () => {
  const verifier = new Verifier("report-policy", { version: "1.0.0", inputs: { report: STRING } });
  expect(verifier.addInput("purpose", STRING)).toBe(verifier);
  expect(verifier.description("Report policy")).toBe(verifier);

  const step = checked("scan");
  verifier.addStep(step);
  step.addDependsOn("later").timeoutSeconds(5);
  expect((spec(verifier).steps as unknown[])[0], "a child is captured as it was when passed").toEqual({
    id: "scan",
    action: { kind: "code", source: { inline: "def f(**_):\n    return {}\n" }, entrypoint: "f" },
    requirements: [
      {
        id: "scan-ok",
        statement: "Fine",
        check: { kind: "assert", path: "steps.scan.output.ok", operator: "equals", expected: true },
      },
    ],
  });

  const component = new Component("reviewer", judge("reviewer")).verifiedBy([verifier]);
  const service = new Service("finance-release", { version: "1.0.0" }).addComponent(component);
  verifier.description("edited after capture");
  expect(spec(service).components).toEqual([
    {
      alias: "reviewer",
      ref: { sibling: ref("Agent", "reviewer") },
      verified_by: [{ verifier: { sibling: ref("Verifier", "report-policy") } }],
    },
  ]);
  service.validate();

  const forward = new Verifier("forward", { version: "1.0.0", inputs: { report: STRING } });
  forward.addStep(new Step("first", Action.code(CodeSource.inline("x"), "f").inputs({ later: "steps.second.output" })));
  forward.addRequirement(new Requirement("done", "Done", Check.assert("steps.first.output.ok", "equals", true)));
  expect(invalid(forward), "a forward reference is refused only at validation").toContain("steps.second");
  forward.addStep(checked("second"));
  forward.validate();

  const ordered = new Verifier("ordered", { version: "1.0.0", inputs: { report: STRING } });
  ordered.steps([checked("a"), checked("b"), checked("c", ["a", "b"])]);
  expect(ordered.plan()).toEqual({
    stages: [
      { index: 0, tasks: ["a", "b"] },
      { index: 1, tasks: ["c"] },
    ],
    task_count: 3,
  });

  const duplicated = new Verifier("duplicated", { version: "1.0.0" });
  duplicated.addInput("report", STRING).addInput("report", { type: "integer" });
  duplicated.addStep(checked("scan")).addStep(checked("scan"));
  expect(invalid(duplicated)).toContain("inputs.report: duplicate entry");
  expect(spec(duplicated).inputs).toEqual({ report: { type: "json", schema: STRING } });
  expect((spec(duplicated).steps as { readonly id: string }[]).map(({ id }) => id)).toEqual(["scan", "scan"]);

  const replaced = new Verifier("duplicated", { version: "1.0.0", inputs: { report: STRING } });
  replaced.inputs({ purpose: STRING });
  expect(Object.keys(spec(replaced).inputs as object), "a whole-field setter replaces earlier additions").toEqual([
    "purpose",
  ]);

  // @ts-expect-error an unknown option is a type error, and refused when untyped
  expect(() => new Verifier("typo", { version: "1.0.0", stepz: [] })).toThrow("Verifier has no field `stepz`");
  // @ts-expect-error a list field takes a list
  expect(() => new Agent("listed", { version: "1.0.0" }).toolNames("fs.read")).toThrow("tool_names takes a list");
  expect("name" in new Service("fixed", { version: "1.0.0" }), "name is constructor-only").toBe(false);
  expect("id" in new Step("fixed", Action.extract("inputs.d"))).toBe(false);
  expect(() => new Agent("listed", { version: "1.0.0" }).toolNames(["fs.read", 1n] as unknown as string[])).toThrow(
    WyrdError,
  );
});

test("schemas_and_slots_are_canonical", () => {
  const schema = { type: "object", properties: { text: { type: "string" } } };
  const verifier = new Verifier("plain", { version: "1.0.0", inputs: { report: schema } });
  verifier.addInput("notes", { type: ["string", "null"] });
  verifier.addInput("canonical", { type: "json", schema: STRING });
  verifier.addInput("open", true).addInput("closed", false);
  verifier.addInput("explicit_open", InputSlot.json(true)).addInput("explicit_closed", InputSlot.json(false));
  schema.properties.text.type = "integer";
  verifier.addRequirement(new Requirement("has-text", "Has text", Check.assert("inputs.report", "is_non_empty", null)));
  verifier.validate();
  expect(spec(verifier).inputs).toEqual({
    report: { type: "json", schema: { type: "object", properties: { text: STRING } } },
    notes: { type: "json", schema: { type: ["string", "null"] } },
    canonical: { type: "json", schema: STRING },
    open: { type: "json", schema: true },
    closed: { type: "json", schema: false },
    explicit_open: { type: "json", schema: true },
    explicit_closed: { type: "json", schema: false },
  });

  const slots: readonly Readonly<Record<string, CardTypes.JsonValue>>[] = [
    { type: "json", schema: STRING, required: false },
    { type: "trace", required: true },
  ];
  for (const slot of slots) {
    const optional = new Verifier("optional", { version: "1.0.0", inputs: { note: slot } });
    const message = invalid(optional);
    expect(message).toContain("inputs.note");
    expect(message).toContain("unknown field `required`");
  }
  expect("required" in InputSlot).toBe(false);
  expect("addInput" in Action.extract("inputs.d")).toBe(false);
});

test("every_native_family_constructs_canonical_specs", () => {
  const pager = { kind: "http", method: "post", url: "https://hooks.example.com/release" } as const;
  const singles: readonly (readonly [AuthoredCard, string])[] = [
    [
      new Data("authored", { version: "1.0.0" })
        .interface({ kind: "Parquet", meta: { compression: "Snappy" } })
        .schema({ columns: [{ name: "latency", dtype: "float64" }] })
        .stats({
          row_count: 1,
          col_count: 1,
          byte_count: 1,
          sha256: "c4f7700b380aff2662e88d034154d1890c33d3c12c8dda1a0ca9481f43bb9906",
        }),
      "Data",
    ],
    [
      new Model("authored", {
        version: "1.0.0",
        interface: {
          kind: "Custom",
          meta: { framework_version: "0.1.0", loader_module: "assistant", loader_class: "LatencyModel", extra: {} },
        },
        task_type: "Other",
        signature: {
          inputs: [{ name: "latency", dtype: "float64" }],
          outputs: [{ name: "score", dtype: "float64" }],
        },
      }),
      "Model",
    ],
    [
      new Experiment("authored", { version: "1.0.0", experiment_type: "ab" }).addTargetRef(
        CardRef.model("ranker", "1.0.0"),
      ),
      "Experiment",
    ],
    [new Mcp("authored", { version: "1.0.0", server_name: "files", transport: "stdio" }).addScope("read"), "Mcp"],
    [new Audit("authored", { version: "1.0.0", description: "Quarterly review" }), "Audit"],
    [new Trigger("authored", { version: "1.0.0", kind: "schedule", cron: "0 0 * * *" }), "Trigger"],
    [new Operator("authored", { version: "1.0.0" }).action(pager), "Operator"],
    [
      new Artifact("authored", { version: "1.0.0", artifact_kind: "document" })
        .addMetadata("owner", { type: "str", value: "finance" })
        .addLabel("team", "finance")
        .addAnnotation("reviewed", "yes"),
      "Artifact",
    ],
    [judge(), "Agent"],
  ];
  for (const [card, kind] of singles) {
    expect(card.cardRef()?.kind).toBe(kind);
    card.validate();
  }
  const kinds = ["Data", "Model", "Artifact", "Experiment", "Agent", "Mcp", "Service", "Audit", "Verifier"];
  for (const kind of [...kinds, "Trigger", "Operator"]) {
    expect(verify, `${kind} is authorable by name`).toHaveProperty(kind);
  }
  expect("CardDefinition" in verify, "no generic constructor admits another kind").toBe(false);
  expect(singles[3]?.[0].spec()).toEqual({ server_name: "files", transport: "stdio", scopes: ["read"] });
  expect(singles[6]?.[0].spec(), "an Operator's action is flattened into its spec").toEqual(pager);
  expect(singles[2]?.[0].spec()).toMatchObject({ target_refs: [ref("Model", "ranker")] });

  const policy = CardRef.artifact("company-sharing-policy", "1.0.0").file("extracted/policy.json");
  const local = Artifact.fromPath("company-sharing-policy", SHARING_POLICY, { version: "2.0.0" }).extract(true);
  expect(policy).toBeInstanceOf(ArtifactFile);
  local.validate();

  const call = Action.agent(CardRef.agent("judge", "1.0.0"))
    .bindInput("report", "inputs.report")
    .addWorkspace("/inputs/report.json", "inputs.report")
    .addWorkspace("/inputs/policy.json", policy);
  const checkCall = Check.agent(judge())
    .bindInput("report", "inputs.report")
    .workspace({ "/inputs/policy.json": local.file("x.pdf") });
  const operators = [
    "equals",
    { kind: "in_range", min: 0.0, max: 1.0, inclusive: true },
    { kind: "matches_regex", pattern: "^r" },
  ] as const;
  const checks = [
    Check.code(CodeSource.inline("def f():\n    return {}\n"), "f", { response_format: "verdict" }),
    checkCall,
    ...operators.flatMap((operator) => [
      Check.assert("inputs.report", operator, 1),
      Check.each("inputs.report.items[*]", operator, 1),
      Check.trace("inputs.trace", "$.spans[0].name", operator, 1),
    ]),
  ];
  const verifier = new Verifier("inventory", {
    version: "1.0.0",
    inputs: {
      report: InputSlot.json({ type: "object" }),
      document: InputSlot.media(["pdf"], { max_bytes: 1024 }),
      trace: InputSlot.trace(),
    },
    steps: [
      new Step("extract", Action.extract("inputs.document")),
      new Step("judge", call, { when: { path: "steps.extract.status", operator: "equals", expected: "succeeded" } }),
      new Step("summarize", Action.code(policy, "summarize").runtime("python")),
      { id: "canonical", action: { kind: "extract", from: "inputs.document" } },
    ],
    requirements: checks.map((check, index) => new Requirement(`r${index}`, "Compared", check)),
    decide: { pass_rate: { threshold: 0.5 } },
    context_capture: "hash",
  });
  type Spec = {
    readonly steps: readonly { readonly action: Record<string, Record<string, unknown>> }[];
    readonly requirements: readonly { readonly check: Record<string, unknown> }[];
  };
  const inventory = verifier.spec() as Spec;
  expect(inventory.steps[1]?.action.workspace?.["/inputs/policy.json"]).toEqual({
    artifact: { card: ref("Artifact", "company-sharing-policy"), path: "extracted/policy.json" },
  });
  expect(inventory.steps[1]?.action.inputs).toEqual({ report: "inputs.report" });
  expect(inventory.requirements[1]?.check.inputs).toEqual({ report: "inputs.report" });
  expect(inventory.requirements[1]?.check.agent).toEqual({ sibling: ref("Agent", "judge") });
  expect(
    inventory.requirements[1]?.check.workspace,
    "a local Artifact stands as a sibling until it is published at declaration",
  ).toEqual({
    "/inputs/policy.json": {
      artifact: { card: { sibling: ref("Artifact", "company-sharing-policy", "2.0.0") }, path: "x.pdf" },
    },
  });
  expect(inventory.requirements[3]?.check).toEqual({
    kind: "assert",
    each: "inputs.report.items[*]",
    operator: "equals",
    expected: 1,
  });
  expect(inventory.steps[3]).toEqual({ id: "canonical", action: { kind: "extract", from: "inputs.document" } });
  verifier.validate();

  const group = new Verifier("conversation", {
    version: "1.0.0",
    inputs: {
      turns: InputSlot.source("verifiers.turn-check.inputs.turn"),
      answered: "verifiers.turn-check.requirements.answered.outcome",
    },
    order_by: "inputs.turns.turn",
    requirements: [new Requirement("answered", "Answered", Check.each("inputs.answered[*]", "equals", "passed"))],
  });
  expect((spec(group).inputs as Record<string, unknown>).turns).toBe("verifiers.turn-check.inputs.turn");
  group.validate();

  const driftSpec = {
    method: "Spc",
    signal: { kind: "Distribution", baseline_ref: ref("Data", "latency-baseline"), features: ["latency"] },
    condition: { kind: "Statistical" },
    profile: { kind: "Spc", sample_size: 5 },
  } as const;
  const drift = new Verifier("latency-drift", {
    version: "1.0.0",
    inputs: { latency: InputSlot.samples(["latency"]) },
  });
  drift.addRequirement(new Requirement("stable", "Stable", Check.drift("inputs.latency", driftSpec)));
  expect((spec(drift).requirements as { readonly check: unknown }[])[0]?.check).toEqual({
    kind: "drift",
    on: "inputs.latency",
    ...driftSpec,
  });
  drift.validate();
  const baseline = new Data("latency-baseline", { version: "1.0.0" })
    .interface({ kind: "Parquet", meta: { compression: "Snappy" } })
    .schema({ columns: [{ name: "latency", dtype: "float64" }] })
    .stats({
      row_count: 1,
      col_count: 1,
      byte_count: 1,
      sha256: "c4f7700b380aff2662e88d034154d1890c33d3c12c8dda1a0ca9481f43bb9906",
    });
  const baselineAt = (baselineRef: CardRef | Data): unknown => {
    const check = Check.drift("inputs.latency", {
      ...driftSpec,
      signal: { ...driftSpec.signal, baseline_ref: baselineRef },
    });
    const card = new Verifier("latency-drift", {
      version: "1.0.0",
      inputs: { latency: InputSlot.samples(["latency"]) },
    });
    card.addRequirement(new Requirement("stable", "Stable", check));
    card.validate();
    return (spec(card).requirements as { readonly check: { readonly signal: unknown } }[])[0]?.check.signal;
  };
  expect(baselineAt(CardRef.data("latency-baseline", "1.0.0")), "an exact CardRef stays external").toEqual(
    driftSpec.signal,
  );
  expect(baselineAt(baseline), "an authored Data Card is a sibling").toEqual({
    ...driftSpec.signal,
    baseline_ref: { sibling: ref("Data", "latency-baseline") },
  });

  const binding = new Binding(new Verifier("checked", { version: "1.0.0" }), {
    runs_on: new Trigger("daily", { version: "1.0.0", kind: "schedule", cron: "0 0 * * *" }),
  });
  binding.addOnFailure(new Operator("page", { version: "1.0.0", action: pager })).addOnFailure(pager);
  binding.addOnVerdictChange(CardRef.operator("notify", "1.0.0"));
  const bound = judge("bound").addVerifiedBy(binding);
  expect(spec(bound).verified_by).toEqual([
    {
      verifier: { sibling: ref("Verifier", "checked") },
      runs_on: { sibling: ref("Trigger", "daily") },
      on_failure: [{ sibling: ref("Operator", "page") }, pager],
      on_verdict_change: [ref("Operator", "notify")],
    },
  ]);

  const registered = new CardRef("Agent", "judge", "1.0.0");
  expect([registered.kind, registered.name, registered.version]).toEqual(["Agent", "judge", "1.0.0"]);
  const component = new Component("judge", registered, { source: { path: "services/judge", kind: "python" } })
    .addConfig("region", { type: "str", value: "eu" })
    .addVerifiedBy(CardRef.verifier("checked", "1.0.0"));
  const service = new Service("authored", {
    version: "1.0.0",
    description: "Release service",
    components: [component, { alias: "raw", ref: ref("Agent", "raw") }],
    verified_by: ["./not-a-file.yaml"],
  }).verifiedBy([CardRef.verifier("release", "1.0.0")]);
  expect(service.spec()).toEqual({
    description: "Release service",
    components: [
      {
        alias: "judge",
        ref: ref("Agent", "judge"),
        source: { path: "services/judge", kind: "python" },
        config: { region: { type: "str", value: "eu" } },
        verified_by: [{ verifier: ref("Verifier", "checked") }],
      },
      { alias: "raw", ref: ref("Agent", "raw") },
    ],
    verified_by: [{ verifier: ref("Verifier", "release") }],
  });
  service.validate();

  const pending = new Verifier("pending", { version: "1.0.0", inputs: { report: STRING } });
  pending.addRequirement(new Requirement("coded", "Coded", Check.code(CodeSource.file(`${POLICY}missing.py`), "f")));
  expect(
    (spec(pending).requirements as { readonly check: { readonly source: unknown } }[])[0]?.check.source,
    "a code file is read only at validation",
  ).toBeNull();
  expect(JSON.stringify(refusal(pending).details)).toContain("missing.py");
});

/** The `WyrdError` `build` throws. */
function thrown(build: () => unknown): WyrdError {
  try {
    build();
  } catch (error) {
    expect(error).toBeInstanceOf(WyrdError);
    return error as WyrdError;
  }
  throw new Error("expected a WyrdError");
}

test("prompts_project_every_native_builder_capability", () => {
  const route = { kind: "wyrd_gateway", deployment: "anthropic" } as const;
  const base = Prompt.anthropic("claude-sonnet-5", {
    system: "Be brief",
    messages: "Summarise {{report}}",
    output: { type: "json_schema", name: "summary", schema: { type: "object" } },
    settings: { max_tokens: 256 },
  });
  const prompt = base
    .user([{ type: "image", source: { type: "url", url: "https://example.com/chart.png" } }])
    .user(`Compare \${media:scan}`)
    .assistant("Noted.")
    .toolResult("call-1", "42");
  expect(base.render({ report: "Q3" }), "an operation leaves its Prompt unchanged").toEqual({
    provider: "anthropic_message",
    body: {
      model: "claude-sonnet-5",
      system: "Be brief",
      max_tokens: 256,
      messages: [{ role: "user", content: [{ type: "text", text: "Summarise Q3" }] }],
      output_config: { format: { type: "json_schema", schema: { type: "object" } } },
    },
  });
  const writer = new Agent("writer", { prompt, execution_route: route });
  expect(spec(writer).prompt).toMatchObject({
    provider: "anthropic",
    model: "claude-sonnet-5",
    format: "messages",
    body: {
      system: "Be brief",
      max_tokens: 256,
      messages: [
        { role: "user", content: [{ type: "text", text: "Summarise {{report}}" }] },
        { role: "user", content: [{ type: "image", source: { type: "url", url: "https://example.com/chart.png" } }] },
        {
          role: "user",
          content: [
            { type: "text", text: "Compare " },
            { type: "text", text: `\${media:scan}` },
          ],
        },
        { role: "assistant", content: [{ type: "text", text: "Noted." }] },
        { role: "user", content: [{ type: "tool_result", tool_use_id: "call-1" }] },
      ],
    },
  });
  writer.validate();

  const bound = prompt
    .bind({ report: "Q3" })
    .bindMedia("scan", { kind: "image", source: { type: "base64", mime_type: "image/png", data: "QUJD" } });
  const rendered = bound.render() as { readonly body: { readonly messages: readonly unknown[] } };
  expect(rendered.body.messages[0]).toEqual({ role: "user", content: [{ type: "text", text: "Summarise Q3" }] });
  expect(JSON.stringify(rendered.body.messages[2]), "the media binds as a native image block").toContain("QUJD");
  expect(Prompt.openaiResponses("gpt-4o", { system: "Plan", messages: "Go" }).render()).toMatchObject({
    body: { instructions: "Plan" },
  });
  expect(Prompt.gemini("gemini-2.5-pro", { messages: "Go" }).render()).toMatchObject({
    body: { contents: [{ role: "user", parts: [{ text: "Go" }] }] },
  });
  expect(
    spec(new Agent("v", { prompt: Prompt.vertex("gemini-2.5-pro"), execution_route: route })).prompt,
  ).toMatchObject({
    provider: "vertex",
  });

  const cyclic: Record<string, unknown> = {};
  cyclic.self = cyclic;
  expect(thrown(() => Prompt.openaiChat("gpt-4o", { settings: cyclic as never })).code).toBe(
    "WYRD_SPEC_400_VALIDATION",
  );
  expect(thrown(() => Prompt.openaiChat("gpt-4o", { settings: { temperature: "hot" } })).code).toBe(
    "WYRD_PROMPT_400_INVALID",
  );
  expect(
    thrown(() => Prompt.openaiChat("gpt-4o", { responseFormat: { type: "verdict", data: 1 as never } })).code,
  ).toBe("WYRD_PROMPT_400_INVALID");
  expect(thrown(() => Prompt.anthropic("claude-sonnet-5", { promptCacheKey: "k" } as never)).code).toBe(
    "WYRD_PROMPT_400_INVALID",
  );
  expect(thrown(() => prompt.user({ type: "bogus" })).code).toBe("WYRD_PROMPT_400_INVALID");
  expect(thrown(() => prompt.bind({ report: 1 as never })).code).toBe("WYRD_PROMPT_400_INVALID");
  expect(thrown(() => base.render()).code).toMatch(/^WYRD_/);

  for (const role of ["user", "assistant"] as const) {
    const rich = Prompt.anthropic("claude-sonnet-5", { messages: "Hi", settings: { max_tokens: 64 } })[role]([
      { type: "text", text: "Hi {{name}}" },
    ]);
    expect(thrown(() => rich.render()).code, `a ${role} text part requires its variable`).toBe(
      "WYRD_PROMPT_422_MISSING_VARIABLE",
    );
    const expected = { role, content: [{ type: "text", text: "Hi Ada" }] };
    const supplied = rich.render({ name: "Ada" }) as { readonly body: { readonly messages: readonly unknown[] } };
    expect(supplied.body.messages[1]).toEqual(expected);
    const boundRich = rich.bind({ name: "Ada" }).render() as {
      readonly body: { readonly messages: readonly unknown[] };
    };
    expect(boundRich.body.messages[1]).toEqual(expected);
  }

  const edited = Prompt.anthropic("claude-sonnet-5", { messages: "Hello {{value}}", settings: { max_tokens: 64 } })
    .bind({ value: "literal {{other}}" })
    .user([{ type: "text", text: "extra" }])
    .render() as { readonly body: { readonly messages: readonly { readonly content: unknown }[] } };
  expect(edited.body.messages.map((message) => message.content)).toEqual([
    [{ type: "text", text: "Hello literal {{other}}" }],
    [{ type: "text", text: "extra" }],
  ]);
});

test("versions_are_optional_and_prompts_are_native", () => {
  const unversioned = new Agent("writer");
  expect(unversioned.cardRef(), "registration assigns an omitted version").toBeUndefined();
  const local = Artifact.fromPath("company-sharing-policy", SHARING_POLICY);
  expect(local.cardRef()).toBeUndefined();
  const service = new Service("release", { components: [new Component("writer", unversioned)] });
  expect(spec(service).components, "a sibling without a version names no version").toEqual([
    { alias: "writer", ref: { sibling: { kind: "Agent", name: "writer" } } },
  ]);
  const step = new Step("summarize", Action.code(local.file("x.py"), "f"));
  expect((spec(new Verifier("v").addStep(step)).steps as { readonly action: unknown }[])[0]?.action).toEqual({
    kind: "code",
    source: { artifact: { card: { sibling: { kind: "Artifact", name: "company-sharing-policy" } }, path: "x.py" } },
    entrypoint: "f",
  });
  expect(new Service("pinned", { version: "2.0.0" }).cardRef()?.version).toBe("2.0.0");

  const prompt = Prompt.openaiChat("gpt-4o", {
    system: "Be brief",
    messages: ["Summarise {{report}}", "Then list risks"],
    responseFormat: { type: "json_schema", name: "summary", schema: { type: "object" } },
    promptCacheKey: "summary",
    settings: { temperature: 0 },
  });
  expect(prompt, "a Prompt is a value, not a Card").not.toBeInstanceOf(verify.AuthoredCard);
  const writer = new Agent("writer", { prompt, execution_route: { kind: "wyrd_gateway", deployment: "openai" } });
  expect(spec(writer).prompt, "an Agent holds its Prompt in the native form").toEqual({
    provider: "openai",
    model: "gpt-4o",
    format: "chat_completion",
    body: {
      prompt_cache_key: "summary",
      messages: [
        { role: "system", content: "Be brief" },
        { role: "user", content: "Summarise {{report}}" },
        { role: "user", content: "Then list risks" },
      ],
      response_format: {
        type: "json_schema",
        json_schema: { name: "summary", schema: { type: "object" }, strict: true },
      },
      temperature: 0,
    },
  });
  writer.validate();

  expect(spec(judge()).prompt, "the verdict format is the canonical Verdict").toEqual({
    provider: "openai",
    model: "gpt-4o",
    format: "chat_completion",
    body: { messages: [{ role: "user", content: "Judge {{report}}" }] },
    response_format: "verdict",
  });

  let refused: unknown;
  try {
    Prompt.openaiChat("");
  } catch (error) {
    refused = error;
  }
  expect(refused).toBeInstanceOf(WyrdError);
  expect((refused as WyrdError).code).toBe("WYRD_PROMPT_400_EMPTY_MODEL");
});

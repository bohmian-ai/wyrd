---
title: Run your first verification
description: Declare a Verifier and the Service it checks in two YAML files, then judge a passing and a failing reply from one Python, Rust, or TypeScript program.
pillar: wyrd
section: Get started
order: 3
status: runnable
---

You write two small YAML files and one short program in Python, Rust, or TypeScript. The program declares them to your server, then asks Wyrd to judge two support replies against one rule: a refund is at most 100 dollars. One reply passes and one fails, and each verdict names the exact versions that produced it.

No model, provider key, or application is involved. You supply the reply as JSON, so this shows Wyrd judging declared criteria, not measuring a system that ran.

## Prerequisites

- The server from [Set up Wyrd](/get-started/setup/), running in its own terminal.
- The `quickstart` directory from setup, with the SDK for your language installed. Run everything below there, in a second terminal.

## 1. Declare the criteria

Save the Verifier in your `quickstart` directory as `refund-limit.yaml`. Its one requirement checks the `refund` field of a JSON input named `reply`:

<CodeFromFile file="examples/quickstart/refund-limit.yaml" lang="yaml" title="refund-limit.yaml" />

Save the subject beside it as `support-reply.yaml`. It is a Service, and `verified_by` binds the Verifier to it by relative path:

<CodeFromFile file="examples/quickstart/support-reply.yaml" lang="yaml" title="support-reply.yaml" />

Each file is one Card, named and versioned in its `metadata`. A Card's name and version always mean the same content, which is what lets a verdict point at exactly what was checked.

## 2. Verify two replies

Save the program and run it from the `quickstart` directory, so it finds both YAML files there:

<LangTabs langs={['python', 'rust', 'typescript']}>
<LangTab lang="python">

<CodeFromFile file="examples/quickstart/quickstart.py" lang="python" title="quickstart.py" />

```bash
python quickstart.py
```

</LangTab>
<LangTab lang="rust">

<CodeFromFile file="examples/quickstart/quickstart.rs" lang="rust" title="src/main.rs" />

```bash
cargo run
```

</LangTab>
<LangTab lang="typescript">

<CodeFromFile file="examples/quickstart/quickstart.ts" lang="typescript" title="quickstart.ts" />

```bash
node quickstart.ts
```

</LangTab>
</LangTabs>

`WyrdState.declare` reads `support-reply.yaml` and the Verifier it references, declares both Cards to the server, and returns a state ready to use. `verify` sends one reply as the Verifier's `reply` input and waits for the verdict. Every language prints the same:

```text
passed: the comparison held
  subject:  default/Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/refund-limit@1.0.0#<uid>
failed: operator LessThanOrEquals returned false
  subject:  default/Service/support-reply@1.0.0#<uid>
  verifier: default/Verifier/refund-limit@1.0.0#<uid>
```

Each verdict names the subject and the Verifier by space, kind, name, version, and the server-assigned `uid` of that exact declaration. The server assigns each `uid` when the Card is first declared, so yours differ from these.

The second verdict is `failed`, and it is still a successful call: the check ran, and 250 is more than 100. The result holds one entry per requirement with its `id`, `outcome`, `reason`, and the `actual` value it judged.

Run the program again. Declaring unchanged Cards changes nothing, so you get the same verdicts with the same identities. It leaves no files behind.

## 3. Change the rule

Edit `refund-limit.yaml` to `expected: 50` and run the program again. It stops with `WYRD_REGISTRY_409_SPEC_DRIFT`: `refund-limit@1.0.0` already means the declaration with 100, and a version never changes meaning. To change the rule, give it a new version: set `version: 1.1.0` in `refund-limit.yaml`. The Service now references different content too, so set its `version: 1.1.0` as well. Run again, and the verdicts name `support-reply@1.1.0` and `refund-limit@1.1.0`.

## Errors are not verdicts

A refused request fails with a `WyrdError` carrying a stable `code`, raised in Python and TypeScript and returned as `Err` in Rust, and produces no verdict. For example, a credential with only the `viewer` role cannot declare Cards or run a Verifier, and either call fails with `WYRD_PERMISSION_403_DENIED_RBAC`.

| You see | Cause | Fix |
| --- | --- | --- |
| `transport unavailable` | The server is not running | Start it as in [Set up Wyrd](/get-started/setup/#start-the-server) |
| `WYRD_CLIENT_401_NO_CREDENTIALS` | This terminal sees a different config directory | Use the same `WYRD_CONFIG_HOME` as the server terminal |
| `WYRD_REGISTRY_409_SPEC_DRIFT` | A file changed but its version did not | Bump `metadata.version`, or revert the change |
| `WYRD_LOADER_400_INVALID_ENVELOPE` | A file is missing or not a valid Card | Run from the directory that holds both YAML files |
| `WYRD_PERMISSION_403_DENIED_RBAC` | The credential lacks permission to declare or verify | Use the tenant administrator key that setup saved |

The verdicts are also recorded on the server, written in batches, so they can take a moment to appear in queries. The result your code receives is complete when `verify` returns.

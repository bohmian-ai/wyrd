# Docs site writing rules

Rules for every page under `src/content/docs/`. They apply to humans and
agents. Frontmatter, generated pages, and site commands are in
[CONTRIBUTING.md](CONTRIBUTING.md); audience and product terms are in
[PRODUCT.md](PRODUCT.md).

## Page types

Every page is one of four types. Pick it before writing.

**Landing** (the introduction)

- What Wyrd is in one paragraph, one tested example, and where to start.
- No procedure and no exhaustive lists.

**Guide** (Get started, Build and verify, Connect, Operate)

- Assumes no knowledge of Wyrd. May assume the reader knows their language,
  a terminal, and PostgreSQL.
- Second person, imperative: "Save the file", "Run the program".
- Covers only the behavior needed to finish the task. Does not enumerate
  options, flags, or every case.
- Shows how the reader knows each important step worked.
- Runs project code and the CLI through the reader's tool: `uv run`,
  `cargo run`, `npx`. Never asks the reader to activate an environment.
- Ends with a call to action under `## Next`.

**Concept** (Understand)

- Explains how something behaves and why, in detail.
- Third person. Use "you" only for a reader action the behavior depends on;
  no imperative steps or imperative headings.
- Opens with two sentences before any heading: what it is, then why it
  matters.
- Links to reference inline, where an exhaustive list would otherwise go.

**Reference** (Reference, API)

- Enumerates every field, option, limit, and error.
- Third person, neutral. Generated from source where a generator exists.
- One table or section per contract. At most one example block per
  contract; a nested wire shape may use one `typescript` type block instead
  of a table. Prose states only facts a table cannot hold.
- Failures go under `## Errors` as a table of condition and result.

## What a guide leaves out

- **Implementation detail.** Include it only when it changes a decision the
  reader makes or explains something they see. Config precedence, credential
  lookup order, file layouts, internal services, and environment variables the
  reader never types belong in reference.
- **Edge cases and alternatives.** Put them in an `<Aside>` with a link, or on
  a concept or reference page.
- **Failures from other tasks.** End a guide with `## If it fails`: a two-column
  table (`You see` | `Fix`) of the failures this page's own steps commonly
  produce, at most about four rows. Everything else goes to troubleshooting.
  An error a step triggers on purpose, to teach something, stays in that step;
  show its printed output only when a test captures that text. `## If it
  fails` is for guides only.
- **Terms the page does not need.** Introduce a Wyrd term only at the step
  that uses it, in one sentence. Use the term from `PRODUCT.md` and never a
  synonym.

## Page shapes

Copy the named page when writing a new one of the same shape.

| Shape | Sections | Copy |
| --- | --- | --- |
| Landing | One paragraph → one example (its YAML, the program, and its output) → how it fits → what Wyrd does not do → `## Next` | `get-started/introduction.md` |
| Install | What you need (minimum versions table) → install, one command per language in `<LangTabs>` → start or check → `## If it fails` → `## Next` | `get-started/setup.md` |
| Quickstart or guide | One-sentence goal → numbered `##` steps, each with its files, command, and output → `## If it fails` → `## Next` | `get-started/quickstart.md` |
| Concept | What it is (two sentences) → one example → behavior → links | `understand/how-verification-works.md` |
| Reference | One table or section per contract; no narrative | `reference/verifier-card.md` |

## Examples

- Embed code with `<CodeFromFile file="examples/...">` from a file a journey
  test runs. Do not hand-write a snippet a reader will copy. Language tabs
  showing the same scenario count as one example.
- Explain code with short comments in the example file, at the line they
  describe. Prose after a block covers only its output and what to do next.
- Show the output a command prints when it confirms success, in a `text` block
  directly after the command.
- Use `<LangTabs langs={['python', 'rust', 'typescript']}>` for anything that
  differs by language, in that order.
- Pin versions in install commands to the documented release.
- Public pages use the reader's tools (`uv add`, `cargo add`, `npm install`,
  `wyrd ...`), never repository tasks; `scripts/check_commands.py` enforces it.
  Python uses uv, not pip.

## Components

| Need | Use |
| --- | --- |
| Per-language code | `<LangTabs>` / `<LangTab>` |
| Side note, alternative, or link out | `<Aside type="note" title="...">` |
| Something that can lose data or break a deployment | `<Aside type="caution">` or `type="danger"` |
| Non-language alternatives (YAML vs JSON, CLI vs SDK) | `<Tabs>` / `<Tab>` |

Do not invent markup or callout styles.

## Writing

- Lead with what the page does for the reader, in one or two sentences.
- One idea per paragraph. Short paragraphs over long ones.
- State facts plainly. No "simply", "just", "easy", "powerful", "seamless".
- No "we". Name Wyrd or use the imperative.
- Sentence-case headings. Every code block has a language.
- Use backticks for commands, code, file paths, packages, and values.
- Do not claim adoption, performance, or production use without evidence.

## Done when

1. **Deletion test.** Remove each clause mentally. Delete it when the reader
   loses nothing useful; shorten it when the same fact fits in fewer words.
2. **Reader test.** A fresh agent given only the rendered page and the task
   completes it. Fix whatever blocked it with a clearer step or example, and
   delete whatever it says it did not need.
3. **`mise run docs:check`** passes, including the example journeys.

Modeled on uv's `STYLE.md`, pydantic-ai's documentation guidelines, and the
getting-started pages of FastAPI, Pydantic, and MLflow.

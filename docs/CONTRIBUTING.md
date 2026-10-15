# Docs contributing

The docs site is a static SvelteKit app under `docs/`. Pages are `.svx` or `.md` files in `src/content/docs/`. Publish a page only when its procedure or reference matches current Wyrd behavior; the site stays coherent with any subset of pages, including none.

Use repository tasks for normal work:

```sh
mise run docs:dev
mise run docs:generate
mise run docs:check
```

Contributor-only commands are acceptable when they are labelled for maintainers and are not presented as the public path.

```sh contributor
pnpm test
pnpm check
```

## Page frontmatter

Navigation, search, the landing page's section links, and `llms.txt` are all derived from frontmatter. A page appears in them once its file exists.

| Field | Required | Values |
| --- | --- | --- |
| `title` | yes | Page heading, browser title, and sidebar label. Name the reader's goal or the contract. Start the body at `##`; a `#` heading fails the build. |
| `pillar` | yes | `wyrd`, `fathom`, or `shared` |
| `section` | yes | `Get started`, `Build and verify`, `Connect`, `Operate`, `Understand`, or `Reference` |
| `topic` | under `Build and verify` only | `Declare`, `Observe`, or `Verify` |
| `order` | no | Number; lower sorts first within its section or topic |
| `description` | no | One sentence, used in search, metadata, and `llms.txt` |
| `status` | no | `runnable`, `partial`, or `planned`; `planned` pages stay out of navigation |
| `draft` | no | `true` keeps the page out of production builds |

An unknown `section` or `topic` fails the build. The "Last updated" date comes from the last commit that changed the page file, so an uncommitted page shows none; do not set it by hand.

## Generated pages

`mise run docs:generate` rewrites `public/llms.txt` and `public/llms-full.txt` from the content tree, and refreshes generated reference pages in `src/content/docs/api/`. A generated page is never created by the generator; to publish one, add its file (for example `src/content/docs/api/errors.md`) and run the task. `mise run docs:check` fails when generated output is stale or uncommitted.

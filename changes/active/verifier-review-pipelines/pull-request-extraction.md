# Pull-request extraction contract

Revision 16 authoring target. This extends existing extraction; it is not yet
implemented. `extractor: pull_request` automatically runs the public
`bohmian-code-review` ReviewPipeline and acquires complete PR evidence.

```yaml
- id: extract-pr
  action:
    kind: extract
    extractor: pull_request
```

## Identity and acquisition

A verified GitHub trigger supplies the installation, host, numeric repository
ID and PR number. A manual execution request selects repository and PR; the
server resolves the authorized installation. Neither path requires a Verifier
input slot, token, `from` or authored repository descriptor. Missing identity,
foreign tenant, disabled connection or unallowed repository refuses before
source IO. There is no inferred repository fallback.

Capture one PR metadata response and pin target `base_sha`, `head_sha`, head
repository/tree and `merge_base_sha`. The merge base is the common ancestor
used for the PR diff, not necessarily the target branch tip. Verify identities
around acquisition. If commits move, return moved/superseded; do not silently
rebase this accepted execution. Bounded transient retries retain the accepted
identity and deadline. A later generation can acquire the newer PR normally.
Title/body changes after capture do not mutate this metadata snapshot.

The public library facade must accept the pinned identity and return the
complete paired evidence. Reuse its existing pipeline rather than reimplementing
its index/overlay/ranking workflow in Wyrd. Current upstream supplies supporting
context and optional per-file patches, not the complete bundle below; its PR
metadata parser lacks title/body/URL. These are required library additions.
Use existing verified blob/tree reads for changed source and add complete diff
acquisition/reconstruction. Do not bypass the facade with a parallel Wyrd
pipeline. Provider operations remain library-owned; authority, retention and
execution scheduling remain Wyrd-owned.

## Successful result

`steps.extract-pr.output` is the PR metadata JSON below. It is not a file
handle, the diff or an automatically assembled prompt.

The engine persists the extraction result automatically and exposes
`steps.extract-pr.publications.evidence` as one durable directory. The extractor
does not have or call Agent write/publish tools:

```text
pr.json
changes.json
diff.patch
changed/
  before/
  after/
context.json
```

One reviewer mount is sufficient:

```yaml
workspace:
  /inputs/pr: steps.extract-pr.publications.evidence
```

The reviewer explicitly reads `/inputs/pr/pr.json`, `/inputs/pr/diff.patch`,
`/inputs/pr/changes.json`, changed source and `/inputs/pr/context.json` with
existing filesystem tools. Reviewers may read/search only this mounted retained
bundle; no additional repository read/search tools or reviewer-triggered GitHub
source fetches are introduced. Library context selection remains extraction-owned.
Merely binding metadata to a Prompt placeholder would not
read these files. Extraction success commits the entire directory atomically;
there are no independently replaceable diff/context members from other heads.

### Metadata: output and pr.json

`pr.json` contains exactly the metadata returned in `output`:

| Field | Type and meaning |
| --- | --- |
| `repository` | Object: `host`, `id`, `owner`, `name`. Host is the canonical GitHub API base URL, including the approved GHES API base path. `id` is the positive numeric repository identity; owner/name are labels. |
| `number` | Positive PR number in the base repository. |
| `url` | Provider-supplied PR web URL; display metadata, not authority to fetch it. |
| `title` | Captured provider title string. |
| `body` | Captured authored description string or null. No fabricated summary. |
| `state` | `open` or `closed`. |
| `draft` | Boolean. |
| `base_sha` | Full pinned target commit object ID. |
| `merge_base_sha` | Full common-ancestor commit object ID used for the diff/before source. |
| `head_sha` | Full pinned PR-head commit object ID. |
| `head_repository_id` | Positive numeric head repository ID, including forks. |

Unknown/missing provider-required fields, invalid identities and bounded metadata
limits fail acquisition explicitly. An optional generated summary is a separate
Agent action authored by the Verifier; the extractor does not generate one.

### Change inventory: changes.json

This is an ordered JSON array, one entry per changed path, sorted by effective
path (`new_path`, otherwise `old_path`) in UTF-8 byte order. Each entry has:

| Field | Type and meaning |
| --- | --- |
| `status` | `added`, `modified`, `deleted`, `renamed` or `copied`. Provider statuses are normalized here; unsupported statuses fail rather than disappear. |
| `old_path` | Original repository-relative UTF-8 path, or null for an addition. |
| `new_path` | New repository-relative UTF-8 path, or null for a deletion. |
| `before` | Null if that side does not exist; otherwise `{path, blob}`. `path` is the bundle-relative path under `changed/before/`; `blob` is the verified Git object ID at the merge base. |
| `after` | Null if that side does not exist; otherwise `{path, blob}` under `changed/after/` at the head. |
| `binary` | Boolean: the file change cannot be represented as a UTF-8 textual patch. |

Modified entries have both paths (normally equal) and both contents. Renames
and copies retain old/new paths and both available contents. Git file mode
changes remain represented in `diff.patch`; symlinks/gitlinks/submodules or
unsafe paths that cannot meet the ordinary-file bundle contract fail extraction
explicitly rather than following a target or silently skipping evidence.

The bundle preserves exact source bytes and safe paths, including spaces and
Unicode, under the portable [snapshot rules](output-publication.md#workspace-snapshot-rules).
Identities and content digests are verified and stored in the publication
manifest. Zero changed files is a valid empty inventory/diff; extraction itself
never supplies a passed review verdict.

### Diff and changed source

`diff.patch` is a complete UTF-8 Git-style unified patch from the recorded merge
base to head. Include path/status/mode changes and available textual hunks.
Binary changes receive a binary-change marker and full before/after bytes in
the bundle; `changes.json` identifies them explicitly. An Agent unable to assess
binary evidence must report that limitation rather than treating the marker as
proof of safety. No repository code is executed to acquire or review evidence.

Missing/truncated provider patches are never interpreted as no change. Acquire
or reconstruct the complete patch from pinned objects, or fail extraction.
Unobtainable blobs, exhausted pagination, over-limit changed bytes/file count,
invalid paths or unsupported evidence prevent successful bundle commit. Do not
publish a partial bundle as successful extraction. Partial diagnostics remain
inspectible under the execution's ordinary failure contract.

### Supplemental context: context.json

Retain the library `ReviewContext` without changing its schema or field meanings:
`format_version`, `extraction_version`, `identity`, `pull_request`, `base`, `head`,
`tree`, `spans`, `omissions`, `not_analyzed`, `budget`, `coverage`. Spans include
path/blob, one-based inclusive line range, score, names, text and token estimate.
Verify repository/base/head against the accepted identity. Supplemental context
uses the pinned target/head indexing contract; the separately recorded merge
base explains the PR diff. The two bases must not be mislabelled as identical.

The library deliberately selects unchanged supporting declarations. It does
not supply the diff or replace changed-file source. Its context token/file
limits are selection limits; omissions remain valid, visible context results.
Its existing `coverage` describes indexing, not what an Agent reviewed. Wyrd
introduces no review-completeness field named coverage or platform finding type.

## Durability, budgets and failure

One logical extraction commits metadata, source/diff and supplemental context
before any consumer starts. A lost uncommitted response may require another
library call; only one resulting bundle is accepted. All consumers and queued
recovery reuse that accepted content, without GitHub or per-reviewer rebuilding.
Keep ContextId/digest/versions in producer provenance; they are not user inputs
or deterministic cache keys.

Expose effective acquisition, publication, context and model-request limits.
Context tokens do not budget the diff, changed source, Prompt, tool results,
conversation or response. Refuse an oversized model request or let the author
explicitly partition work; never silently truncate. Extraction failure produces
no successful `.output` or evidence mount and cannot become a clean PR verdict.

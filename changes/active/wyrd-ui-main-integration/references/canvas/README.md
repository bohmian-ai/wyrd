# Approved UI canvas — revision 18

Visual reference for spec revision 5 (REQ-021–REQ-031), approved by the caller
on 2026-10-19. The spec is authority; the canvas illustrates it. Values in the
boards are illustrative, not fixtures; the session Verifier and drift scores
are invented because no fixture exists yet.

Read the PNGs to see a board; read the `.dc.html` for exact markup, spacing,
and tokens. `canvas.json` holds board titles, positions, and canvas notes
(open the notes for the decisions behind each area). Boards are 1440px wide,
dark theme only.

| Board | Image | What it shows |
|---|---|---|
| [Main](Main.dc.html) | [png](png/Main.png) | Traces: one row per trace, live with pause, Traces | Spans modes, filters folded |
| [Logs](Logs.dc.html) | [png](png/Logs.png) | Logs — same toolbar and timeline, severity-stacked |
| [Metrics](Metrics.dc.html) | [png](png/Metrics.png) | Metrics — one chart, group-by and legend instead of top-4 |
| [TraceDrawer](TraceDrawer.dc.html) | [png](png/TraceDrawer.png) | Trace drawer: opens over the list, waterfall on top, span details below |
| [Spans](Spans.dc.html) | [png](png/Spans.png) | Spans mode: every matching span, filters open, trace id opens the drawer |
| [TimePicker](TimePicker.dc.html) | [png](png/TimePicker.png) | Time picker, open |
| [Home](Home.dc.html) | [png](png/Home.png) | Home — attention from failed verification, Services health, live Observe summary |
| [Agent](Agent.dc.html) | [png](png/Agent.png) | Agent Card: composition, invoke_agent spans and gateway calls attributed to it, bound Verifiers |
| [Prompt](Prompt.dc.html) | [png](png/Prompt.png) | Prompt Card: the prompt is the page; spec only, no telemetry |
| [Model](Model.dc.html) | [png](png/Model.png) | Model Card: interface, artifacts, attributed telemetry when present, drift |
| [Data](Data.dc.html) | [png](png/Data.png) | Data Card: schema from the spec, spec.stats, drift per column from bound drift checks |
| [Service](Service.dc.html) | [png](png/Service.png) | Service Overview: signals by service_card_uid, broken down by component card_uid |
| [Query](Query.dc.html) | [png](png/Query.png) | Query — catalog, editor, results table joined across the warehouse |
| [QueryChart](QueryChart.dc.html) | [png](png/QueryChart.png) | Query — chart view of the same result |
| [ServiceCharts](ServiceCharts.dc.html) | [png](png/ServiceCharts.png) | Service Charts: built-in verification, drift, gateway and signal charts, plus custom charts from ServiceSpec.charts |
| [Verify](Verify.dc.html) | [png](png/Verify.png) | Verify: Traces-style list of recorded results; a run result open in the drawer on the Verifier graph |
| [VerifySession](VerifySession.dc.html) | [png](png/VerifySession.png) | Verify grouped by session: the session verdict heads its member runs; session result in the drawer |
| [VerifyDrift](VerifyDrift.dc.html) | [png](png/VerifyDrift.png) | Verify: a drift result in the drawer, one row per feature with recorded score, threshold, verdict |
| [Verifier](Verifier.dc.html) | [png](png/Verifier.png) | Verifier Card: the Verifier graph from the spec; results live in Verify |
| [Changes](Changes.dc.html) | [png](png/Changes.png) | Change Requests: a PR-style list with revision and verification state |
| [ChangeDetail](ChangeDetail.dc.html) | [png](png/ChangeDetail.png) | Change Request Overview: the existing GitHub-like mock, Verifiers panel, revisions with their cause |
| [ChangeVerification](ChangeVerification.dc.html) | [png](png/ChangeVerification.png) | Change Request Verification tab: one row per Verifier for the current revision |
| [Repos](Repos.dc.html) | [png](png/Repos.png) | Repositories: webhook health, wyrd.toml, the Verifiers folder |

`archive/` holds the 14 original canvas files preserved byte-for-byte
(hashes in `../preserved-files.json`). They are superseded references, not
design authority.

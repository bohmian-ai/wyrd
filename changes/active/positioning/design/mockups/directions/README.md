# Flow direction variants

Three spatial arrangements of the accepted `../simple/` figure, for choosing a
flow direction. The content is identical in all three: the finance handler,
the three checks, the Gateway model call, and asynchronous Bifrost capture.
The hero above is unchanged and nothing is placed over its background.

Every variant fills the full content width below the hero and is a shallow
band, much wider than tall.

## Preview

Serve the parent `mockups/` directory and open `/directions/` (`#a`, `#b`,
`#c` open a variant directly):

```bash
cd changes/active/positioning/design/mockups
mise exec -- python -m http.server 8000   # then http://localhost:8000/directions/
```

## Variants

| | Layout | Handler → Wyrd route | Drawn size | At 1240px wide |
|---|---|---|---|---|
| **A · Top to bottom** | Handler upper left. The checks sit on a lower level to the right, as three columns in one wide frame. Gateway above the Agent column. Bifrost is a compact rail under the handler. | Out of the verify line, then a step **down** into the top of the checks frame. | 960 × 258 | about 333px tall |
| **B · Left to right** | Handler left, the checks as three rows in a frame to its right, Gateway further right on the Agent row. Bifrost rail along the bottom. | **Right**, from the verify line. | 960 × 256 | about 331px tall |
| **C · Right to left** | B mirrored: Gateway, then the checks, then the handler on the right. | **Left**, from the verify line. | 960 × 256 | about 331px tall |

In every variant Bifrost is fed by dotted taps from the handler and from the
checks, and is never a step on the call path.

Only the selected variant is rendered, so only its pulses run. The section
heading, selector, and figure all align to the page's content width
(`min(1240px, 100% - 40px)`). Below 960px of available width the diagram keeps
its drawn size and scrolls sideways inside the figure, with a one-line hint;
the page itself does not scroll sideways.

## Color semantics

| Color | Means | Where |
|---|---|---|
| Periwinkle `#7a8cff` | Wyrd real-time verification, the primary subject | The solid, lightly filled checks frame and its header label, the handler link, the call pulse |
| Amber `#f0a13c` | Gateway / model route, as the third diagram series | The Gateway outline, its stub, the "MODEL CALL" label, the two model-call pulses |
| Lime `#c5f23b` | Bifrost and retained evidence | The rail outline, "BIFROST" and "ASYNC CAPTURE" labels, taps, capture pulses |
| Green `#3dbe5a` / red `#e2484d` | Passed / failed, always with a ✓ or ✕ | Progress bars, the verdict pulse |
| Muted gray | The application's own handler outline and line numbers | Handler frame |

Amber is not used as a status here. The Agent check itself stays neutral and
ends green or red like the assertions; only its route to the Gateway is amber.

## The loop

Unchanged from `../simple/`: about 2.7 seconds, repeating the all-pass case.
The call pulse leaves the handler, the assertion bars fill, the Agent bar
fills while a model-call pulse goes out to the Gateway and back, the verdict
pulse returns, and only then does the handler's release (or hold) line
highlight. Capture pulses to Bifrost start about 0.35 seconds after that and
nothing waits on them. "Policy violation" shows the failing case on request.

The pace is illustrative. It is not measured verification, model, or storage
performance, and the page submits no data.

## How it is built

- `index.html`: the unchanged hero, the direction selector, one SVG per
  variant, and the same review controls as `../simple/`.
- `directions.css`: loaded after `../simple/hero.css`; full-width layout, the
  color hierarchy, and the shared pulse keyframes.
- `directions.js`: fills in the handler (and the check rows for B and C) from
  one template each, and wires the selector.
- The beat timer, case switch, pause/replay, reduced-motion state, and
  offscreen/hidden pause come from `../simple/hero.js`, loaded as is. A change
  to that file changes this page too.

## Known trade-offs

- The diagrams draw 10–12 unit labels on a 960-unit canvas, so text is about
  1.3× that at full desktop width and exactly that size once the figure starts
  scrolling. Diagram labels are about one unit larger than in `../simple/`.
  The handler code is the exception: 9.5 units on an 11-unit line step. It is
  the smallest text on the page (about 12px at desktop width, 9.5px once the
  figure scrolls).
- Conventional blank lines cost height. The handler is 166 units tall (was
  146) to hold twelve physical lines, and every variant grew 20–25 units:
  A to 258, B and C to 256, about 331–333px at 1240px wide, just over the
  earlier 330px target. In A the checks frame moved down again so the step
  from line 9 (y123) into it stays 37 units; in B and C the checks frame,
  Gateway, and Bifrost rail all moved down 20 so frames stay bottom-aligned.
- In the code, amber (functions) and lime (strings) are the docs site's syntax
  colours; in the diagram the same two hues mean Gateway and Bifrost. The code
  tokens are small and sit inside the handler frame, but the overlap is real.
- A's upper right corner is empty by design; that space is what makes the
  downward step visible.
- A's Bifrost rail is 380 units wide under the handler, not full width.

## Naming the Wyrd call

A reviewer could not tell that the code panel is their own API with Wyrd
embedded: "is run.observe.verify, wyrd? … the name wyrd isnt shown anywhere.
is it an import?"

First attempt, since replaced: a persistent "WYRD VERIFICATION" label above
the call. It was browser-checked (the label cleared the `def` line, the call
text cleared the progress slot, page overflow and the reduced-motion state
were rechecked, captures refreshed in `previews/`), but the reviewer asked for
the import and route decorator in the code instead of an annotation.

Current: the handler, identical in all three variants, is this integration
sketch under the header "YOUR FINANCE API":

```python
from wyrd import WyrdState

state = WyrdState.declare(service)


@api.post("/reports/send")
def send_report(report: Report):
    with state.run() as run:
        r = run.observe.verify(name, **data)
    if r.verdict == "passed":
        return release(report)
    return hold(report)
```

Scope: this sketches where Wyrd sits in an API handler; it is not a runnable
app. `service`, `api`, `Report`, `name`, `data`, `release`, and `hold` are
supplied by the surrounding application and are deliberately not imported or
set up here. The Wyrd calls (`WyrdState.declare`, `state.run()`,
`run.observe.verify`) follow the repository's real examples.

The loop highlights line 9 (the verify call) during the call, then line 11
(`return release`) or line 12 (`return hold`); line 9 also carries a
periwinkle stripe at rest. Each variant's link and pulses attach at line 9.
Line numbers count physical lines, blanks included. The supporting sentence reads
"Your finance API calls Wyrd to verify each report before releasing it."

### Code formatting and colour

A reviewer asked for "better formatting and coloring". The code meaning is
unchanged; the static lines are hand-tokenised with `tspan`s (no highlighter).

| Token | Colour | Applies to |
|---|---|---|
| Keyword | periwinkle `#7a8cff` | `from` `import` `def` `with` `as` `if` `return` |
| String | lime `#c5f23b` | `"/reports/send"`, `"passed"` |
| Function | amber `#f0a13c` | `@api.post`, `declare`, `send_report`, `run`, `release`, `hold` |
| Punctuation | muted `#8f92a5` | `= . ( ) : , ** ==` |
| Wyrd name | periwinkle | `WyrdState`, `verify` |
| Everything else | text `#e6e4da` | names, `Report` |

These are the docs site's code colours. Indents are four and eight 9.5-unit
mono advances (x 44, 66.8, 89.6).

A first formatting pass grouped nine lines with 3 extra units between import,
setup, and handler. The reviewer then asked for conventional formatting: "there
needs to be a space between the function and import." The code now has real
blank lines, one after the import and two before the decorated handler, none
between decorator and `def`: twelve physical lines, line n at baseline
38 + 11(n − 1), blank lines 2, 4, and 5 numbered like the rest.

Browser-checked after the blank-line layout: line numbers 1–12 align with
physical lines, including blanks 2/4/5. The call clears its progress slot,
highlights align with lines 9/11/12, all three diagrams fit, and mobile has no
page overflow. Desktop A/B were inspected visually and all captures refreshed.

Browser-checked after the code sketch update at desktop and mobile sizes.
The verification line clears the progress slot, highlights match lines 6/8/9
(the nine-line numbering of that revision),
and the loop and reduced-motion finished state work. Desktop A/B and mobile B
were inspected visually; captures for all three directions were refreshed.
Whether the code now reads as “my API calling Wyrd” still needs a fresh viewer.

Browser-checked after the formatting and color pass: computed syntax colors
match the docsite palette, four/eight-character indents align, the call clears
the progress slot, and highlights remain aligned. All three desktop/mobile
captures are refreshed; B was inspected visually at both sizes. Mobile still
scrolls within the figure with no page overflow. No application behavior changed.

## Review evidence and limits

Browser-checked at 1440px desktop and 390px mobile. At desktop the diagram
fills the 1240px content width: A is 333px tall, B and C are 331px tall.
Desktop captures and mobile captures for all three are in `previews/`.

Checks passed for one visible variant, direction switching, the 2.7-second
loop advancing, Pause/Replay, both cases, Gateway pulses during the Agent
beats, response before asynchronous capture, reduced-motion finished state,
and no page overflow on phones. The mobile diagram scrolls internally to
preserve text size; the whole flow is not visible at once on a phone.
Desktop compositions and a mobile capture were inspected visually.

The style detector reports inherited hero typography and compact SVG label
warnings. The existing hero is intentionally preserved; diagram labels scale
with the SVG and have a 960px minimum diagram width. No warnings were suppressed.
This is a local illustration, not a backend benchmark or a complete production
accessibility review. The production docsite is unchanged.

# Workflows section

A lower landing-page section: four small animated diagrams in one row,
showing what people build on the Wyrd verification engine. It complements the
detailed finance animation in `../directions/` (variant B), which sits above
it on the page; this mockup links to that one instead of embedding it.

This is an illustrative product concept. It makes no backend calls, and its
pace is not a performance claim.

## Preview

Serve the parent `mockups/` directory and open `/workflows/#workflows`:

```bash
cd changes/active/positioning/design/mockups
mise exec -- python -m http.server 8000   # then http://localhost:8000/workflows/#workflows
```

## The four stories

| Panel | What the diagram shows |
|---|---|
| **Real-time verification** | Request → Your API → down to the Wyrd Verifier and back with "✓ passed" → Your API releases. |
| **Agent evaluation** | Your agent runs Plan → Retrieve → Compose. Each step drops into a lime run record (three pips). The record is submitted to a Verifier, which ticks off the three steps, then a verdict. The agent executes; Wyrd only judges. |
| **Code review** | A change fans out on three amber branches to Correctness, Security, and Tests reviewers working in parallel. They finish at different times and fan in to one Verifier (three pips), which gives one combined verdict. |
| **Session evaluation** | Four events (user turn, tool call, agent turn, user turn) stack up one by one in a lime session frame. "quiet · ready" appears, the group moves as one larger pulse into a Verifier, then a session verdict. |

Colour: grey dashed frames are your application's steps and turn solid when
they run; amber is agent work; lime is recorded evidence; solid periwinkle is
always the Verifier; green with a ✓ is a passed result.

## Motion and controls

One shared loop of about 3.6 seconds (six 280 ms beats, then a 1.9 second
rest on the finished state) drives all four diagrams, so they move together
rather than on four timelines. Panel 1 finishes first; the others settle on
the last beats. "Illustrative animation" is labelled once, with Pause and
Replay for all four. With reduced motion the finished state shows and nothing
moves until Play or Replay. The loop pauses while the section is offscreen or
the tab is hidden.

## How it is built

- `index.html`: the unchanged hero, a one-line review note where the finance
  animation goes, and the section with four static SVGs. Each SVG carries its
  own text alternative; labels and frames are in the HTML, so the diagrams
  read without script.
- `workflows.css`: loaded after `../simple/hero.css` (tokens, fonts, hero,
  shared diagram primitives); adds the four-panel row and node colours.
- `workflows.js`: the beat timer, Pause/Replay, reduced motion, and the
  offscreen/hidden pause. It is a small copy of the `../simple/hero.js` driver
  without the pass/fail case switch, which that file requires.
- `../simple/background.js` paints the hero background, unchanged.

Layout: four columns across the full content width with dashed vertical
rules; two by two at 1000px and below; one column at 560px and below. Each
diagram is a 270 × 196 viewBox drawn at about 1:1 on desktop (12.5px labels,
12px mono notes) and capped at 340px wide.

## Limits

- Only the passing outcome is shown; there is no failing case here.
- Before the first beat of each loop the diagrams are grey skeletons with
  their labels; the session rows are empty outlines until events arrive.
- Whether each capability ships as drawn is a product question this preview
  does not answer.

## Browser review

Checked 1440px desktop, 768px tablet and 390px phone. Desktop shows four panels
in one row with three dashed vertical dividers; each diagram is about 203px tall.
Tablet uses two columns and phone one, with no page overflow. Desktop/mobile
were inspected visually, and captures are in `previews/`.

Behavior checks passed for loop advancement, Pause/Replay, reduced-motion
finished state, hidden-tab/offscreen suspension, and session rows accumulating
before verification. No JavaScript exceptions were observed. The agent diagram
explicitly labels submit/queued, and the three review branches converge on the
Verifier. Syntax and whitespace checks passed.

The style detector flags only preserved hero typography. No warnings were
suppressed. This remains a local concept preview, not a production accessibility
qualification or a runtime performance measurement.

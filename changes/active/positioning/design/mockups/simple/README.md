# Verification in the flow

A landing-page section, not a hero change. The cover keeps its existing
painted background, headline, positioning, and CTA with nothing placed over
it. Directly below the cover, a section titled "Verification in the flow."
holds one sentence and one small animated figure.

## Composition

Two dashed outlines joined by one short dashed link, plus two subordinate
details: a gateway stub under the Agent check and a Bifrost rail at the bottom.

```text
┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌ FINANCE API ╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
  1  def send_report(report):
  2      r = run.observe.verify(name, **data)  ▰▱ ✓
  3      if r.verdict == "passed":
  4          release(report)              → Release
  5      else:
  6          hold(report)
└┬╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌●╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘
 ┊        WYRD · VERIFY ┆ ✓ PASSED
 ┊ ┌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┐
 ┊   ▰▰▰ ✓          ▰▰▰ ✓          ▰▰▰ ✓
 ┊   ASSERT         AGENT          ASSERT
 ┊   Recipient      Sharing        Destination
 ┊   authorized     policy · PDF   private
 ┊ └╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┆╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┬╌╌╌┘
 ┊                    ┆ MODEL CALL     ┊
 ┊                 [GATEWAY]           ┊ ASYNC CAPTURE
┌┴╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┴╌╌╌╌╌┐
  BIFROST   Records · traces · app data
└╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌╌┘
```

The top outline is the application's own handler: one synchronous verify call,
then its own release-or-hold branch. The middle outline is Wyrd's three checks
as small progress bars. The handler code and every label are static; only the
line highlight, the bars, the pulses, and the result marks move.

- **Gateway.** Only the Agent check has the stub. It stands for the Agent's
  model call going to the provider through the gateway. It does not mean every
  Agent tool call or file read goes there, and the assertions never use it.
- **Bifrost.** Two dotted taps, one from the handler and one from the checks,
  lead to a rail labeled "Records · traces · app data". Their pulses start
  only after the handler has continued. It is background capture, not another
  stage the response waits on. It means data the application supplies or
  configures for capture, plus verification results and evidence; not
  automatic collection of everything the application does.

The layout takes its restraint from the supplied reference (dashed contours,
small labels, progress bars, a single link). The colors, fonts, content, and
claims are Wyrd's; there is no comparison panel.

## Preview

Serve the parent `mockups/` directory (the fonts live in `../fonts/`) and open
`/simple/`:

```bash
cd changes/active/positioning/design/mockups
mise exec -- python -m http.server 8000   # then http://localhost:8000/simple/
```

## The loop

About 2.7 seconds, repeating the all-pass case:

1. Line 2 highlights, the call's bar starts filling, a pulse runs down the link.
2. Both assertion bars fill at once; the Agent bar fills over a longer span
   while a pulse runs out to the gateway.
3. The gateway pulse returns and the Agent bar completes.
4. The verdict pulse returns up the link.
5. The handler's `release(report)` line highlights and shows `→ Release`,
   then rests for about 1.8 seconds. During that rest, starting about 0.35
   seconds in, small lime pulses travel down the two taps into Bifrost.

The release or hold line never highlights before the verdict has returned,
and no capture pulse starts before it has.

The loop does not switch case on its own. The "Policy violation" review
control shows the second case: the Agent bar ends as a failure, both
assertions still pass, and the `hold(report)` line highlights with `‖ Hold`.

## What is illustrative

All of it. The pace shows verification sitting inside a request without
stalling it; it is not a measured latency. The Agent bar is drawn slower than
the assertions only to show they are different kinds of work, not how long a
model judgment takes. The gateway and capture pulses are likewise drawn pace,
not model or storage performance. Nothing is benchmarked, nothing here is
wired into the docs site, and the preview submits no data anywhere.

The handler code is a sketch written for this figure. It is not taken from a
tested example while its verification call uses the public Python Run/Observe API. A
production version must embed a complete journey-tested example file.

## Limits

- `background.js` is the existing `CoverScene` drawing, unmodified, and the
  hero places nothing over it.
- Bars and result marks reset at the start of each loop, so they are empty for
  a few hundred milliseconds every cycle.
- Figure text is SVG and scales with the figure (680px wide at most). The
  code is the smallest text and will be hard to read on phones around 320px.
- With `prefers-reduced-motion` the figure rests on the finished state with
  the pulses removed; Play and Replay step through the beats.
- Resuming after Pause restarts the current beat. With beats this short, most
  pauses land on the finished state.
- The figure pauses offscreen and when the tab is hidden.
- Browser checks cover below-hero placement, the default successful case,
  the 2.7-second illustrative cycle, animation advancement, pause/replay,
  verdict-before-response ordering, both cases, reduced motion and mobile
  overflow. The Gateway out/return beats and delayed Bifrost capture after the
  response were also checked, and desktop/mobile captures in `previews/` were
  refreshed. This is not a backend benchmark or complete production
  accessibility qualification.
- With reduced motion the capture pulses are removed, so the ordering of
  Bifrost capture rests on the "ASYNC CAPTURE" label and the description.

## Files

- `index.html`: the cover, the flow section with its SVG figure, and the
  review controls.
- `hero.css`: dark-field tokens, layout, and motion.
- `hero.js`: the beat timer and controls.
- `background.js`: the existing cover drawing (do not edit).

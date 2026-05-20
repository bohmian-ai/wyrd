# Wyrd Data Performance

## Decision rules

- Keep large data operations server-backed by default.
- Paginate, filter, sort, and aggregate on the server for remote datasets.
- Keep browser payloads bounded and typed.
- Render only what the user can inspect.
- Use virtualization for large scrollable lists or tables.

## Pagination and filtering

- Prefer cursor or server-backed pagination when data changes often.
- Keep query state in the URL when users need to share, refresh, or navigate back to a filtered view.
- Debounce text filters that trigger network requests.
- Cancel stale requests when filters change quickly.

## Tables and dense views

- Stabilize column widths and row heights where possible.
- Use sticky headers only when they help repeated scanning.
- Keep actions discoverable but compact.
- For large tables, render summary counts and current filter state near the table.

## Charts and timelines

- Use server aggregation for high-cardinality or long-window data.
- Keep chart legends close to the chart and use semantic colors consistently.
- Do not overload Wyrd status colors for decorative series.
- Always provide useful empty and no-data states.

## Files and code blocks

- Lazy-load large file contents.
- Add copy affordances for IDs, file names, commands, and snippets.
- Use monospaced layouts for code and structured logs.
- Avoid rendering unbounded markdown or code content without size limits.

## Anti-patterns

- Fetching all pages to sort or filter in the browser.
- Running expensive transformations during every render.
- Letting charts or tables resize unpredictably when data changes.
- Sending raw backend payloads directly into components without shaping them.

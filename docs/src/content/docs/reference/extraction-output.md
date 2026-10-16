---
title: Extraction output
description: The exact JSON an extract step returns for every media format, how to address it from assertions and citations, and what extraction does not do.
pillar: wyrd
section: Reference
order: 4
draft: true
status: runnable
---

An `extract` step reads one `media` slot and returns JSON whose shape depends on the file's format. The same shape reaches assertions, Agent and code inputs, and every SDK's result types.

```yaml
steps:
  - id: document
    action: { kind: extract, from: inputs.policy }
```

Every field is always present. A value that is unavailable is `null` only where the type below allows it; an empty list is `[]` and empty text is `""`. Counts come from list lengths: there is no `page_count` or `slide_count`.

## Formats

The TypeScript notation describes the JSON on the wire.

### PDF

```typescript
type PdfOutput = { format: "pdf"; pages: { number: number; text: string }[] };
```

```json
{"format":"pdf","pages":[{"number":1,"text":"Refund policy\nRefunds are issued within 30 days."},{"number":2,"text":""}]}
```

`number` is one-based.

### PPTX

```typescript
type PptxOutput = {
  format: "pptx";
  slides: {
    number: number;
    title: string | null;
    paragraphs: string[];
    tables: { rows: string[][] }[];
    notes: string;
  }[];
};
```

```json
{"format":"pptx","slides":[{"number":1,"title":"Q3 refunds","paragraphs":["Refund rate fell to 2%"],"tables":[{"rows":[["Region","Rate"],["US","2%"]]}],"notes":""}]}
```

Slides are in presentation order; `number` is one-based.

### DOCX

```typescript
type DocxOutput = {
  format: "docx";
  blocks: ({ kind: "paragraph"; text: string; style: string | null } | { kind: "table"; rows: string[][] })[];
};
```

```json
{"format":"docx","blocks":[{"kind":"paragraph","text":"Refund policy","style":"Heading1"},{"kind":"table","rows":[["Window","Days"],["Standard","30"]]}]}
```

Blocks are in document order. `style` is the stored paragraph style ID.

### XLSX

```typescript
type XlsxOutput = {
  format: "xlsx";
  sheets: {
    name: string;
    cells: { address: string; value: string | number | boolean | null; formula: string | null; error: string | null }[];
  }[];
};
```

```json
{"format":"xlsx","sheets":[{"name":"Refunds","cells":[{"address":"A1","value":"Days","formula":null,"error":null},{"address":"B2","value":30,"formula":null,"error":null}]}]}
```

- Sheets keep workbook order; cells keep row, then column order.
- `address` is the worksheet address, such as `B2`, not a position in the list. Empty cells are omitted, so `cells[1]` above is `B2`.
- `value` is the stored value or the cached result of a formula. Formulas are never recalculated.
- Dates stay Excel serial numbers.
- An error cell has `value: null` and the error, such as `#DIV/0!`, in `error`.

### CSV

```typescript
type CsvOutput = { format: "csv"; rows: string[][] };
```

```json
{"format":"csv","rows":[["region","rate"],["US","0.02"]]}
```

Every row is kept, including the first. Neither headers nor value types are inferred: every cell is a string.

### JSON

```typescript
type JsonOutput = { format: "json"; value: JsonValue };
```

```json
{"format":"json","value":{"refund_days":30,"regions":["US","EU"]}}
```

`value` is the parsed document.

### Text, Markdown, HTML

```typescript
type TextOutput = { format: "text" | "markdown" | "html"; text: string };
```

```json
{"format":"markdown","text":"# Refund policy\n\nRefunds are issued within 30 days.\n"}
```

The source text, unchanged. Markdown and HTML are not parsed into structure.

## Tables

In PPTX, DOCX, and similar tables, each cell holds its extracted text. A merged cell's value is in its top-left position; the positions it covers are `""`, so every row of a table has the same length.

## Addressing the output

References use dotted paths with zero-based indexes and no `$.` prefix:

```yaml
requirements:
  - id: titled
    statement: The policy opens with its title
    check: { kind: assert, path: steps.document.output.blocks[0].text, operator: equals, expected: Refund policy }
  - id: thirty-days
    statement: The standard refund window is 30 days
    check: { kind: assert, path: steps.workbook.output.sheets[0].cells[1].value, operator: equals, expected: 30 }
  - id: no-errors
    statement: No cell in the workbook is an error
    check: { kind: assert, each: "steps.workbook.output.sheets[*].cells[*].error", operator: equals, expected: null }
```

`[*]` is allowed only in `each`, which passes when every element passes. List indexes are zero-based; PDF page and PPTX slide `number` values are one-based.

## Citations into extracted files

A citation names the original slot, never the extract step:

| Format | Locator | Example |
| --- | --- | --- |
| PDF | `page` | `{ "slot": "policy", "locator": { "page": 2 } }` |
| PDF | `region` | `{ "slot": "policy", "locator": { "region": { "page": 2, "x": 72, "y": 140, "width": 300, "height": 40 } } }` |
| PPTX | `slide` | `{ "slot": "deck", "locator": { "slide": 3 } }` |
| Text, Markdown, HTML, CSV | `lines` | `{ "slot": "notes", "locator": { "lines": { "start": 4, "end": 6 } } }` |
| DOCX, XLSX, JSON | `json_pointer` | `{ "slot": "policy", "locator": { "json_pointer": "/blocks/0/text" } }` |

For DOCX and XLSX, the JSON pointer addresses this page's extracted JSON for that slot, such as `/blocks/0/text` or `/sheets/0/cells/1/value`. It is not a byte offset into the file. The server checks every citation against the pinned file and the extraction version that produced it; a pointer or page that does not exist makes a passing Agent check `inconclusive`.

## Limits

- Extraction is text and structure only. There is no OCR, no rendering, and no extracted image files.
- To check how something looks, give an Agent the original file as a `media` input; it reaches a model that supports that format natively. A caller supplies a PDF or image rendition of an Office file when a visual check needs one.
- `ppt`, `doc`, and `xls` are distinct from `pptx`, `docx`, and `xlsx` and are not extracted.
- Pages, slides, and extracted text are bounded by `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` and `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB`; see [Verification runtime limits](/reference/verification-runtime-limits/). A file over a limit fails the step, and requirements that need it are `inconclusive`.

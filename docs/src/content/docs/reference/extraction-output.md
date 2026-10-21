---
title: Extraction output
description: The JSON an extract step returns for each media format.
pillar: wyrd
section: Reference
order: 4
status: runnable
---

An `extract` step returns JSON whose shape depends on the file's format. Every field is always present: an empty list is `[]`, empty text is `""`, and `null` appears only where the type allows it. Types are TypeScript notation for the wire JSON.

## PDF

```typescript
type PdfOutput = { format: "pdf"; pages: { number: number; text: string }[] };
```

`number` starts at 1. `text` is `""` for a page with no text layer, such as a scan.

## PPTX

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

- Slides are in presentation order; `number` starts at 1.
- `title` is the title placeholder's text, lines joined by `\n`, or `null`.
- `paragraphs` holds the other non-empty paragraphs, not table text. `notes` is `""` when there are none.

## DOCX

```typescript
type DocxOutput = {
  format: "docx";
  blocks: ({ kind: "paragraph"; text: string; style: string | null } | { kind: "table"; rows: string[][] })[];
};
```

- Blocks are the document body in order, including content controls. Headers, footers, footnotes, and comments are not read.
- `style` is Word's style ID, such as `Heading1`, or `null`.

## XLSX

```typescript
type XlsxOutput = {
  format: "xlsx";
  sheets: {
    name: string;
    cells: { address: string; value: string | number | boolean | null; formula: string | null; error: string | null }[];
  }[];
};
```

- Sheets are in workbook order; cells row by row, left to right. Empty cells are left out, so look a cell up by `address`, such as `B2`.
- `value` is the stored value, or a formula's last saved result; formulas are never recalculated. Dates are Excel serial numbers.
- An error cell has `value: null` and the error, such as `#DIV/0!`, in `error`.

## CSV

```typescript
type CsvOutput = { format: "csv"; rows: string[][] };
```

Every row, including the header. Every cell is a string.

## JSON

```typescript
type JsonOutput = { format: "json"; value: JsonValue };
```

## Text, Markdown, HTML

```typescript
type TextOutput = { format: "text" | "markdown" | "html"; text: string };
```

The file unchanged; Markdown and HTML are not parsed.

## Tables

In PPTX and DOCX tables, a merged cell's text is in its top-left position and the positions it covers are `""`, so every row has the same length.

## Paths

Read fields as `steps.<id>.output.pages[0].text`. Indexes are zero-based. `[*]` is allowed only in an `assert` check's `each`.

## Citations

A `json_pointer` locator addresses this extracted JSON for DOCX and XLSX, such as `/blocks/0/text`, and the parsed document for JSON. Other locators are in [Verifier Card](/reference/verifier-card/#verdicts-and-citations).

## Not extracted

| Input | Instead |
| --- | --- |
| `png`, `jpeg`, `gif`, `webp` | No OCR or rendering; give the file to an Agent as `media` |
| `doc`, `xls`, `ppt` | Convert to `docx`, `xlsx`, or `pptx` |
| `zip`, `tar_gz` | Put them in a step Agent's `workspace` |

## If it fails

The step fails and the requirements that need it are `inconclusive` when the file does not parse as its format, a PDF is encrypted, a text file is not UTF-8, or the file exceeds `WYRD_VERIFICATION_EXTRACT_MAX_PAGES` or `WYRD_VERIFICATION_EXTRACT_TEXT_MAX_MIB` ([limits](/reference/verification-runtime-limits/)). Each result records the extractor version `wyrd-extract/1`.

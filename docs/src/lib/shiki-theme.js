// Wyrd code-block theme. Code sits on the canonical dark code field in both
// modes, so there is one theme and it carries no colors: Shiki emits
// `var(--shiki-*)` references that docs.css maps to the generated `--code-*`
// tokens (brand/palette.json). No syntax color is authored here.
import { createCssVariablesTheme } from 'shiki';

export const wyrdCode = createCssVariablesTheme({
  name: 'wyrd-code',
  variablePrefix: '--shiki-',
  variableDefaults: {},
  fontStyle: true
});

export const WYRD_THEME = 'wyrd-code';

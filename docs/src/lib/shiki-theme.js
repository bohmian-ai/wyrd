// Custom Shiki themes for Wyrd code blocks.
//
// Syntax colors ARE the brand tokens, so highlighted code matches the approved
// Arcade Cabinet mock instead of generic github-light/github-dark. The hex
// values mirror docs/src/styles/wyrd-tokens.css (generated from
// brand/palette.json) — TextMate themes require static colors, so they cannot
// reference the CSS vars directly; keep these in sync with the palette.
//
// Used dual-theme: Shiki inlines the light color on each token plus a
// `--shiki-dark` custom prop with the dark color; arcade.css swaps to
// `--shiki-dark` under `:root[data-theme="dark"]`. Both themes render over a
// transparent `<pre>` so the `.code` surface token provides the background.

// --- light (parchment / white surface) ---
const L = {
  text: '#14141a',
  comment: '#6b6b76', // muted
  keyword: '#7c3aed', // rune-strong
  string: '#2f9e44', // ok (readable green; lime is invisible on white)
  func: '#ec8b14', // server-bar
  number: '#2f6f9e', // control-bar
  type: '#2f6f9e', // control-bar
  punct: '#6b6b76' // muted
};

// --- dark (near-black surface) ---
const D = {
  text: '#d6d6dc',
  comment: '#8a8a95', // muted
  keyword: '#a78bfa', // rune-strong
  string: '#c5f23c', // lime
  func: '#f2a23c', // server-bar
  number: '#6fb3d6', // control-bar
  type: '#6fb3d6', // control-bar
  punct: '#8a8a95' // muted
};

/**
 * @param {string} name
 * @param {'light' | 'dark'} type
 * @param {Record<string, string>} c
 * @param {string} bg
 * @returns {import('shiki').ThemeRegistrationRaw}
 */
function theme(name, type, c, bg) {
  return {
    name,
    type,
    colors: { 'editor.foreground': c.text, 'editor.background': bg },
    settings: [
      { settings: { foreground: c.text } },
      {
        scope: ['comment', 'punctuation.definition.comment', 'string.comment'],
        settings: { foreground: c.comment, fontStyle: 'italic' }
      },
      {
        scope: [
          'keyword',
          'storage',
          'storage.type',
          'storage.modifier',
          'keyword.control',
          'keyword.operator.new',
          'keyword.operator.expression',
          'variable.language'
        ],
        settings: { foreground: c.keyword }
      },
      {
        scope: ['string', 'string.quoted', 'string.template', 'constant.other.symbol'],
        settings: { foreground: c.string }
      },
      {
        scope: [
          'entity.name.function',
          'support.function',
          'meta.function-call',
          'meta.function-call.generic'
        ],
        settings: { foreground: c.func }
      },
      {
        scope: ['constant.numeric', 'constant.language', 'constant.character', 'constant.language.boolean'],
        settings: { foreground: c.number }
      },
      {
        scope: [
          'entity.name.type',
          'entity.name.class',
          'entity.name.namespace',
          'support.type',
          'support.class'
        ],
        settings: { foreground: c.type }
      },
      {
        scope: ['punctuation', 'meta.brace', 'keyword.operator', 'punctuation.separator'],
        settings: { foreground: c.punct }
      }
    ]
  };
}

export const wyrdLight = theme('wyrd-light', 'light', L, '#ffffff');
export const wyrdDark = theme('wyrd-dark', 'dark', D, '#17171b');

export const WYRD_THEMES = { light: 'wyrd-light', dark: 'wyrd-dark' };

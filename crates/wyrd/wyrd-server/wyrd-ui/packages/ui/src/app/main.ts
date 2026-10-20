/**
 * The `ui://wyrd/view` MCP App: receives a view document as a tool result,
 * loads every panel through the host's `tools/call` as the same caller, and
 * follows the host's theme. Built into one self-contained HTML file.
 */
import '@fontsource/familjen-grotesk/latin-400.css';
import '@fontsource/familjen-grotesk/latin-600.css';
import '@fontsource/familjen-grotesk/latin-700.css';
import '@fontsource/fragment-mono/latin-400.css';
import { App } from '@modelcontextprotocol/ext-apps';
import { mcpExecutor } from '../lib/mcp.js';
import { mountView } from '../lib/view.js';

const app = new App({ name: 'Wyrd view', version: '0.1.0' }, { availableDisplayModes: ['inline', 'fullscreen'] });
const executor = mcpExecutor(app);
let view: { destroy(): void } | undefined;

function theme() {
  document.documentElement.dataset.mode = app.getHostContext()?.theme === 'dark' ? 'dark' : 'light';
}

app.onhostcontextchanged = theme;
app.ontoolresult = (result) => {
  view?.destroy();
  // Layout edits stay on screen for this render only; nothing is saved.
  view = mountView(document.body, { doc: result.structuredContent, executor, onlayout: () => {} });
};
void app.connect().then(theme);

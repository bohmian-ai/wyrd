// Build-time page update dates. A page's date is the last commit that changed
// its source file, so it moves only with a real content change and never with
// the build clock. Node-only: imported by svelte.config.js, not the client.
import { execFileSync } from 'node:child_process';
import path from 'node:path';

/**
 * Commit date (YYYY-MM-DD) of the last commit that changed `file`, or undefined
 * when the file has no history (new, ignored, outside a repository) or git is
 * unavailable. Requires full history: a shallow clone dates every file to the
 * clone's tip commit.
 * @param {string} file absolute path to the page source
 * @returns {string | undefined}
 */
export function lastCommitDate(file) {
  try {
    const out = execFileSync('git', ['log', '-1', '--format=%cs', '--', path.basename(file)], {
      cwd: path.dirname(file),
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'ignore']
    }).trim();
    return out || undefined;
  } catch {
    return undefined;
  }
}

/**
 * mdsvex remark plugin: sets `updated` in page frontmatter from git history,
 * replacing any hand-authored value so repository history is the one source.
 * The value flows to `metadata.updated` and is validated by content.ts.
 * @returns {(tree: unknown, file: { filename?: string, data: { fm?: Record<string, unknown> } }) => void}
 */
export function remarkUpdated() {
  return (_tree, file) => {
    const fm = file.data.fm;
    if (!fm) return;
    const date = file.filename ? lastCommitDate(file.filename) : undefined;
    if (date) fm.updated = date;
    else delete fm.updated;
  };
}

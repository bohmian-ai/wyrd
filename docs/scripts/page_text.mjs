// Print the plain-Markdown body of each content page named on the command
// line as one JSON array, in argument order, for generate_llms_txt.py.
import { readFileSync } from 'node:fs';
import { pageText } from '../src/lib/mdsvex/page-text.js';

process.stdout.write(JSON.stringify(process.argv.slice(2).map((file) => pageText(readFileSync(file, 'utf8')))));

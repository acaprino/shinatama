// Links in terminal text: web URLs and local HTML paths, opened in a browser
// tab with a click (ADR-0056 v2). Pure parsing; the xterm wiring is in main.

export interface FoundLink { readonly start: number; readonly end: number; readonly target: string }
export type LinkTarget =
  | { readonly kind: 'web'; readonly url: string }
  | { readonly kind: 'file'; readonly path: string }
  | { readonly kind: 'refused'; readonly reason: string };

const WEB = /\bhttps?:\/\/[^\s<>"'`]+/gi;
// A Windows path ending in .html/.htm: absolute (C:\…), relative (docs\a.html, .\a.html) or file:///.
const HTML_PATH = /(?:file:\/\/\/[^\s<>"'`]+?\.html?|(?:[A-Za-z]:[\\/]|\.{1,2}[\\/])?(?:[\w.$@()+-]+[\\/])*[\w.$@()+-]+\.html?)(?![\w.])/gi;
const TRAILING = /[.,;:!?]+$/;

/** Drops trailing punctuation and a closing bracket the URL did not open. */
function trim(candidate: string): string {
  let text = candidate.replace(TRAILING, '');
  for (const [open, close] of [['(', ')'], ['[', ']'], ['{', '}']] as const) {
    while (text.endsWith(close) && text.split(open).length < text.split(close).length) text = text.slice(0, -1).replace(TRAILING, '');
  }
  return text;
}

export function findLinks(line: string): FoundLink[] {
  const links: FoundLink[] = [];
  for (const match of line.matchAll(WEB)) {
    const target = trim(match[0]); const start = match.index ?? 0;
    if (target.length > 'https://'.length) links.push({ start, end: start + target.length, target });
  }
  for (const match of line.matchAll(HTML_PATH)) {
    const start = match.index ?? 0; const end = start + match[0].length;
    if (links.some(link => start < link.end && end > link.start)) continue;
    links.push({ start, end, target: match[0] });
  }
  return links.sort((a, b) => a.start - b.start);
}

function joinWindows(cwd: string, relative: string): string {
  const parts = cwd.replace(/[\\/]+$/, '').split(/[\\/]/);
  for (const piece of relative.split(/[\\/]/)) {
    if (piece === '' || piece === '.') continue;
    if (piece === '..') { if (parts.length > 1) parts.pop(); continue; }
    parts.push(piece);
  }
  return parts.join('\\');
}

/** A terminal cell as the link finder reads it: its text and its 1-based column. */
export interface CellText { readonly chars: string; readonly x: number }
export interface CellLink { readonly target: string; readonly start: { x: number; y: number }; readonly end: { x: number; y: number } }
/** Links in a line the terminal wrapped over several rows (`rows[0]` is row `firstY`, 1-based):
 *  a path cut by the right edge is still one link, its range running from row to row. */
export function findLinksInRows(rows: readonly (readonly CellText[])[], firstY: number): CellLink[] {
  let text = ''; const at: { x: number; y: number }[] = [];
  rows.forEach((cells, row) => { for (const cell of cells) { text += cell.chars; for (let unit = 0; unit < cell.chars.length; unit++) at.push({ x: cell.x, y: firstY + row }); } });
  return findLinks(text).map(link => ({ target: link.target, start: at[link.start], end: at[link.end - 1] }));
}

/** What a found link (or an OSC 8 URI) would open. Nothing is opened here. */
export function classifyTarget(target: string, cwd: string): LinkTarget {
  const text = target.trim();
  if (/^https?:\/\//i.test(text)) {
    try { const url = new URL(text); return { kind: 'web', url: url.href }; } catch { return { kind: 'refused', reason: 'Not a valid link.' }; }
  }
  if (/^file:\/\/\//i.test(text)) {
    let path: string;
    try { path = decodeURIComponent(text.slice('file:///'.length)).replace(/\//g, '\\'); } catch { return { kind: 'refused', reason: 'Not a valid file link.' }; }
    return /\.html?$/i.test(path) && /^[A-Za-z]:\\/.test(path) ? { kind: 'file', path } : { kind: 'refused', reason: 'Only .html files open in the browser.' };
  }
  if (/^[a-z][a-z0-9+.-]*:/i.test(text) && !/^[A-Za-z]:[\\/]/.test(text)) return { kind: 'refused', reason: `${text.split(':')[0]}: links do not open in Shinatama.` };
  if (/\.html?$/i.test(text)) return { kind: 'file', path: /^[A-Za-z]:[\\/]/.test(text) ? text.replace(/\//g, '\\') : joinWindows(cwd, text) };
  return { kind: 'refused', reason: 'Not a link Shinatama opens.' };
}

/** The text shown on hover: always the real target, never the link's own text. */
export function hoverText(target: LinkTarget): string {
  switch (target.kind) {
    case 'web': return `Click to open ${target.url} in a browser tab`;
    case 'file': return `Click to open ${target.path} in a browser tab`;
    case 'refused': return target.reason;
  }
}

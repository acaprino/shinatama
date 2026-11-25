// The answers of the browser for the files of the project: folders, a read, a save checked against the version read, a search by name and a
// picture. Development only.
import type { Answers } from './index.ts';

type Node = { kind: 'file'; text: string } | { kind: 'directory'; children: Record<string, Node> };
const file = (text: string): Node => ({ kind: 'file', text });
const TREE: Record<string, Node> = {
  docs: { kind: 'directory', children: { plans: { kind: 'directory', children: { 'billing-retries.md': file('# Billing retries\n\nRetry **with a cap**.\n\n- base 100 ms\n- cap 30 s\n\n```ts\nconst wait = Math.min(cap, base * 2 ** attempt);\n```\n') } } } },
  src: { kind: 'directory', children: {
    'retry.ts': file('import { log } from "./log";\n\nexport interface RetryPolicy {\n  readonly base: number;\n  readonly cap: number;\n}\n\nexport const DEFAULT_POLICY: RetryPolicy = { base: 100, cap: 30_000 };\n'),
    'charge.ts': file('export const charge = () => 1;\n'),
    'index.html': file('<!doctype html>\n<title>Retry</title>\n'),
  } },
  'README.md': file('# shinatama\n\nThe window.\n'),
  'logo.png': file(''),
  'data.bin': file(''),
};

const sha = (text: string) => `${text.length.toString(16).padStart(8, '0')}`.repeat(8);
const disk = new Map<string, string>();

function lookup(path: string): Node | undefined {
  let node: Node | undefined = { kind: 'directory', children: TREE };
  for (const part of path.split('/').filter(Boolean)) {
    if (node?.kind !== 'directory') return undefined;
    node = node.children[part];
  }
  return node;
}
function flat(prefix: string, children: Record<string, Node>): { path: string; name: string; kind: 'file' | 'directory' }[] {
  return Object.entries(children).flatMap(([name, node]) => {
    const path = prefix ? `${prefix}/${name}` : name;
    return [{ path, name, kind: node.kind }, ...(node.kind === 'directory' ? flat(path, node.children) : [])];
  });
}
/** A file that changes on disk behind the panel's back (an agent wrote it): the next save finds a version it did not read. */
export function changeOnDisk(path: string, text: string): void {
  disk.set(path, text);
}
const textOf = (path: string, node: Node) => disk.get(path) ?? (node.kind === 'file' ? node.text : '');

export const answers: Answers = {
  files_list: args => {
    const path = String(args?.path ?? '');
    const node = lookup(path);
    if (node?.kind !== 'directory') throw 'That folder is not in the project.';
    return {
      path, truncated: false,
      entries: Object.entries(node.children).map(([name, child]) => ({
        name, path: path ? `${path}/${name}` : name, kind: child.kind, size: child.kind === 'file' ? textOf(path ? `${path}/${name}` : name, child).length : null, linkInside: null,
      })).sort((a, b) => (a.kind === b.kind ? a.name.localeCompare(b.name) : a.kind === 'directory' ? -1 : 1)),
    };
  },
  files_search: args => {
    const query = String(args?.query ?? '').toLowerCase();
    const hits = flat('', TREE).filter(entry => entry.kind === 'file' && entry.name.toLowerCase().includes(query)).map(entry => ({ ...entry, size: 10, linkInside: null }));
    return { query: String(args?.query), hits, truncated: false, cancelled: false, timedOut: false };
  },
  files_cancel: null,
  files_read: args => {
    const path = String(args?.path);
    const node = lookup(path);
    if (node?.kind !== 'file') throw 'That file is not in the project.';
    if (path === 'data.bin') return { path, text: null, binary: true, truncated: false, bytes: 2048, sha256: sha('bin') };
    const text = textOf(path, node);
    return { path, text, binary: false, truncated: false, bytes: text.length, sha256: sha(text) };
  },
  files_save: args => {
    const request = args?.request as { path: string; text: string; baseSha256: string; overwrite: boolean };
    const node = lookup(request.path);
    const current = node?.kind === 'file' ? textOf(request.path, node) : '';
    if (request.baseSha256 !== sha(current) && !request.overwrite) return { state: 'conflict', disk_sha256: sha(current) };
    disk.set(request.path, request.text);
    return { state: 'saved', sha256: sha(request.text) };
  },
  files_image: args => ({ state: 'ready', image: { path: String(args?.path), width: 2, height: 2, bytes: 70, dataUrl: 'data:image/gif;base64,R0lGODlhAQABAIAAAP///wAAACwAAAAAAQABAAACAkQBADs=' } }),
  files_markdown: { html: '', truncated: false },
};

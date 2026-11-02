import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import './styles.css';

type Workspace = { root: string; entries: { path: string; directory: boolean }[] };
type Session = { id: string; cwd: string; shell: string };
type Batch = { chunks: number[][]; eof: boolean; exit_code: number | null; error: string | null };
const $ = <T extends HTMLElement = HTMLElement>(selector: string) => document.querySelector<T>(selector)!;
const terminal = new Terminal({ screenReaderMode: true, fontFamily: 'Consolas, monospace', fontSize: 13,
  scrollback: 5000, theme: { background: '#070d17', foreground: '#d9e5f2', cursor: '#f05e97' } });
const fit = new FitAddon();
terminal.loadAddon(fit); terminal.open($('#terminal'));
let session: Session | null = null;
let workspace: Workspace;
let closed = false;
let sizeChain: Promise<void> = Promise.resolve();
let fitTimer = 0;
const encode = new TextEncoder();
const text = () => Array.from({ length: terminal.buffer.active.length }, (_, row) => terminal.buffer.active.getLine(row)?.translateToString(true) || '').join('\n');
Object.defineProperty(window, '__TL1__', { value: { text, sessionId: () => session?.id ?? null,
  size: () => ({ rows: terminal.rows, cols: terminal.cols }) }, configurable: false });
function status(message: unknown) { $('#status').textContent = String(message); }
function scheduleFit() {
  clearTimeout(fitTimer);
  fitTimer = window.setTimeout(() => {
    fit.fit();
    const current = session;
    $('#dimensions').textContent = `${terminal.cols} × ${terminal.rows}`;
    if (current) sizeChain = sizeChain.then(() => invoke<void>('terminal_resize', { id: current.id, rows: terminal.rows, cols: terminal.cols })).catch(status);
  }, 60);
}
new ResizeObserver(scheduleFit).observe($('#terminal'));
terminal.onData(data => { if (session) void invoke('terminal_write', { id: session.id, data: [...encode.encode(data)] }).catch(status); });
// Match the production terminal's non-executable output policy.
for (const code of [0, 1, 2, 7, 8, 9, 52, 777]) terminal.parser.registerOscHandler(code, () => true);
terminal.parser.registerCsiHandler({ final: 't' }, () => true);

async function poll(current: Session) {
  while (!closed && session?.id === current.id) {
    const batch = await invoke<Batch>('terminal_read', { id: current.id });
    for (const chunk of batch.chunks) await new Promise<void>(resolve => terminal.write(new Uint8Array(chunk), resolve));
    if (batch.error) status(batch.error);
    if (batch.eof) { status(`Process exited · ${batch.exit_code ?? 'unknown'}`); break; }
    await new Promise(resolve => setTimeout(resolve, batch.chunks.length ? 0 : 20));
  }
}
$('#start').onclick = async () => {
  $<HTMLButtonElement>('#start').disabled = true;
  try {
    fit.fit(); terminal.clear();
    session = await invoke<Session>('terminal_start', { cwd: workspace.root, rows: terminal.rows, cols: terminal.cols });
    status('Real PTY running · session preserved while panes resize');
    $<HTMLButtonElement>('#stop').disabled = false; terminal.focus();
    void poll(session).catch(status);
  } catch (error) { status(error); $<HTMLButtonElement>('#start').disabled = false; }
};
$('#stop').onclick = async () => {
  const current = session; session = null;
  if (current) await invoke('terminal_stop', { id: current.id }).catch(status);
  $<HTMLButtonElement>('#stop').disabled = true; $<HTMLButtonElement>('#start').disabled = false;
  status('Session stopped');
};
$('#close').onclick = async () => { closed = true; await invoke('terminal_shutdown'); await getCurrentWindow().close(); };

for (const splitter of document.querySelectorAll<HTMLElement>('.splitter')) {
  const parent = splitter.parentElement!;
  let ratio = Number(parent.dataset.ratio);
  const update = (next: number) => {
    ratio = Math.max(15, Math.min(85, next)); parent.style.setProperty('--ratio', String(ratio));
    // A nested split has its own ratio. Set the parent's child weights directly
    // so that a child's custom property cannot change the parent's geometry.
    (parent.children[0] as HTMLElement).style.flex = `${ratio} 1 0`;
    (parent.children[2] as HTMLElement).style.flex = `${100 - ratio} 1 0`;
    splitter.setAttribute('aria-valuenow', String(Math.round(ratio)));
  };
  splitter.setAttribute('aria-valuemin', '15'); splitter.setAttribute('aria-valuemax', '85'); update(ratio);
  splitter.addEventListener('keydown', event => {
    const axis = parent.classList.contains('horizontal') ? ['ArrowLeft', 'ArrowRight'] : ['ArrowUp', 'ArrowDown'];
    if (axis.includes(event.key)) { event.preventDefault(); update(ratio + (event.key === axis[0] ? -5 : 5)); }
  });
  splitter.addEventListener('pointerdown', event => { if (event.button === 0) { splitter.setPointerCapture(event.pointerId); event.preventDefault(); splitter.focus(); } });
  splitter.addEventListener('pointermove', event => {
    if (!splitter.hasPointerCapture(event.pointerId)) return;
    const bounds = parent.getBoundingClientRect();
    update(parent.classList.contains('horizontal') ? 100 * (event.clientX - bounds.left) / bounds.width : 100 * (event.clientY - bounds.top) / bounds.height);
  });
}

// A deliberately small, inert proof renderer. Full Markdown/Mermaid fidelity is TL4/TL6.
function preview(source: string, markdown: boolean) {
  const target = $('#preview'); target.replaceChildren();
  if (!markdown) { const pre = document.createElement('pre'); pre.textContent = source; target.append(pre); return; }
  let code: HTMLPreElement | null = null;
  let list: HTMLUListElement | null = null;
  for (const line of source.split(/\r?\n/)) {
    if (line.startsWith('```')) { if (code) code = null; else { code = document.createElement('pre'); target.append(code); } list = null; continue; }
    if (code) { code.textContent += `${line}\n`; continue; }
    const heading = /^(#{1,6})\s+(.*)$/.exec(line);
    if (heading) { const element = document.createElement(`h${heading[1].length}`); element.textContent = heading[2]; target.append(element); list = null; }
    else if (/^[-*] /.test(line)) { if (!list) { list = document.createElement('ul'); target.append(list); } const item = document.createElement('li'); item.textContent = line.slice(2); list.append(item); }
    else { list = null; if (line.trim()) { const paragraph = document.createElement('p'); paragraph.textContent = line; target.append(paragraph); } }
  }
}
try {
  workspace = await invoke<Workspace>('tl1_files');
  $('#runtime').textContent = 'Embedded Tauri · read-only files · Rust ConPTY';
  for (const entry of workspace.entries) {
    const element = document.createElement(entry.directory ? 'div' : 'button');
    element.textContent = entry.path; element.classList.toggle('directory', entry.directory);
    if (!entry.directory) {
      element.dataset.path = entry.path;
      element.onclick = async () => {
        try {
          const source = await invoke<string>('tl1_read', { relativePath: entry.path });
          $('#source').textContent = source; $('#file-name').textContent = entry.path;
          preview(source, /\.md$/i.test(entry.path));
          for (const item of $('#files').querySelectorAll('[aria-current]')) item.removeAttribute('aria-current');
          element.setAttribute('aria-current', 'true'); status(`Read-only file: ${entry.path}`);
        } catch (error) { status(error); }
      };
    }
    $('#files').append(element);
  }
  $('#files button')?.click(); scheduleFit();
} catch (error) { status(error); $<HTMLButtonElement>('#start').disabled = true; }

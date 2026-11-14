/**
 * Files, folders and images into a terminal. Every harness reads a path, so each becomes one:
 * a dropped or Explorer-copied file is its own path, a clipboard image is saved by the native
 * side under `<home>\pastes` first. The paths go in as one paste, so the harness sees a paste.
 */
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type { Terminal } from '@xterm/xterm';

/** The native side refuses larger images; the page does not read or send them. */
export const IMAGE_LIMIT = 64 * 1024 * 1024;
/** Image types every harness reads; any other image is converted to PNG first. */
const READABLE_IMAGES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp']);

/**
 * Who reads the pasted line. `powershell`: a PowerShell prompt, where double quotes still expand
 * `$` and the backtick, so paths go in single quotes. `agent`: a harness prompt, which reads plain
 * double-quoted paths.
 */
export type PathReader = 'powershell' | 'agent';

/** Paths as the reader takes them: quoted when they hold a space or a character its syntax reads. */
export function formatPaths(paths: readonly string[], reader: PathReader = 'agent'): string {
  const quoted = paths.filter(path => path.length > 0).map(path => {
    if (reader === 'powershell') return /[\s&()[\]{}^=;!'+,`~$#@%|<>"]/.test(path) ? `'${path.replaceAll("'", "''")}'` : path;
    return /[\s&()[\]{}^=;!'+,`~$#@%|<>]/.test(path) ? `"${path}"` : path;
  });
  return quoted.length ? `${quoted.join(' ')} ` : '';
}

export interface TerminalTarget {
  readonly terminal: Terminal; readonly element: HTMLElement; readonly reader: PathReader;
  /** True while the live terminal takes input: running, no modal, no history view over it. */
  accepts(): boolean; focus(): void;
}

/** The live terminal of `target` holds `node`, not an overlay mounted in the same pane. */
function onTerminal(target: TerminalTarget, node: Node | null): boolean {
  return !!node && !!target.terminal.element?.contains(node);
}

/** A PNG copy of an image the harnesses cannot read (BMP, TIFF, ICO...), drawn by the browser. */
async function readable(image: File): Promise<Blob> {
  if (READABLE_IMAGES.has(image.type)) return image;
  const bitmap = await createImageBitmap(image);
  try {
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    canvas.getContext('2d')!.drawImage(bitmap, 0, 0);
    return await canvas.convertToBlob({ type: 'image/png' });
  } finally { bitmap.close(); }
}

/**
 * Ctrl+V without text: copied files paste their paths, a copied image pastes the path of its saved
 * copy. Text keeps going to xterm's own paste. Listens in capture, ahead of xterm's handler.
 */
export function attachTerminalPaste(target: TerminalTarget, notify: (message: unknown, tone?: 'info' | 'error') => void): () => void {
  const onPaste = (event: ClipboardEvent) => {
    const data = event.clipboardData;
    if (!data || !target.accepts() || !onTerminal(target, event.target as Node | null)) return;
    if (data.getData('text/plain').length > 0) return;
    const images = Array.from(data.files).filter(file => file.type.startsWith('image/'));
    if (!images.length && !data.types.includes('Files')) return;
    event.preventDefault(); event.stopImmediatePropagation();
    void (async () => {
      // Explorer's Copy carries real paths; prefer them over the browser's nameless file copies.
      // A clipboard held by another program still leaves the images the browser already has.
      let paths: string[] = [];
      try { paths = await invoke<string[]>('terminal_clipboard_files'); }
      catch (error) { if (!images.length) { notify(error); return; } }
      if (!paths.length && images.length) {
        const large = images.find(image => image.size > IMAGE_LIMIT);
        if (large) { notify('The pasted image is larger than 64 MiB.'); return; }
        try {
          paths = await Promise.all(images.map(async image =>
            invoke<string>('terminal_paste_image', new Uint8Array(await (await readable(image)).arrayBuffer()))));
        } catch (error) { notify(error); return; }
      }
      const text = formatPaths(paths, target.reader);
      if (!text) { notify('The clipboard holds files that are not on disk, such as mail attachments. Save them first, then copy or drag them from Explorer.', 'info'); return; }
      if (target.accepts()) target.terminal.paste(text);
    })();
  };
  target.element.addEventListener('paste', onPaste, true);
  return () => target.element.removeEventListener('paste', onPaste, true);
}

/**
 * Files and folders dropped from Explorer paste their paths into the terminal under the pointer.
 * The window's native drop handler reports them with physical positions.
 */
export async function startTerminalDrop(at: (element: Element | null) => TerminalTarget | null): Promise<() => void> {
  let marked: HTMLElement | null = null;
  const mark = (target: TerminalTarget | null) => {
    const next = target?.element ?? null;
    if (next === marked) return;
    marked?.classList.remove('drop-target'); marked = next; marked?.classList.add('drop-target');
  };
  /** The terminal under the pointer, only where its live screen shows and takes input. */
  const under = (position: { x: number; y: number }) => {
    const ratio = window.devicePixelRatio || 1;
    const node = document.elementFromPoint(position.x / ratio, position.y / ratio);
    const target = at(node);
    return target && target.accepts() && onTerminal(target, node) ? target : null;
  };
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === 'leave') { mark(null); return; }
    const target = under(payload.position);
    if (payload.type !== 'drop') { mark(target); return; }
    mark(null);
    const text = target && formatPaths(payload.paths, target.reader);
    if (!target || !text) return;
    target.focus(); target.terminal.paste(text);
  });
}

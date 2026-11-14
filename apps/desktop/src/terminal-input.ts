/**
 * Files, folders and images into a terminal. Every harness reads a path, so each becomes one:
 * a dropped or Explorer-copied file is its own path, a clipboard image is saved by the native
 * side under `<home>\pastes` first. The paths go in as one paste, so the harness sees a paste.
 */
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import type { Terminal } from '@xterm/xterm';

/** Paths as a shell or a harness prompt reads them: quoted when they hold a space or a quote-worthy character. */
export function formatPaths(paths: readonly string[]): string {
  const quoted = paths.filter(path => path.length > 0).map(path => /[\s&()[\]{}^=;!'+,`~$#@%]/.test(path) ? `"${path}"` : path);
  return quoted.length ? `${quoted.join(' ')} ` : '';
}

export interface TerminalTarget { readonly terminal: Terminal; readonly element: HTMLElement; accepts(): boolean; focus(): void }

/**
 * Ctrl+V without text: copied files paste their paths, a copied image pastes the path of its saved
 * copy. Text keeps going to xterm's own paste. Listens in capture, ahead of xterm's handler.
 */
export function attachTerminalPaste(target: TerminalTarget, notify: (message: unknown) => void): () => void {
  const onPaste = (event: ClipboardEvent) => {
    const data = event.clipboardData;
    if (!data || !target.accepts()) return;
    if (data.getData('text/plain').length > 0) return;
    const images = Array.from(data.files).filter(file => file.type.startsWith('image/'));
    const hasFiles = data.types.includes('Files');
    if (!images.length && !hasFiles) return;
    event.preventDefault(); event.stopImmediatePropagation();
    void (async () => {
      try {
        // Explorer's Copy carries real paths; prefer them over the browser's nameless file copies.
        let paths = await invoke<string[]>('terminal_clipboard_files');
        if (!paths.length) paths = await Promise.all(images.map(async image =>
          invoke<string>('terminal_paste_image', new Uint8Array(await image.arrayBuffer()))));
        const text = formatPaths(paths);
        if (text && target.accepts()) target.terminal.paste(text);
      } catch (error) { notify(error); }
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
    const next = target?.accepts() ? target.element : null;
    if (next === marked) return;
    marked?.classList.remove('drop-target'); marked = next; marked?.classList.add('drop-target');
  };
  const under = (position: { x: number; y: number }) => {
    const ratio = window.devicePixelRatio || 1;
    return at(document.elementFromPoint(position.x / ratio, position.y / ratio));
  };
  return getCurrentWebview().onDragDropEvent(({ payload }) => {
    if (payload.type === 'leave') { mark(null); return; }
    const target = under(payload.position);
    if (payload.type !== 'drop') { mark(target); return; }
    mark(null);
    if (!target?.accepts()) return;
    const text = formatPaths(payload.paths);
    if (!text) return;
    target.focus(); target.terminal.paste(text);
  });
}

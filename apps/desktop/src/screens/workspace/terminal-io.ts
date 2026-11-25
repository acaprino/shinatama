// What a terminal takes besides keys: files, folders and images. The glue between the page and the clipboard and the window's drops; the
// words of it (how paths are written) are `terminal-input.ts`, and the terminal is Sigrid's (`Terminal`, through its handle). No class, no
// style: nothing here draws.
import { terminalsApi } from '../../api/terminals.ts';
import { formatPaths, IMAGE_LIMIT, isReadableImage, type PathReader } from '../../terminal-input.ts';

/** A terminal that takes paths, as the paste and the drop see it. */
export interface InputTarget {
  /** xterm's own element: what a paste or a drop lands on. */
  readonly element: HTMLElement;
  readonly reader: PathReader;
  /** True while the live terminal takes input: running, no dialog, no history view over it. */
  accepts(): boolean;
  /** Types text in as a paste. */
  paste(text: string): void;
  focus(): void;
}

/** A PNG copy of an image the agents cannot read (BMP, TIFF, ICO...), drawn by the browser. */
async function readable(image: File): Promise<Blob> {
  if (isReadableImage(image.type)) return image;
  const bitmap = await createImageBitmap(image);
  try {
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    canvas.getContext('2d')!.drawImage(bitmap, 0, 0);
    return await canvas.convertToBlob({ type: 'image/png' });
  } finally { bitmap.close(); }
}

/**
 * Ctrl+V without text: copied files paste their paths, a copied image pastes the path of its saved copy. Text keeps going to xterm's own
 * paste. Listens in capture, ahead of xterm's handler. Gives back how to stop.
 */
export function attachTerminalPaste(target: InputTarget, say: (message: unknown, error?: boolean) => void): () => void {
  const onPaste = (event: ClipboardEvent) => {
    const data = event.clipboardData;
    if (!data || !target.accepts() || !target.element.contains(event.target as Node | null)) return;
    if (data.getData('text/plain').length > 0) return;
    const images = Array.from(data.files).filter(file => file.type.startsWith('image/'));
    if (!images.length && !data.types.includes('Files')) return;
    event.preventDefault();
    event.stopImmediatePropagation();
    void (async () => {
      // Explorer's Copy carries real paths; prefer them over the browser's nameless file copies. A clipboard held by another program still
      // leaves the images the browser already has.
      let paths: string[] = [];
      try { paths = await terminalsApi.clipboardFiles(); }
      catch (error) { if (!images.length) { say(error, true); return; } }
      if (!paths.length && images.length) {
        if (images.some(image => image.size > IMAGE_LIMIT)) { say('The pasted image is larger than 64 MiB.', true); return; }
        try {
          paths = await Promise.all(images.map(async image => terminalsApi.pasteImage(new Uint8Array(await (await readable(image)).arrayBuffer()))));
        } catch (error) { say(error, true); return; }
      }
      const text = formatPaths(paths, target.reader);
      if (!text) { say('The clipboard holds files that are not on disk, such as mail attachments. Save them first, then copy or drag them from Explorer.'); return; }
      if (target.accepts()) target.paste(text);
    })();
  };
  target.element.addEventListener('paste', onPaste, true);
  return () => target.element.removeEventListener('paste', onPaste, true);
}

/** The terminals that take drops, by the element a drop lands on. */
const targets = new WeakMap<Element, InputTarget>();

/** Lets files dropped on this terminal paste their paths into it. Gives back how to stop. */
export function acceptDrops(target: InputTarget): () => void {
  targets.set(target.element, target);
  return () => { if (targets.get(target.element) === target) targets.delete(target.element); };
}

/**
 * Files and folders dropped from Explorer paste their paths into the terminal under the pointer. The window's native drop handler reports
 * them with physical positions. Gives back how to stop (a promise: the listener is made when the window answers).
 */
export function startTerminalDrops(): Promise<() => void> {
  return terminalsApi.onDrop(({ paths, position }) => {
    const ratio = window.devicePixelRatio || 1;
    const node = document.elementFromPoint(position.x / ratio, position.y / ratio);
    for (let at: Element | null = node; at; at = at.parentElement) {
      const target = targets.get(at);
      if (!target) continue;
      const text = formatPaths(paths, target.reader);
      if (target.accepts() && text) { target.focus(); target.paste(text); }
      return;
    }
  });
}

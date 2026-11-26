import test from 'node:test';
import assert from 'node:assert/strict';
import { cellStyle, createArchive, createTerminalHistory, serializeLine, windowStart, windowTitle, WINDOW_LINES, type HistorySource } from '../src/terminal-history.ts';

type Fake = { chars: string; width?: number; bold?: boolean; fg?: number; fgRgb?: boolean; bg?: number; bgRgb?: boolean };
const cell = (value: Fake) => ({
  getChars: () => value.chars, getWidth: () => value.width ?? 1, getCode: () => 0,
  isBold: () => Number(!!value.bold), isDim: () => 0, isItalic: () => 0, isUnderline: () => 0, isBlink: () => 0,
  isInverse: () => 0, isInvisible: () => 0, isStrikethrough: () => 0, isOverline: () => 0,
  isFgRGB: () => !!value.fgRgb, isBgRGB: () => !!value.bgRgb,
  isFgPalette: () => value.fg !== undefined && !value.fgRgb, isBgPalette: () => value.bg !== undefined,
  isFgDefault: () => value.fg === undefined, isBgDefault: () => value.bg === undefined,
  getFgColor: () => value.fg ?? 0, getBgColor: () => value.bg ?? 0,
  getFgColorMode: () => 0, getBgColorMode: () => 0, isAttributeDefault: () => value.fg === undefined && !value.bold,
});
const line = (cells: Fake[], wrapped = false) => ({
  isWrapped: wrapped, length: cells.length,
  getCell: (x: number) => cells[x] && cell(cells[x]),
  translateToString: () => cells.map(value => value.chars).join(''),
}) as never;

test('a cell keeps its attributes and every color mode as SGR', () => {
  assert.equal(cellStyle(cell({ chars: 'a' }) as never), '');
  assert.equal(cellStyle(cell({ chars: 'a', bold: true, fg: 1 }) as never), ';1;31');
  assert.equal(cellStyle(cell({ chars: 'a', fg: 9 }) as never), ';91');
  assert.equal(cellStyle(cell({ chars: 'a', fg: 200 }) as never), ';38;5;200');
  assert.equal(cellStyle(cell({ chars: 'a', fg: 0x102030, fgRgb: true }) as never), ';38;2;16;32;48');
  assert.equal(cellStyle(cell({ chars: 'a', bg: 4 }) as never), ';44');
});

test('a line is serialized once per style change, without its trailing blanks', () => {
  const cells: Fake[] = [{ chars: 'o', fg: 2 }, { chars: 'k', fg: 2 }, { chars: ' ' }, { chars: '!' }, { chars: ' ' }, { chars: '' }];
  const staged = serializeLine(line(cells), 6, cell({ chars: '' }) as never);
  assert.equal(staged.text, '\x1b[0;32mok\x1b[0m !');
  assert.equal(staged.width, 4);
  const wide = serializeLine(line([{ chars: '漢', width: 2 }, { chars: '', width: 0 }, { chars: 'x' }]), 3, cell({ chars: '' }) as never);
  assert.equal(wide.text, '漢x'); assert.equal(wide.width, 3);
});

test('the archive joins wrapped lines, fills whole chunks and drops the oldest past its cap', async () => {
  const archive = createArchive(2, 4);
  archive.add({ text: 'one', width: 3, wrapped: false });
  archive.add({ text: '-more', width: 5, wrapped: true });
  assert.equal(archive.lines(), 1); assert.equal(archive.width(0), 8);
  for (const text of ['two', 'three', 'four', 'five', 'six']) archive.add({ text, width: text.length, wrapped: false });
  assert.equal(archive.dropped(), 2);
  assert.deepEqual(await archive.read(0, archive.lines()), ['three', 'four', 'five', 'six']);
  assert.deepEqual(await archive.read(1, 3), ['four', 'five']);
  archive.add({ text: 'seven', width: 5, wrapped: false });
  assert.deepEqual(await archive.read(3, 5), ['six', 'seven']);
});

test('a history window stays inside the archive', () => {
  assert.equal(windowStart(10, 500), 0);
  assert.equal(windowStart(5000, 10_000), 4000);
  assert.equal(windowStart(9999, 10_000), 8000);
});

test('a background in true colour is kept too', () => {
  assert.equal(cellStyle(cell({ chars: 'a', bg: 0x0a0b0c, bgRgb: true }) as never), ';48;2;10;11;12');
});

test('the title of the history window says which lines are shown, and how many older ones were not kept', () => {
  assert.equal(windowTitle(0, 2000, 2500, 0), 'Earlier lines 1–2,000 of 2,500');
  assert.match(windowTitle(500, 2000, 2500, 120), /older not kept/);
  assert.equal(WINDOW_LINES, 2000);
});

/** A terminal that holds lines the way xterm does: a buffer that drops its oldest line past a limit, with a marker that follows its line. */
function fakeTerminal(limit: number, rows: number) {
  const lines: string[] = [];
  const markers = new Set<{ line: number; isDisposed: boolean; dispose(): void; offset: number }>();
  let baseY = 0;
  const terminal = {
    cols: 40,
    buffer: {
      active: { type: 'normal' },
      normal: {
        get baseY() { return baseY; }, get cursorY() { return Math.min(rows - 1, lines.length - 1 - baseY); }, get length() { return lines.length; },
        getLine: (index: number) => lines[index] === undefined ? undefined : line([...lines[index]].map(chars => ({ chars }))),
        getNullCell: () => cell({ chars: '' }),
      },
    },
    registerMarker() {
      const marker = { line: Math.max(0, lines.length - 1), isDisposed: false, dispose() { marker.isDisposed = true; markers.delete(marker); }, offset: 0 };
      markers.add(marker);
      return marker;
    },
  } as unknown as HistorySource;
  return {
    terminal,
    print(text: string) {
      lines.push(text);
      baseY = Math.max(0, lines.length - rows);
      while (lines.length > limit) {
        lines.shift(); baseY = Math.max(0, lines.length - rows);
        for (const marker of markers) marker.line -= 1;
      }
      for (const marker of markers) if (marker.line < 0) marker.isDisposed = true;
    },
  };
}

test('what scrolls out of the live buffer is kept, in order, and what is still there is not copied twice', async () => {
  const { terminal, print } = fakeTerminal(10, 4);
  const history = createTerminalHistory(terminal);
  for (let n = 1; n <= 6; n++) { print(`line ${n}`); history.capture(); }
  // Nothing has left the live buffer yet.
  assert.equal(history.archive.lines(), 0);
  for (let n = 7; n <= 25; n++) { print(`line ${n}`); history.capture(); }
  const kept = await history.archive.read(0, history.archive.lines());
  assert.ok(kept.length > 0);
  assert.equal(kept[0], 'line 1');
  assert.deepEqual(kept, kept.map((_, index) => `line ${index + 1}`));
  history.dispose();
});

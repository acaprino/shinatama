import test from 'node:test';
import assert from 'node:assert/strict';
import { cellStyle, createArchive, serializeLine, splitOutput, windowStart } from '../src/terminal-history.ts';
import { archivedLineAt, holdAt, LINE_PITCH, lineAt, minimapGeometry } from '../src/terminal-minimap.ts';

type Fake = { chars: string; width?: number; bold?: boolean; fg?: number; fgRgb?: boolean; bg?: number };
const cell = (value: Fake) => ({
  getChars: () => value.chars, getWidth: () => value.width ?? 1, getCode: () => 0,
  isBold: () => Number(!!value.bold), isDim: () => 0, isItalic: () => 0, isUnderline: () => 0, isBlink: () => 0,
  isInverse: () => 0, isInvisible: () => 0, isStrikethrough: () => 0, isOverline: () => 0,
  isFgRGB: () => !!value.fgRgb, isBgRGB: () => false,
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

test('the minimap gives the archive a capped share and maps the frame both ways', () => {
  const none = minimapGeometry(400, 0, 1024, 0, 24);
  assert.equal(none.archiveHeight, 0); assert.equal(none.liveHeight, 400);
  const some = minimapGeometry(400, 90_000, 1024, 1000, 24);
  assert.equal(some.archiveHeight, 120); assert.equal(some.liveHeight, 280);
  assert.ok(some.frameTop >= 120 && some.frameTop + some.frameHeight <= 400);
  assert.equal(lineAt(0, some, 1024, 24), 0);
  assert.equal(lineAt(400, some, 1024, 24), 1000);
  // A few archived lines take a few pixels, not a third of the strip.
  assert.equal(minimapGeometry(400, 10, 100, 0, 40).archiveHeight, 30);
  assert.equal(archivedLineAt(0, some, 90_000), 0);
  assert.equal(archivedLineAt(119, some, 90_000), 89_250);
});

test('a short scrollback keeps its lines a few pixels apart instead of stretching them', () => {
  const fresh = minimapGeometry(700, 0, 43, 0, 43);
  assert.equal(fresh.pitch, LINE_PITCH);
  assert.deepEqual([fresh.frameTop, fresh.frameHeight], [0, 43 * LINE_PITCH]);
  const scrolled = minimapGeometry(700, 0, 443, 400, 43);
  assert.ok(scrolled.pitch < LINE_PITCH && scrolled.pitch > 1);
  assert.equal(scrolled.frameTop + scrolled.frameHeight, 700);
  // A click below what the strip holds goes to the end; at the top, to the start.
  assert.equal(lineAt(699, fresh, 43, 43), 0);
  assert.equal(lineAt(699, scrolled, 443, 43), 400);
  assert.equal(lineAt(0, scrolled, 443, 43), 0);
});

test('a drag that takes the frame moves it without a jump', () => {
  const shape = minimapGeometry(600, 0, 1043, 500, 43);
  const y = shape.frameTop + 3; const hold = holdAt(y, shape, 500);
  assert.equal(lineAt(y, shape, 1043, 43, hold), 500);
  assert.equal(lineAt(y + 10 * shape.pitch, shape, 1043, 43, hold), 510);
  // A click elsewhere centers the frame on the pointer: the frame's middle stands for where it is.
  assert.ok(Math.abs(lineAt(shape.frameTop + shape.frameHeight / 2, shape, 1043, 43) - 500) <= 1);
});

test('output is cut after every few newlines and nothing is lost', () => {
  const bytes = new TextEncoder().encode('a\nb\nc\nd\ne');
  const parts = splitOutput(bytes, 2).map(part => new TextDecoder().decode(part));
  assert.deepEqual(parts, ['a\nb\n', 'c\nd\n', 'e']);
  assert.deepEqual(splitOutput(new Uint8Array(0)).map(part => part.length), [0]);
  assert.equal(splitOutput(new TextEncoder().encode('x\n'), 1).length, 1);
});

test('a history window stays inside the archive', () => {
  assert.equal(windowStart(10, 500), 0);
  assert.equal(windowStart(5000, 10_000), 4000);
  assert.equal(windowStart(9999, 10_000), 8000);
});

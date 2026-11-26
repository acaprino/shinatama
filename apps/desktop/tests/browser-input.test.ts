import test from 'node:test';
import assert from 'node:assert/strict';
import { coalesce, keyEvent, mouseEvent, settle, wheelEvent } from '../src/browser-input.ts';

const pointer = { clientX: 110, clientY: 50, button: 0, buttons: 1, detail: 1, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
const rect = { left: 10, top: 20 };

test('a click maps to page pixels of the canvas box', () => {
  assert.deepEqual(mouseEvent('pressed', pointer, rect), { type: 'mouse', kind: 'pressed', x: 100, y: 30, button: 'left', buttons: 1, modifiers: 0, clickCount: 1 });
  assert.deepEqual(mouseEvent('released', { ...pointer, button: 2, buttons: 0 }, rect).button, 'right');
  assert.equal(mouseEvent('pressed', { ...pointer, detail: 7 }, rect).clickCount, 3);
});

test('a move carries no click count and reports a held left button', () => {
  const moved = mouseEvent('moved', pointer, rect);
  assert.equal(moved.type === 'mouse' && moved.clickCount, 0);
  assert.equal(moved.type === 'mouse' && moved.button, 'left');
  assert.equal(mouseEvent('moved', { ...pointer, buttons: 0 }, rect).type === 'mouse' && (mouseEvent('moved', { ...pointer, buttons: 0 }, rect) as { button: string }).button, 'none');
});

test('ctrl+a keeps its modifiers and sends no text', () => {
  const key = { key: 'a', code: 'KeyA', keyCode: 65, altKey: false, ctrlKey: true, metaKey: false, shiftKey: false };
  assert.deepEqual(keyEvent('down', key), { type: 'key', kind: 'down', key: 'a', code: 'KeyA', text: null, modifiers: 2, keyCode: 65 });
});

test('a printable key has text on keydown only; Enter types a carriage return', () => {
  const key = { key: 'é', code: 'Quote', keyCode: 222, altKey: false, ctrlKey: false, metaKey: false, shiftKey: false };
  assert.equal((keyEvent('down', key) as { text: string | null }).text, 'é');
  assert.equal((keyEvent('up', key) as { text: string | null }).text, null);
  assert.equal((keyEvent('down', { ...key, key: 'Enter', code: 'Enter', keyCode: 13 }) as { text: string | null }).text, '\r');
});

test('wheel keeps deltas, lines become pixels', () => {
  assert.deepEqual(wheelEvent({ ...pointer, deltaX: 0, deltaY: 120, deltaMode: 0 }, rect), { type: 'wheel', x: 100, y: 30, dx: 0, dy: 120, modifiers: 0 });
  assert.equal((wheelEvent({ ...pointer, deltaX: 0, deltaY: 3, deltaMode: 1 }, rect) as { dy: number }).dy, 120);
});

test('resize settles to one metrics call with the last size', () => {
  const pending: Array<() => void> = []; const calls: number[] = [];
  const timers = { set: (fn: () => void) => { pending.push(fn); return pending.length - 1; }, clear: (h: unknown) => { pending[h as number] = () => {}; } };
  const resize = settle<number>(150, width => calls.push(width), timers);
  for (let width = 300; width < 310; width++) resize(width);
  pending.forEach(fn => fn());
  assert.deepEqual(calls, [309]);
});

test('moves coalesce to the last, other events keep their order', () => {
  const move = (x: number) => mouseEvent('moved', { ...pointer, clientX: x }, rect);
  const down = mouseEvent('pressed', pointer, rect);
  const out = coalesce([move(11), move(12), down, move(13), move(14)]);
  assert.deepEqual(out.map(e => e.type === 'mouse' ? `${e.kind}:${e.x}` : e.type), ['moved:2', 'pressed:100', 'moved:4']);
});

test('what the frame surface tells is the same input as the canvas gave, with the point already in the box', async () => {
  const { frameKeyInput, framePointerInput, frameWheelInput, keepsForApp } = await import('../src/browser-input.ts');
  const held = { altKey: false, ctrlKey: false, metaKey: false, shiftKey: true };
  assert.deepEqual(framePointerInput({ kind: 'down', x: 12, y: 34, button: 2, buttons: 2, detail: 2, ...held }), { type: 'mouse', kind: 'pressed', x: 12, y: 34, button: 'right', buttons: 2, modifiers: 8, clickCount: 2 });
  assert.deepEqual(framePointerInput({ kind: 'move', x: 1, y: 2, button: 0, buttons: 1, detail: 0, ...held }), { type: 'mouse', kind: 'moved', x: 1, y: 2, button: 'left', buttons: 1, modifiers: 8, clickCount: 0 });
  assert.equal(framePointerInput({ kind: 'up', x: 1, y: 2, button: 0, buttons: 0, detail: 1, ...held }).type, 'mouse');
  assert.deepEqual(frameWheelInput({ x: 5, y: 6, deltaX: 0, deltaY: 3, deltaMode: 1, ...held }), { type: 'wheel', x: 5, y: 6, dx: 0, dy: 120, modifiers: 8 });
  assert.deepEqual(frameKeyInput({ kind: 'down', key: 'a', code: 'KeyA', keyCode: 65, ...held }), { type: 'key', kind: 'down', key: 'a', code: 'KeyA', text: 'a', modifiers: 8, keyCode: 65 });
  assert.equal(keepsForApp({ key: 'Tab', ctrlKey: true }), true);
  assert.equal(keepsForApp({ key: 'Tab', ctrlKey: false }), false);
});

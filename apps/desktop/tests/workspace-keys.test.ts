// The keys of the window that are Shinatama's own, and which of them a terminal leaves to the window (src/keys.ts).
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { commandOfKey, terminalLeavesKey, type KeyLike } from '../src/keys.ts';

const key = (name: string, mods: Partial<KeyLike> = {}): KeyLike => ({ key: name, ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...mods });

test('a chord of the table is the command it names, a letter in either case', () => {
  assert.equal(commandOfKey(key('T', { ctrlKey: true, shiftKey: true })), 'new-session');
  assert.equal(commandOfKey(key('t', { ctrlKey: true, shiftKey: true })), 'new-session');
  assert.equal(commandOfKey(key('ArrowLeft', { ctrlKey: true, shiftKey: true })), 'pane-left');
  assert.equal(commandOfKey(key('F', { ctrlKey: true, shiftKey: true })), 'find-in-terminal');
  assert.equal(commandOfKey(key('F3')), 'open-folder');
  // Cmd is Ctrl on a Mac.
  assert.equal(commandOfKey(key('T', { metaKey: true, shiftKey: true })), 'new-session');
});

test('a key that is not a chord of the table is no command: it is the program\'s', () => {
  assert.equal(commandOfKey(key('t', { ctrlKey: true })), null);
  assert.equal(commandOfKey(key('t', { ctrlKey: true, shiftKey: true, altKey: true })), null);
  assert.equal(commandOfKey(key('c', { ctrlKey: true })), null);
  assert.equal(commandOfKey(key('k', { ctrlKey: true })), null, 'Ctrl+K is the program\'s inside a terminal');
});

test('a terminal leaves to the window its chords, F6 (so the keyboard is never trapped) and the zoom', () => {
  assert.equal(terminalLeavesKey(key('ArrowRight', { ctrlKey: true, shiftKey: true })), true);
  assert.equal(terminalLeavesKey(key('F6')), true);
  assert.equal(terminalLeavesKey(key('F6', { shiftKey: true })), true);
  for (const zoom of ['+', '=', '-', '0']) assert.equal(terminalLeavesKey(key(zoom, { ctrlKey: true })), true, `Ctrl+${zoom}`);
  assert.equal(terminalLeavesKey(key('a')), false);
  assert.equal(terminalLeavesKey(key('c', { ctrlKey: true })), false);
  assert.equal(terminalLeavesKey(key('k', { ctrlKey: true })), false);
  assert.equal(terminalLeavesKey(key('f', { ctrlKey: true })), false, 'Ctrl+F is the program\'s: less, vim');
});

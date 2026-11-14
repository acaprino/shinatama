// The rules that keep the desktop app from behaving like a web page, checked without running
// it (the tauri-native-desktop skill; native_frame.rs and desktop-shell.ts implement them).
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { confirmsField, keepsContextMenu, keepsSelectAll, nextZoom, savedZoom, zoomCommand, ZOOM_STEPS } from '../src/desktop-shell-model.ts';

const src = new URL('../src/', import.meta.url);
const files = readdirSync(src).filter(name => /\.(ts|css)$/.test(name));
const read = (name: string) => readFileSync(new URL(name, src), 'utf8');
const rust = new URL('../src-tauri/src/', import.meta.url);
const readRust = (name: string) => readFileSync(new URL(name, rust), 'utf8');
const key = (key: string, code: string, extra: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean; metaKey: boolean }> = {}) =>
  ({ key, code, ctrlKey: true, shiftKey: false, altKey: false, metaKey: false, ...extra });

test('chrome uses the arrow cursor: no hand outside links in rendered content', () => {
  // A link inside a previewed document is content; the engine's hand is kept there.
  const allowed = ['.files-preview a[data-path]'];
  for (const name of files.filter(name => name.endsWith('.css'))) {
    for (const rule of read(name).split('}')) {
      if (!/cursor:\s*pointer/.test(rule)) continue;
      assert.ok(allowed.some(selector => rule.includes(selector)), `${name}: hand cursor in ${rule.trim().slice(0, 80)}`);
    }
  }
  for (const name of [...files.filter(name => name.endsWith('.ts')), '../index.html', '../widget.html']) {
    assert.doesNotMatch(read(name), /cursor-pointer|style\.cursor\s*=\s*['"]pointer/, name);
  }
});

test('the interface is not selectable text; fields, panels and code are', () => {
  const css = read('styles.css');
  assert.match(css, /html \{ cursor: default; -webkit-user-select: none; user-select: none; \}/);
  for (const content of ['input', 'textarea', 'dialog', 'pre', 'code', '.cm-editor', '#notice > span']) {
    assert.match(css, new RegExp(`${content.replace(/[.*+?^${}()|[\]\\>]/g, '\\$&')}[^{]*\\{ -webkit-user-select: text; user-select: text; \\}`), content);
  }
});

test('every Enter that confirms a field leaves IME composition alone', () => {
  // Enter on buttons, tree rows and the browser tab's key mapping is not a field confirmation.
  const exempt = new Set(['files-tree.ts', 'new-session.ts', 'browser-input.ts', 'desktop-shell-model.ts']);
  for (const name of files.filter(name => name.endsWith('.ts') && !exempt.has(name))) {
    assert.doesNotMatch(read(name), /key\s*[!=]==\s*'Enter'/, `${name}: use confirmsField(event)`);
  }
});

test('no page closes its window with window.close(), which bypasses CloseRequested', () => {
  for (const name of files.filter(name => name.endsWith('.ts'))) assert.doesNotMatch(read(name), /\bwindow\.close\(\)/, name);
});

test('every window is built by the shared bootstrap', () => {
  const recovery = readRust('webview_recovery.rs');
  for (const step of ['.on_navigation(crate::native_frame::navigation_policy(app))', '.on_new_window(', 'crate::native_frame::prepare(&window)', 'SetIsBuiltInErrorPageEnabled(false)']) {
    assert.ok(recovery.includes(step), `webview_recovery::build lacks ${step}`);
  }
  for (const name of readdirSync(rust, { recursive: true }).map(String).filter(name => name.endsWith('.rs') && name !== 'webview_recovery.rs')) {
    const source = readRust(name);
    for (const match of source.matchAll(/WebviewWindowBuilder::/g)) {
      const before = source.slice(Math.max(0, match.index - 120), match.index);
      assert.match(before, /#\[cfg\(not\(windows\)\)\]/, `${name}: a window built outside webview_recovery::build`);
    }
  }
});

test('the navigation policy compares the origin exactly', () => {
  const policy = readRust('native_frame.rs');
  assert.match(policy, /url\.host_str\(\) == Some\("tauri\.localhost"\)/);
  assert.doesNotMatch(policy, /starts_with\("http/);
});

test('zoom keys follow Ctrl+= / Ctrl+- / Ctrl+0 on any layout', () => {
  assert.equal(zoomCommand(key('=', 'Equal')), 'in');
  assert.equal(zoomCommand(key('+', 'BracketRight', { shiftKey: true })), 'in'); // Italian layout
  assert.equal(zoomCommand(key('+', 'NumpadAdd')), 'in');
  assert.equal(zoomCommand(key('-', 'Slash')), 'out'); // Italian layout
  assert.equal(zoomCommand(key('-', 'NumpadSubtract')), 'out');
  assert.equal(zoomCommand(key('0', 'Digit0')), 'reset');
  assert.equal(zoomCommand(key('_', 'Minus', { shiftKey: true })), null);
  assert.equal(zoomCommand(key('0', 'Digit0', { altKey: true })), null);
  assert.equal(zoomCommand({ ...key('=', 'Equal'), ctrlKey: false }), null);
});

test('zoom reaches 200% and returns to 100%', () => {
  assert.equal(ZOOM_STEPS.at(-1), 2);
  let level = 1;
  for (let i = 0; i < 20; i++) level = nextZoom(level, 'in');
  assert.equal(level, 2);
  assert.equal(nextZoom(2, 'in'), 2);
  assert.equal(nextZoom(1.3, 'out'), 1.25);
  assert.equal(nextZoom(1.75, 'reset'), 1);
  assert.equal(savedZoom('1.5'), 1.5);
  assert.equal(savedZoom('7'), 1);
  assert.equal(savedZoom(null), 1);
});

test('the engine menu and Select All stay in fields and over selected text only', () => {
  const inside = (selector: string) => ({ closest: (query: string) => (query.includes(selector) ? {} : null) });
  const chrome = { closest: () => null };
  assert.equal(keepsContextMenu(inside('textarea'), false), true);
  assert.equal(keepsContextMenu(inside('.xterm'), false), true);
  assert.equal(keepsContextMenu(chrome, true), true);
  assert.equal(keepsContextMenu(chrome, false), false);
  assert.equal(keepsContextMenu(null, false), false);
  assert.equal(keepsSelectAll(inside('input')), true);
  assert.equal(keepsSelectAll(chrome), false);
});

test('an Enter that picks an IME candidate does not confirm', () => {
  assert.equal(confirmsField({ key: 'Enter', isComposing: false, keyCode: 13 }), true);
  assert.equal(confirmsField({ key: 'Enter', isComposing: true, keyCode: 13 }), false);
  assert.equal(confirmsField({ key: 'Enter', isComposing: false, keyCode: 229 }), false);
  assert.equal(confirmsField({ key: 'a', isComposing: false, keyCode: 65 }), false);
});

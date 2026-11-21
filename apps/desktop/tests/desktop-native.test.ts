// The rules that keep the desktop app from behaving like a web page, checked without running
// it (the tauri-native-desktop skill; native_frame.rs and desktop-shell.ts implement them).
import assert from 'node:assert/strict';
import { readdirSync, readFileSync } from 'node:fs';
import { test } from 'node:test';
import { clampZoom, guardDesktop, stepZoom, zoomDirection } from '@acaprino/imago/shell';
import { confirmsField, ENGINE_SURFACES, keepsSelectAll, ZOOM_STEPS } from '../src/desktop-shell-model.ts';

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
  const css = readFileSync(new URL(import.meta.resolve('@acaprino/imago/css/base.css')), 'utf8');
  assert.match(css, /\.im-shell\s*\{[^}]*user-select:\s*none/);
  for (const selector of ['.im-content-text', 'input', 'textarea', '[contenteditable="true"]']) {
    assert.ok(css.split('user-select: text')[0].includes(selector), selector);
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
  for (const step of ['guard_navigation(', 'watch_process_failures(window', 'crate::native_frame::prepare(&window)']) {
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

test('only development adds its configured server to Imago’s embedded origin guard', () => {
  const recovery = readRust('webview_recovery.rs');
  assert.match(recovery, /dev_url\s*\.as_ref\(\)\s*\.filter\(\|_\| tauri::is_dev\(\)\)/);
  assert.match(recovery, /guard_navigation\(\s*tauri::WebviewWindowBuilder::from_config\(app, config\)\?,\s*&origins/);
});

test('the main frame uses Imago’s snap overlay and save-before-reload menu', () => {
  const frame = readRust('native_frame.rs');
  assert.match(frame, /if !tauri::is_dev\(\) \{\s*let _ = disable_browser_accelerators\(window\)/);
  assert.match(frame, /if window.label\(\) == "main" \{\s*let overlay = install_snap_layouts\(window\)/);
  assert.match(frame, /install_reload_menu\(window, "Reload interface"/);
  assert.match(frame, /overlay\.set_rect\(x, y, width, height\)/);
  assert.match(frame, /"interface-reload-requested", token/);
  assert.match(frame, /if take_pending\(token\) \{\s*let _ = main.reload\(\)/);
});

// The guard as desktop-shell.ts mounts it, on a document that only records its listeners.
function guarded(selection: { isCollapsed: boolean; containsNode: () => boolean } | null = null) {
  const listeners = new Map<string, (event: object) => void>();
  const doc = { addEventListener: (type: string, listener: (event: object) => void) => { listeners.set(type, listener); },
    removeEventListener() {}, getSelection: () => selection } as unknown as Document;
  guardDesktop(doc, { allowReloadIn: ENGINE_SURFACES });
  return (type: string, event: object): boolean => {
    let refused = false;
    listeners.get(type)!({ ...event, preventDefault() { refused = true; } });
    return refused;
  };
}
const inside = (selector: string) => ({ closest: (query: string) => (query.includes(selector) ? {} : null) });
const chrome = { closest: () => null };

test('zoom keys follow Ctrl+= / Ctrl+- / Ctrl+0 on any layout', () => {
  assert.equal(zoomDirection(key('=', 'Equal')), 1);
  assert.equal(zoomDirection(key('+', 'BracketRight', { shiftKey: true })), 1); // Italian layout
  assert.equal(zoomDirection(key('+', 'NumpadAdd')), 1);
  assert.equal(zoomDirection(key('-', 'Slash')), -1); // Italian layout
  assert.equal(zoomDirection(key('-', 'NumpadSubtract')), -1);
  assert.equal(zoomDirection(key('0', 'Digit0')), 0);
  assert.equal(zoomDirection(key('_', 'Minus', { shiftKey: true })), null);
  assert.equal(zoomDirection(key('0', 'Digit0', { altKey: true })), null);
  assert.equal(zoomDirection({ ...key('=', 'Equal'), ctrlKey: false }), null);
});

test('zoom steps through the app levels to 200%, and a saved older level still opens', () => {
  assert.equal(ZOOM_STEPS.at(-1), 2);
  let level = 1;
  for (let i = 0; i < 20; i++) level = stepZoom(level, 1, ZOOM_STEPS);
  assert.equal(level, 2);
  assert.equal(stepZoom(2, 1, ZOOM_STEPS), 2);
  assert.equal(stepZoom(1.3, -1, ZOOM_STEPS), 1.25);
  assert.equal(stepZoom(1.75, 0, ZOOM_STEPS), 1);
  // shinatama.zoom as earlier versions saved it: 67, 75, 125 and 175% are kept, anything else is 100%.
  for (const saved of ['0.67', '0.75', '1.25', '1.75', '1.5']) assert.equal(clampZoom(Number(saved), ZOOM_STEPS), Number(saved));
  assert.equal(clampZoom(Number('7'), ZOOM_STEPS), 1);
  assert.equal(clampZoom(Number(null), ZOOM_STEPS), 1);
});

test('the engine menu stays in fields, terminals and over selected text only; Select All in fields', () => {
  const menu = (target: object | null, selected = false) =>
    guarded({ isCollapsed: !selected, containsNode: () => selected })('contextmenu', { target });
  assert.equal(menu(inside('textarea')), false);
  assert.equal(menu(inside('.xterm')), false);
  assert.equal(menu(chrome, true), false);
  assert.equal(menu(chrome), true);
  assert.equal(menu(null), true);
  assert.equal(keepsSelectAll(inside('input')), true);
  assert.equal(keepsSelectAll(chrome), false);
});

test('an Enter that picks an IME candidate does not confirm', () => {
  assert.equal(confirmsField({ key: 'Enter', isComposing: false, keyCode: 13 }), true);
  assert.equal(confirmsField({ key: 'Enter', isComposing: true, keyCode: 13 }), false);
  assert.equal(confirmsField({ key: 'Enter', isComposing: false, keyCode: 229 }), false);
  assert.equal(confirmsField({ key: 'a', isComposing: false, keyCode: 65 }), false);
});

test('browser keys are refused, editing and app keys are not, and terminals keep theirs', () => {
  const k = (key: string, extra: Partial<{ ctrlKey: boolean; shiftKey: boolean; altKey: boolean }> = {}) =>
    ({ key, code: '', ctrlKey: false, shiftKey: false, altKey: false, metaKey: false, ...extra });
  const refused = guarded();
  for (const event of [k('F5'), k('F5', { ctrlKey: true }), k('r', { ctrlKey: true }), k('R', { ctrlKey: true, shiftKey: true }), k('p', { ctrlKey: true }),
    k('f', { ctrlKey: true }), k('u', { ctrlKey: true }), k('ArrowLeft', { altKey: true }), k('F12'), k('F7'), k('BrowserBack')]) {
    assert.equal(refused('keydown', { ...event, target: chrome }), true, JSON.stringify(event));
  }
  for (const event of [k('c', { ctrlKey: true }), k('v', { ctrlKey: true }), k('x', { ctrlKey: true }), k('z', { ctrlKey: true }), k('a', { ctrlKey: true }),
    k('F', { ctrlKey: true, shiftKey: true }), k('T', { ctrlKey: true, shiftKey: true }), k('ArrowLeft', { ctrlKey: true, shiftKey: true }), k('F3'), k('r'), k('ArrowLeft')]) {
    assert.equal(refused('keydown', { ...event, target: chrome }), false, JSON.stringify(event));
  }
  for (const surface of ['.xterm', '.browser-canvas', '.cm-editor']) {
    assert.equal(refused('keydown', { ...k('r', { ctrlKey: true }), target: inside(surface) }), false, surface);
  }
  assert.equal(refused('mouseup', { button: 3 }), true);
});

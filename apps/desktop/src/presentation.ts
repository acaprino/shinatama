import { invoke } from '@tauri-apps/api/core';
import type { ITheme, ITerminalOptions, Terminal } from '@xterm/xterm';
import { openModal } from './modal.ts';

const base = {
  stage: '#04070f', chrome: '#070c18', 'chrome-2': '#0a1120', surface: '#0e1729', 'surface-2': '#13203a', terminal: '#050912',
  line: '#1c3563', 'line-strong': '#4a78e6', ice: '#a9c8ff', text: '#e6f0ff', 'text-2': '#b4c8e8', muted: '#8fa6c9',
  accent: '#f48fb8', 'accent-soft': '#ffb3d0', 'accent-ink': '#1c0811', live: '#6fe3d2', warning: '#e8bb72', error: '#ff8495',
  danger: '#a0294e', 'danger-line': '#e35f87', 'on-danger': '#ffffff', selection: '#2a4a8f', scrim: '#02050bcc', shadow: '#000000aa', grid: '#1c356340',
} as const;
type Tokens = { readonly [K in keyof typeof base]: string };
export const SKINS: Readonly<Record<string, { readonly name: string; readonly tokens: Tokens }>> = {
  shinatama: { name: 'Shinatama', tokens: base },
  neutral: { name: 'Neutral dark', tokens: { ...base, stage: '#080b10', chrome: '#0d1219', 'chrome-2': '#121a24', surface: '#17212e', 'surface-2': '#1d2a3a', terminal: '#090e15', line: '#334256', 'line-strong': '#899bb6', ice: '#d5e1f4', accent: '#b8c9e3', 'accent-soft': '#dbe7fa', 'accent-ink': '#0a1220', grid: '#33425640' } },
  section9: { name: 'Section 9', tokens: { ...base, stage: '#030b0c', chrome: '#061214', 'chrome-2': '#091b1d', surface: '#0c2225', 'surface-2': '#103035', terminal: '#041012', line: '#24545a', 'line-strong': '#62aeb6', ice: '#a8edf2', text: '#e4f8f6', 'text-2': '#bededb', muted: '#a4c9c6', accent: '#78e2dc', 'accent-soft': '#b4fff8', 'accent-ink': '#032021', selection: '#235e62', grid: '#24545a40' } },
  tctf: { name: 'TCTF', tokens: { ...base, stage: '#090b0e', chrome: '#10151b', 'chrome-2': '#171e26', surface: '#1b2631', 'surface-2': '#24313e', terminal: '#0a1016', line: '#455567', 'line-strong': '#b59b78', ice: '#f6d9b3', text: '#f5ece0', 'text-2': '#d8cab8', muted: '#c7baa7', accent: '#ffc072', 'accent-soft': '#ffdfad', 'accent-ink': '#241405', selection: '#635035', grid: '#45556740' } },
  sumi: { name: 'Sumi', tokens: { ...base, stage: '#080707', chrome: '#100c0c', 'chrome-2': '#171111', surface: '#211919', 'surface-2': '#2b2020', terminal: '#0a0808', line: '#573a37', 'line-strong': '#c88b81', ice: '#f3d5cd', text: '#f7e9e3', 'text-2': '#dcc7c0', muted: '#cbb5ae', accent: '#f49c8b', 'accent-soft': '#ffd2c5', 'accent-ink': '#2a0c08', selection: '#653c35', grid: '#573a3740' } },
};
const ansi = {
  black: '#172538', red: '#ff8495', green: '#75d9a7', yellow: '#e8bb72', blue: '#8bb8ff', magenta: '#d5a4ff', cyan: '#58d6c2', white: '#d9e5f2',
  brightBlack: '#91a7ba', brightRed: '#ffb4bf', brightGreen: '#a8f0ca', brightYellow: '#f5d8a5', brightBlue: '#b7d5ff', brightMagenta: '#e8ccff', brightCyan: '#a2f3e5', brightWhite: '#ffffff',
} as const;
export interface Profile {
  readonly version: 1; readonly name: string; readonly foreground: string; readonly background: string; readonly accent: string;
  readonly fontFamily: string; readonly fontSize: number; readonly lineHeight: number; readonly letterSpacing: number;
  readonly cursorStyle: 'bar' | 'block' | 'underline'; readonly palette: typeof ansi;
}
const fallback = '"Cascadia Mono", "Cascadia Code", Consolas, monospace';
const profile = (name: string, foreground: string, background: string, accent: string): Profile => ({ version: 1, name, foreground, background, accent, fontFamily: fallback, fontSize: 13, lineHeight: 115, letterSpacing: 0, cursorStyle: 'bar', palette: ansi });
export const PROFILES: Readonly<Record<string, Profile>> = {
  neutral: profile('Neutral', '#d9e5f2', '#050912', '#f48fb8'),
  claude: profile('Claude', '#f3dfce', '#100d0b', '#f072a2'),
  codex: profile('Codex', '#d4e9ff', '#061018', '#8bb8ff'),
  opencode: profile('OpenCode', '#d1f4eb', '#071512', '#58d6c2'),
  pi: profile('Pi', '#e6dcff', '#100d1b', '#b7a4ff'),
  hermes: profile('Hermes', '#f2e6cc', '#110e09', '#e8bb72'),
};
const fonts = ['profile', 'Cascadia Mono', 'Cascadia Code', 'Consolas', 'monospace'] as const;
export interface PresentationSettings { readonly version: 1; readonly skin: string; readonly followSkin: boolean; readonly fontFamily: string; readonly fontSize: number; readonly lineHeight: number }
interface Snapshot { readonly revision: number; readonly settings: PresentationSettings }
export const DEFAULT_PRESENTATION: PresentationSettings = Object.freeze({ version: 1, skin: 'shinatama', followSkin: false, fontFamily: 'profile', fontSize: 13, lineHeight: 115 });
function parseSettings(value: unknown): PresentationSettings | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const data = value as Record<string, unknown>;
  if (Object.keys(data).sort().join(',') !== 'followSkin,fontFamily,fontSize,lineHeight,skin,version'
    || data.version !== 1 || typeof data.skin !== 'string' || !Object.hasOwn(SKINS, data.skin)
    || typeof data.followSkin !== 'boolean' || !fonts.includes(data.fontFamily as typeof fonts[number])
    || !Number.isInteger(data.fontSize) || Number(data.fontSize) < 8 || Number(data.fontSize) > 32
    || !Number.isInteger(data.lineHeight) || Number(data.lineHeight) < 100 || Number(data.lineHeight) > 180) return null;
  return Object.freeze({ version: 1, skin: data.skin, followSkin: data.followSkin, fontFamily: String(data.fontFamily), fontSize: Number(data.fontSize), lineHeight: Number(data.lineHeight) });
}
function snapshot(value: unknown): Snapshot | null {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return null;
  const data = value as Record<string, unknown>; const settings = parseSettings(data.settings);
  return settings && Object.keys(data).sort().join(',') === 'revision,settings' && Number.isSafeInteger(data.revision) && Number(data.revision) >= 0
    ? { revision: Number(data.revision), settings } : null;
}
export function resolveProfile(id: string | null | undefined): { id: string; profile: Profile; fallback: boolean } {
  const known = typeof id === 'string' && Object.hasOwn(PROFILES, id);
  return { id: known ? id : 'neutral', profile: known ? PROFILES[id] : PROFILES.neutral, fallback: !known && id !== null && id !== undefined };
}
// Native glass (Windows acrylic behind the terminal panes only): the terminal background keeps
// its colour and gives up part of its opacity. Everything else in the palette stays opaque.
let terminalGlass = false;
export function setTerminalGlass(enabled: boolean) { terminalGlass = enabled; }
const glassBackground = (color: string) => terminalGlass && /^#[0-9a-f]{6}$/i.test(color) ? `${color}94` : color;
export function optionsForProfile(id: string | null | undefined, settings: PresentationSettings): ITerminalOptions {
  const selected = resolveProfile(id).profile; const skin = SKINS[settings.skin]?.tokens ?? base;
  const palette: ITheme = settings.followSkin ? {
    ...ansi, black: skin.terminal, brightBlack: skin.muted, white: skin['text-2'], brightWhite: skin.text,
    red: skin.error, green: skin.live, yellow: skin.warning, blue: skin['line-strong'], magenta: skin.accent, cyan: skin.ice,
    foreground: skin.text, background: glassBackground(skin.terminal), cursor: skin.accent, selectionBackground: skin.selection,
  } : { ...selected.palette, foreground: selected.foreground, background: glassBackground(selected.background), cursor: selected.accent, selectionBackground: '#41567599' };
  // xterm maps default/ANSI colours. No PTY bytes or truecolour sequences are rewritten. With the
  // skin followed, Claude Code and Codex also start on their themes made of ANSI colours (the
  // engine's terminal_palette, set from these settings in presentation.rs).
  const fontFamily = settings.fontFamily === 'profile' ? selected.fontFamily : settings.fontFamily === 'monospace' ? 'monospace'
    : `"${settings.fontFamily}", ${fallback}`;
  return { theme: palette, fontFamily, fontSize: settings.fontSize, lineHeight: settings.lineHeight / 100, letterSpacing: selected.letterSpacing, cursorStyle: selected.cursorStyle, cursorBlink: false };
}
export function applyTerminalProfile(terminal: Terminal, id: string | null | undefined, settings: PresentationSettings) {
  const resolved = resolveProfile(id); const next = optionsForProfile(id, settings);
  const metricsChanged = terminal.options.fontFamily !== next.fontFamily || terminal.options.fontSize !== next.fontSize
    || terminal.options.lineHeight !== next.lineHeight || terminal.options.letterSpacing !== next.letterSpacing;
  if (JSON.stringify(terminal.options.theme) !== JSON.stringify(next.theme)) terminal.options.theme = next.theme;
  if (metricsChanged) { terminal.options.fontFamily = next.fontFamily; terminal.options.fontSize = next.fontSize; terminal.options.lineHeight = next.lineHeight; terminal.options.letterSpacing = next.letterSpacing; }
  terminal.options.cursorStyle = next.cursorStyle; terminal.options.cursorBlink = next.cursorBlink;
  return { id: resolved.id, metricsChanged, background: next.theme?.background ?? resolved.profile.background, fallback: resolved.fallback };
}
export function createPresentation(options: { desktop: boolean; notify(message: unknown, tone?: 'info' | 'error'): void; onChange(): void }) {
  let current = DEFAULT_PRESENTATION; let revision = 0; let loaded = !options.desktop; let blocked = false;
  let flight: Promise<void> | null = null; let pending: PresentationSettings | null = null; let timer = 0; let edits = 0;
  const warnedProfiles = new Set<string>();
  const toolbar = document.querySelector('.toolbar')!;
  const label = document.createElement('label'); label.className = 'presentation-skin'; label.textContent = 'Theme';
  const selector = document.createElement('select'); selector.setAttribute('aria-label', 'Interface theme');
  for (const [id, skin] of Object.entries(SKINS)) { const option = document.createElement('option'); option.value = id; option.textContent = skin.name; selector.append(option); }
  label.append(selector); toolbar.append(label);
  const button = document.createElement('button'); button.type = 'button'; button.textContent = 'Appearance'; toolbar.append(button);
  const dialog = document.createElement('dialog'); dialog.id = 'presentation-dialog'; dialog.setAttribute('aria-labelledby', 'presentation-title');
  dialog.innerHTML = '<form><div class="dialog-heading"><h1 id="presentation-title">Appearance</h1><button type="button" data-close aria-label="Close appearance">×</button></div><label for="presentation-font">Terminal font</label><select id="presentation-font"></select><label for="presentation-size">Font size</label><input id="presentation-size" type="number" min="8" max="32" step="1"><label for="presentation-height">Line height (%)</label><input id="presentation-height" type="number" min="100" max="180" step="1"><label class="presentation-follow"><input type="checkbox" id="presentation-follow"> Follow interface theme in terminals</label><p class="hint">Follow theme changes default and ANSI colours; Claude Code and Codex started afterwards use their ANSI-colour themes. Application truecolour output stays untouched.</p><p class="hint">Uses local fonts with a system monospace fallback. No fonts or artwork are downloaded.</p><p id="presentation-status" role="status"></p><div class="dialog-actions"><button type="button" data-reload>Reload saved preferences</button><button type="submit" class="primary">Apply</button><button type="button" data-close>Close</button></div></form>';
  document.querySelector('main')!.append(dialog);
  const font = dialog.querySelector<HTMLSelectElement>('#presentation-font')!;
  for (const name of fonts) { const option = document.createElement('option'); option.value = name; option.textContent = name === 'profile' ? 'Profile default' : name; font.append(option); }
  const size = dialog.querySelector<HTMLInputElement>('#presentation-size')!; const height = dialog.querySelector<HTMLInputElement>('#presentation-height')!;
  const follow = dialog.querySelector<HTMLInputElement>('#presentation-follow')!; const status = dialog.querySelector<HTMLElement>('#presentation-status')!;
  const equal = (a: PresentationSettings, b: PresentationSettings) => JSON.stringify(a) === JSON.stringify(b);
  function render() { selector.value = current.skin; font.value = current.fontFamily; size.value = String(current.fontSize); height.value = String(current.lineHeight); follow.checked = current.followSkin; }
  function apply() { for (const [name, value] of Object.entries(SKINS[current.skin].tokens)) document.documentElement.style.setProperty(`--${name}`, value); render(); }
  function reportFailure() { blocked = true; status.textContent = 'Changes remain visible in this window. Saving is unavailable; reload saved preferences before trying again.'; options.notify(status.textContent); }
  function schedule() { clearTimeout(timer); if (!blocked && loaded && pending) timer = window.setTimeout(() => { void save(); }, 0); }
  async function save() {
    if (!options.desktop || !loaded || blocked || flight || !pending) return;
    const desired = pending; pending = null;
    const task = (async () => {
      try {
        const received = snapshot(await invoke('presentation_save', { request: { expectedRevision: revision, settings: desired } }));
        if (!received || received.revision !== revision + 1 || !equal(received.settings, desired)) throw new Error('Invalid preference confirmation');
        revision = received.revision; status.textContent = 'Preferences saved.';
      } catch { reportFailure(); }
    })();
    flight = task; await task; if (flight === task) flight = null; schedule();
  }
  async function initialize() {
    if (flight) return flight;
    if (!options.desktop) { apply(); status.textContent = 'Browser preview: preferences are not saved.'; return; }
    const at = edits;
    const task = (async () => {
      try {
        const received = snapshot(await invoke('presentation_load')); if (!received) throw new Error('Invalid saved presentation');
        revision = received.revision; loaded = true; blocked = false;
        if (at === edits) { current = received.settings; pending = null; } else pending = current;
        apply(); options.onChange(); status.textContent = 'Preferences loaded.';
      } catch { loaded = false; reportFailure(); }
    })();
    flight = task; await task; if (flight === task) flight = null; schedule();
  }
  async function flush(timeoutMs = 2000): Promise<boolean> {
    clearTimeout(timer); const deadline = performance.now() + Math.max(1, Math.min(timeoutMs, 2000));
    for (let attempt = 0; attempt < 4; attempt++) {
      if (!flight && pending) void save();
      if (!flight) return !pending && !blocked;
      const remaining = deadline - performance.now(); if (remaining <= 0) return false;
      let waitTimer = 0;
      const completed = await Promise.race([flight.then(() => true), new Promise<boolean>(resolve => { waitTimer = window.setTimeout(() => resolve(false), remaining); })]);
      clearTimeout(waitTimer); if (!completed) return false;
    }
    return !flight && !pending && !blocked;
  }
  function edit(settings: PresentationSettings) { current = Object.freeze({ ...settings }); edits++; pending = current; apply(); options.onChange(); schedule(); }
  selector.onchange = () => { if (Object.hasOwn(SKINS, selector.value)) edit({ ...current, skin: selector.value }); };
  button.onclick = () => { if (document.querySelector('dialog[open][aria-modal="true"]')) return; render(); openModal(dialog); };
  dialog.querySelectorAll<HTMLButtonElement>('[data-close]').forEach(close => { close.onclick = () => dialog.close(); });
  dialog.querySelector<HTMLButtonElement>('[data-reload]')!.onclick = () => { if (!flight) { edits++; void initialize(); } };
  dialog.querySelector('form')!.onsubmit = event => {
    event.preventDefault(); if (!dialog.querySelector('form')!.reportValidity()) return;
    const settings = parseSettings({ ...current, fontFamily: font.value, fontSize: Number(size.value), lineHeight: Number(height.value), followSkin: follow.checked });
    if (settings) { edit(settings); status.textContent = options.desktop ? blocked ? 'Applied locally. Reload saved preferences to restore saving.' : 'Applied; saving…' : 'Applied in browser preview.'; }
  };
  window.addEventListener('pagehide', () => { clearTimeout(timer); pending = null; }, { once: true });
  apply();
  return { initialize, flush, profiles: PROFILES, settings: () => current,
    optionsForProfile: (id: string | null | undefined) => optionsForProfile(id, current),
    applyProfile: (terminal: Terminal, id: string | null | undefined) => {
      const result = applyTerminalProfile(terminal, id, current);
      if (result.fallback) {
        status.textContent = 'That terminal profile is unavailable. The built-in Neutral profile is used without restarting the terminal.';
        if (warnedProfiles.size < 16 && !warnedProfiles.has(String(id))) { warnedProfiles.add(String(id)); options.notify(status.textContent, 'info'); }
      }
      return result;
    },
  };
}

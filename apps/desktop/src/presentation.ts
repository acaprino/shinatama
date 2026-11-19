import { invoke } from '@tauri-apps/api/core';
import type { ITheme, ITerminalOptions, Terminal } from '@xterm/xterm';
import { confirmsField } from './desktop-shell-model.ts';

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
/** The look of the window: Smooth (the default) or ONI MODE (the look Shinatama was born with). The choice is an attribute on
 * the root (`data-look`): smooth.css is scoped to it, the other stylesheets are what is left without it. Kept in the
 * webview's own storage like the zoom. Smooth has one palette, below; the skins above are ONI MODE's themes. */
export type Look = 'smooth' | 'oni';
export const LOOK_KEY = 'shinatama.look';
/** The look a stored value stands for: ONI MODE only when it was chosen, anything else (nothing stored, a stale value) is Smooth. */
export const storedLook = (raw: string | null): Look => (raw === 'oni' ? 'oni' : 'smooth');
/** Smooth: a slate dark look (a ground, and a sheet one step lighter), her pink hair the one accent. */
export const SMOOTH: Tokens = { ...base, stage: '#12161e', chrome: '#12161e', 'chrome-2': '#161b25', surface: '#1a202c', 'surface-2': '#222a38', terminal: '#0e1118', line: '#2a3240', 'line-strong': '#6b7488', ice: '#c9d4e6', text: '#e8ecf3', 'text-2': '#c0c8d6', muted: '#9aa5b8', selection: '#4a2c40', scrim: '#0a0d12cc', shadow: '#00000066', grid: '#2a324040' };
/** Smooth's theme: the one of Windows, or light or dark whatever Windows says. ONI MODE has none: it is always dark. */
export type Theme = 'system' | 'light' | 'dark';
export const THEME_KEY = 'shinatama.theme';
/** The theme a stored value stands for: anything that is not light or dark follows Windows. */
export const storedTheme = (raw: string | null): Theme => (raw === 'light' || raw === 'dark' ? raw : 'system');
/** Smooth in the light: a grey ground and white sheets. The pink is hers, unchanged, for fills and marks; as text
 * and as the focus ring it is ACCENT_TEXT_LIGHT (the --accent-text the stylesheets read), deep enough to read on white. */
export const ACCENT_TEXT_LIGHT = '#b0286a';
export const SMOOTH_LIGHT: Tokens = { ...base, stage: '#f6f7f9', chrome: '#f6f7f9', 'chrome-2': '#f0f2f5', surface: '#ffffff', 'surface-2': '#eceef2', terminal: '#ffffff', line: '#e3e6ea', 'line-strong': '#7b8494', ice: '#2b4a80', text: '#111827', 'text-2': '#3d4656', muted: '#566070', live: '#0f7566', warning: '#8a5a00', error: '#b3261e', danger: '#b3261e', 'danger-line': '#8f1d17', selection: '#f9d7e6', scrim: '#0f183055', shadow: '#15274a26', grid: '#e3e6ea80' };
const tokensFor = (look: Look, skin: string, light = false): Tokens => look === 'smooth' ? light ? SMOOTH_LIGHT : SMOOTH : SKINS[skin]?.tokens ?? base;
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
export function optionsForProfile(id: string | null | undefined, settings: PresentationSettings, look: Look = 'oni', light = false): ITerminalOptions {
  const selected = resolveProfile(id).profile; const skin = tokensFor(look, settings.skin, light);
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
export function applyTerminalProfile(terminal: Terminal, id: string | null | undefined, settings: PresentationSettings, look: Look = 'oni', light = false) {
  const resolved = resolveProfile(id); const next = optionsForProfile(id, settings, look, light);
  const metricsChanged = terminal.options.fontFamily !== next.fontFamily || terminal.options.fontSize !== next.fontSize
    || terminal.options.lineHeight !== next.lineHeight || terminal.options.letterSpacing !== next.letterSpacing;
  if (JSON.stringify(terminal.options.theme) !== JSON.stringify(next.theme)) terminal.options.theme = next.theme;
  if (metricsChanged) { terminal.options.fontFamily = next.fontFamily; terminal.options.fontSize = next.fontSize; terminal.options.lineHeight = next.lineHeight; terminal.options.letterSpacing = next.letterSpacing; }
  terminal.options.cursorStyle = next.cursorStyle; terminal.options.cursorBlink = next.cursorBlink;
  return { id: resolved.id, metricsChanged, background: next.theme?.background ?? resolved.profile.background, fallback: resolved.fallback };
}
/** The Appearance controls. They live in the Appearance section of Settings (settings-panel.ts);
 * every change applies at once and is saved through presentation_save. */
export function createPresentation(options: { desktop: boolean; notify(message: unknown, tone?: 'info' | 'error'): void; onChange(): void }) {
  let current = DEFAULT_PRESENTATION; let revision = 0; let loaded = !options.desktop; let blocked = false;
  let flight: Promise<void> | null = null; let pending: PresentationSettings | null = null; let timer = 0; let edits = 0;
  const warnedProfiles = new Set<string>();
  const theme = document.createElement('div'); theme.className = 'presentation-theme';
  theme.innerHTML = '<button id="presentation-look" class="look-switch" type="button" role="switch" aria-checked="false">ONI mode</button><p class="hint">ONI mode is the look Shinatama was born with. Off, the window wears the Smooth look.</p><label for="presentation-theme">Smooth theme</label><select id="presentation-theme"><option value="system">Windows</option><option value="light">Light</option><option value="dark">Dark</option></select><label for="presentation-skin">ONI mode theme</label><select id="presentation-skin"></select>';
  let look: Look = (() => { try { return storedLook(localStorage.getItem(LOOK_KEY)); } catch { return storedLook(null); } })();
  const lookSwitch = theme.querySelector<HTMLButtonElement>('#presentation-look')!;
  let smoothTheme: Theme = (() => { try { return storedTheme(localStorage.getItem(THEME_KEY)); } catch { return storedTheme(null); } })();
  const themeSelector = theme.querySelector<HTMLSelectElement>('#presentation-theme')!;
  const systemLight = window.matchMedia('(prefers-color-scheme: light)');
  const light = () => look === 'smooth' && (smoothTheme === 'light' || (smoothTheme === 'system' && systemLight.matches));
  const selector = theme.querySelector<HTMLSelectElement>('#presentation-skin')!;
  for (const [id, skin] of Object.entries(SKINS)) { const option = document.createElement('option'); option.value = id; option.textContent = skin.name; selector.append(option); }
  const form = document.createElement('form'); form.className = 'presentation-terminal';
  form.innerHTML = '<label for="presentation-font">Terminal font</label><select id="presentation-font"></select><label for="presentation-size">Font size</label><input id="presentation-size" type="number" min="8" max="32" step="1" required><label for="presentation-height">Line height (%)</label><input id="presentation-height" type="number" min="100" max="180" step="1" required><label class="presentation-follow"><input type="checkbox" id="presentation-follow"> Follow interface theme in terminals</label><p class="hint">Follow theme changes default and ANSI colours; Claude Code and Codex started afterwards use their ANSI-colour themes. Application truecolour output stays untouched.</p><p class="hint">Uses local fonts with a system monospace fallback. No fonts or artwork are downloaded.</p><p id="presentation-status" role="status"></p>';
  const font = form.querySelector<HTMLSelectElement>('#presentation-font')!;
  for (const name of fonts) { const option = document.createElement('option'); option.value = name; option.textContent = name === 'profile' ? 'Profile default' : name; font.append(option); }
  const size = form.querySelector<HTMLInputElement>('#presentation-size')!; const height = form.querySelector<HTMLInputElement>('#presentation-height')!;
  const follow = form.querySelector<HTMLInputElement>('#presentation-follow')!; const status = form.querySelector<HTMLElement>('#presentation-status')!;
  const equal = (a: PresentationSettings, b: PresentationSettings) => JSON.stringify(a) === JSON.stringify(b);
  function render() { lookSwitch.setAttribute('aria-checked', String(look === 'oni')); selector.disabled = look !== 'oni'; themeSelector.disabled = look === 'oni'; themeSelector.value = smoothTheme; selector.value = current.skin; font.value = current.fontFamily; size.value = String(current.fontSize); height.value = String(current.lineHeight); follow.checked = current.followSkin; }
  function apply() { document.documentElement.dataset.look = look; document.documentElement.style.colorScheme = light() ? 'light' : 'dark'; if (light()) document.documentElement.style.setProperty('--accent-text', ACCENT_TEXT_LIGHT); else document.documentElement.style.removeProperty('--accent-text'); for (const [name, value] of Object.entries(tokensFor(look, current.skin, light()))) document.documentElement.style.setProperty(`--${name}`, value); render(); }
  function reportFailure() { blocked = true; status.textContent = 'Changes remain visible in this window. Saving is unavailable; press Reload in Settings before trying again.'; options.notify(status.textContent); }
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
  // A terminal field applies when its value is committed (a change, or Enter in a number), as the theme always has.
  function applyFields() {
    if (!form.reportValidity()) return;
    const settings = parseSettings({ ...current, fontFamily: font.value, fontSize: Number(size.value), lineHeight: Number(height.value), followSkin: follow.checked });
    if (!settings || equal(settings, current)) return;
    edit(settings); status.textContent = options.desktop ? blocked ? 'Applied locally. Press Reload in Settings to restore saving.' : 'Applied; saving…' : 'Applied in browser preview.';
  }
  lookSwitch.onclick = () => {
    look = look === 'oni' ? 'smooth' : 'oni';
    try { localStorage.setItem(LOOK_KEY, look); } catch { /* no storage: the look lasts until the window closes */ }
    apply(); options.onChange();
  };
  themeSelector.onchange = () => {
    smoothTheme = storedTheme(themeSelector.value);
    try { localStorage.setItem(THEME_KEY, smoothTheme); } catch { /* no storage: the theme lasts until the window closes */ }
    apply(); options.onChange();
  };
  systemLight.addEventListener('change', () => { apply(); options.onChange(); });
  selector.onchange = () => { if (Object.hasOwn(SKINS, selector.value)) edit({ ...current, skin: selector.value }); };
  for (const field of [font, size, height, follow]) field.addEventListener('change', applyFields);
  for (const field of [size, height]) field.addEventListener('keydown', event => { if (confirmsField(event)) { event.preventDefault(); applyFields(); } });
  form.onsubmit = event => { event.preventDefault(); applyFields(); };
  window.addEventListener('pagehide', () => { clearTimeout(timer); pending = null; }, { once: true });
  apply();
  return { initialize, flush, profiles: PROFILES, settings: () => current,
    /** The interface control (the theme) and the terminal controls, for the Appearance section. */
    theme, terminal: form,
    /** Shows the applied values again, dropping a half-typed number. */
    reset: render,
    /** Reads the saved preferences again; after a failed save this is what restores saving. */
    reload() { if (!flight) { edits++; void initialize(); } },
    optionsForProfile: (id: string | null | undefined) => optionsForProfile(id, current, look, light()),
    applyProfile: (terminal: Terminal, id: string | null | undefined) => {
      const result = applyTerminalProfile(terminal, id, current, look, light());
      if (result.fallback) {
        status.textContent = 'That terminal profile is unavailable. The built-in Neutral profile is used without restarting the terminal.';
        if (warnedProfiles.size < 16 && !warnedProfiles.has(String(id))) { warnedProfiles.add(String(id)); options.notify(status.textContent, 'info'); }
      }
      return result;
    },
  };
}

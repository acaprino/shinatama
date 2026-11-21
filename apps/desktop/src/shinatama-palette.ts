// The five skins of ONI MODE: overrides of Imago's tokens, Shinatama's own. They lived in @acaprino/imago/shell until
// v0.2.0, which gave them back (its using-imago.md, section 11, Look and theme); taken unchanged from Imago's tag v0.1.4.
const base = {
  stage: '#04070f', chrome: '#070c18', 'chrome-2': '#0a1120', surface: '#0e1729', 'surface-2': '#13203a', terminal: '#050912',
  line: '#1c3563', 'line-strong': '#4a78e6', ice: '#a9c8ff', text: '#e6f0ff', 'text-2': '#b4c8e8', muted: '#8fa6c9',
  accent: '#f48fb8', 'accent-soft': '#ffb3d0', 'accent-ink': '#1c0811', live: '#6fe3d2', warning: '#e8bb72', error: '#ff8495',
  danger: '#a0294e', 'danger-line': '#e35f87', 'on-danger': '#ffffff', selection: '#2a4a8f', scrim: '#02050bcc', shadow: '#000000aa', grid: '#1c356340',
  'glass-tint': '#04070f',
} as const;
export type ShinatamaPaletteTokens = { readonly [K in keyof typeof base]: string };
export const SHINATAMA_PALETTES: Readonly<Record<string, { readonly name: string; readonly tokens: ShinatamaPaletteTokens }>> = {
  shinatama: { name: 'Shinatama', tokens: base },
  neutral: { name: 'Neutral dark', tokens: { ...base, stage: '#080b10', chrome: '#0d1219', 'chrome-2': '#121a24', surface: '#17212e', 'surface-2': '#1d2a3a', terminal: '#090e15', line: '#334256', 'line-strong': '#899bb6', ice: '#d5e1f4', accent: '#b8c9e3', 'accent-soft': '#dbe7fa', 'accent-ink': '#0a1220', grid: '#33425640' } },
  section9: { name: 'Section 9', tokens: { ...base, stage: '#030b0c', chrome: '#061214', 'chrome-2': '#091b1d', surface: '#0c2225', 'surface-2': '#103035', terminal: '#041012', line: '#24545a', 'line-strong': '#62aeb6', ice: '#a8edf2', text: '#e4f8f6', 'text-2': '#bededb', muted: '#a4c9c6', accent: '#78e2dc', 'accent-soft': '#b4fff8', 'accent-ink': '#032021', selection: '#235e62', grid: '#24545a40' } },
  tctf: { name: 'TCTF', tokens: { ...base, stage: '#090b0e', chrome: '#10151b', 'chrome-2': '#171e26', surface: '#1b2631', 'surface-2': '#24313e', terminal: '#0a1016', line: '#455567', 'line-strong': '#b59b78', ice: '#f6d9b3', text: '#f5ece0', 'text-2': '#d8cab8', muted: '#c7baa7', accent: '#ffc072', 'accent-soft': '#ffdfad', 'accent-ink': '#241405', selection: '#635035', grid: '#45556740' } },
  sumi: { name: 'Sumi', tokens: { ...base, stage: '#080707', chrome: '#100c0c', 'chrome-2': '#171111', surface: '#211919', 'surface-2': '#2b2020', terminal: '#0a0808', line: '#573a37', 'line-strong': '#c88b81', ice: '#f3d5cd', text: '#f7e9e3', 'text-2': '#dcc7c0', muted: '#cbb5ae', accent: '#f49c8b', 'accent-soft': '#ffd2c5', 'accent-ink': '#2a0c08', selection: '#653c35', grid: '#573a3740' } },
};

/** Semantic colours for the named ONI palettes. Translucency applies only to glass surfaces. */
function roles(p: ShinatamaPaletteTokens, glass: boolean) {
  return {
    ground: p.stage + (glass ? 'db' : ''), sheet: p.surface + (glass ? 'cc' : ''), 'sheet-solid': p.surface,
    float: p['surface-2'] + (glass ? 'f7' : ''), field: p.chrome + (glass ? 'e6' : ''), button: p['surface-2'],
    line: p.line, 'line-soft': p.line + '55', 'line-strong': p['line-strong'], edge: p.line + 'aa',
    text: p.text, 'text-2': p['text-2'], 'text-hi': p.text, muted: p.muted,
    hover: p.selection + '44', press: p.selection + '99', selected: p.selection,
    'chip-text': p['text-2'], skeleton: p['surface-2'], terminal: p.terminal + (glass ? 'b3' : ''),
    scrim: p.scrim, shadow: p.shadow, ok: p.live, warn: p.warning, err: p.error, info: p.ice,
    danger: p.danger, 'danger-on': p['on-danger'], accent: p.accent, 'accent-hi': p['accent-soft'],
    'accent-text': p.accent, 'accent-on': p['accent-ink'],
  };
}

type PreviousRole = { property: string; value: string; priority: string };
const previous = new WeakMap<HTMLElement, PreviousRole[]>();

/** Paint one root, restoring its previous cascade when it leaves the customised look. */
export function applyShinatamaPalette(root: HTMLElement, skin: string | null, glass: boolean): void {
  if (skin === null) {
    for (const { property, value, priority } of previous.get(root) ?? []) {
      if (value) root.style.setProperty(property, value, priority);
      else root.style.removeProperty(property);
    }
    previous.delete(root);
    return;
  }
  const palette = Object.hasOwn(SHINATAMA_PALETTES, skin) ? SHINATAMA_PALETTES[skin].tokens : base;
  const entries = Object.entries(roles(palette, glass));
  if (!previous.has(root)) previous.set(root, entries.map(([role]) => {
    const property = '--im-' + role;
    return { property, value: root.style.getPropertyValue(property), priority: root.style.getPropertyPriority(property) };
  }));
  for (const [role, value] of entries) root.style.setProperty('--im-' + role, value);
}

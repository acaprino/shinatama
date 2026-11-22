/** The source view of the Files panel: CodeMirror 6 with syntax highlighting in the skin's colors.
 * Loaded on first use (files-panel.ts imports it dynamically), each language on first need, so a
 * session that never opens a file pays nothing. CodeMirror draws only the visible lines, which
 * keeps a 4 MiB file as cheap as a short one. Each open file keeps its own state (undo history,
 * selection, scroll) until the panel forgets it. */
import { Compartment, EditorState, type Extension } from '@codemirror/state';
import { EditorView, drawSelection, highlightActiveLine, highlightActiveLineGutter, highlightSpecialChars, keymap, lineNumbers } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands';
import { HighlightStyle, StreamLanguage, bracketMatching, foldGutter, foldKeymap, indentOnInput, syntaxHighlighting, type StreamParser } from '@codemirror/language';
import { highlightSelectionMatches, search, searchKeymap } from '@codemirror/search';
import { tags as t } from '@lezer/highlight';

const legacy = (load: () => Promise<StreamParser<unknown>>) => async () => StreamLanguage.define(await load());
/** File name patterns and the language each loads. The first match wins; anything else is plain text. */
const LANGUAGES: [RegExp, string, (name: string) => Promise<Extension>][] = [
  [/\.(ts|mts|cts|tsx)$/i, 'TypeScript', async name => (await import('@codemirror/lang-javascript')).javascript({ typescript: true, jsx: /x$/i.test(name) })],
  [/\.(js|mjs|cjs|jsx)$/i, 'JavaScript', async name => (await import('@codemirror/lang-javascript')).javascript({ jsx: /x$/i.test(name) })],
  [/\.(json|jsonc|json5)$|^\.(eslintrc|prettierrc|babelrc)$/i, 'JSON', async () => (await import('@codemirror/lang-json')).json()],
  [/\.rs$/i, 'Rust', async () => (await import('@codemirror/lang-rust')).rust()],
  [/\.(py|pyw|pyi)$/i, 'Python', async () => (await import('@codemirror/lang-python')).python()],
  [/\.(css|scss|less)$/i, 'CSS', async () => (await import('@codemirror/lang-css')).css()],
  [/\.(html?|vue|svelte|astro)$/i, 'HTML', async () => (await import('@codemirror/lang-html')).html()],
  [/\.(md|markdown|mdx)$/i, 'Markdown', async () => (await import('@codemirror/lang-markdown')).markdown()],
  [/\.(ya?ml)$/i, 'YAML', async () => (await import('@codemirror/lang-yaml')).yaml()],
  [/\.(xml|svg|xaml|csproj|props|targets|resx|plist)$/i, 'XML', async () => (await import('@codemirror/lang-xml')).xml()],
  [/\.sql$/i, 'SQL', async () => (await import('@codemirror/lang-sql')).sql()],
  [/\.toml$|^Cargo\.lock$/i, 'TOML', legacy(async () => (await import('@codemirror/legacy-modes/mode/toml')).toml)],
  [/\.(ps1|psm1|psd1)$/i, 'PowerShell', legacy(async () => (await import('@codemirror/legacy-modes/mode/powershell')).powerShell)],
  [/\.(sh|bash|zsh)$|^\.(bashrc|zshrc|profile)$/i, 'Shell', legacy(async () => (await import('@codemirror/legacy-modes/mode/shell')).shell)],
  [/\.(c|h)$/i, 'C', legacy(async () => (await import('@codemirror/legacy-modes/mode/clike')).c)],
  [/\.(cc|cpp|cxx|hh|hpp|hxx)$/i, 'C++', legacy(async () => (await import('@codemirror/legacy-modes/mode/clike')).cpp)],
  [/\.cs$/i, 'C#', legacy(async () => (await import('@codemirror/legacy-modes/mode/clike')).csharp)],
  [/\.java$/i, 'Java', legacy(async () => (await import('@codemirror/legacy-modes/mode/clike')).java)],
  [/\.(kt|kts)$/i, 'Kotlin', legacy(async () => (await import('@codemirror/legacy-modes/mode/clike')).kotlin)],
  [/\.go$/i, 'Go', legacy(async () => (await import('@codemirror/legacy-modes/mode/go')).go)],
  [/\.lua$/i, 'Lua', legacy(async () => (await import('@codemirror/legacy-modes/mode/lua')).lua)],
  [/\.rb$/i, 'Ruby', legacy(async () => (await import('@codemirror/legacy-modes/mode/ruby')).ruby)],
  [/(^|[\\/])Dockerfile$|\.dockerfile$/i, 'Dockerfile', legacy(async () => (await import('@codemirror/legacy-modes/mode/dockerfile')).dockerFile)],
  [/\.(diff|patch)$/i, 'Diff', legacy(async () => (await import('@codemirror/legacy-modes/mode/diff')).diff)],
  [/\.(ini|cfg|properties|editorconfig|gitattributes|gitignore|env)$|^\.(gitignore|gitattributes|editorconfig|env)$/i, 'Properties', legacy(async () => (await import('@codemirror/legacy-modes/mode/properties')).properties)],
];
const entryFor = (path: string) => { const name = path.split(/[\\/]/).pop() ?? path; return LANGUAGES.find(([pattern]) => pattern.test(name)); };
/** The language a file name asks for, as a label for the status line. */
export function languageOf(path: string): string { return entryFor(path)?.[1] ?? 'Plain text'; }

/** Colors come from the skin's tokens, so a skin change recolors the code with everything else. */
const highlight = HighlightStyle.define([
  { tag: [t.keyword, t.controlKeyword, t.operatorKeyword, t.modifier, t.definitionKeyword, t.moduleKeyword], color: 'var(--im-info)' },
  { tag: [t.string, t.special(t.string), t.regexp, t.character], color: 'var(--im-ok)' },
  { tag: [t.number, t.bool, t.null, t.atom, t.unit], color: 'var(--im-warn)' },
  { tag: [t.comment, t.lineComment, t.blockComment, t.docComment], color: 'var(--im-muted)', fontStyle: 'italic' },
  { tag: [t.function(t.variableName), t.function(t.propertyName), t.definition(t.function(t.variableName))], color: 'var(--im-accent-text)' },
  { tag: [t.typeName, t.className, t.namespace, t.definition(t.typeName)], color: 'color-mix(in srgb, var(--im-info) 55%, var(--im-ok))' },
  { tag: [t.propertyName, t.attributeName], color: 'var(--im-text-2)' },
  { tag: [t.tagName, t.angleBracket], color: 'var(--im-accent-text)' },
  { tag: [t.macroName, t.annotation, t.meta, t.processingInstruction], color: 'var(--im-warn)' },
  { tag: [t.punctuation, t.separator, t.bracket, t.operator], color: 'var(--im-muted)' },
  { tag: [t.heading, t.strong], color: 'var(--im-text)', fontWeight: '700' },
  { tag: t.emphasis, fontStyle: 'italic' },
  { tag: [t.link, t.url], color: 'var(--im-info)', textDecoration: 'underline' },
  { tag: [t.inserted], color: 'var(--im-ok)' },
  { tag: [t.deleted], color: 'var(--im-err)' },
  { tag: t.invalid, color: 'var(--im-err)' },
]);
const theme = EditorView.theme({
  '&': { height: '100%', color: 'var(--im-text)', backgroundColor: 'transparent', fontSize: '13px' },
  '&.cm-focused': { outline: 'none' },
  '.cm-scroller': { fontFamily: 'var(--im-font-code)', lineHeight: '1.6', scrollbarWidth: 'thin', scrollbarColor: 'var(--im-line-strong) transparent' },
  '.cm-content': { caretColor: 'var(--im-accent)', padding: '8px 0' },
  '.cm-line': { padding: '0 14px 0 6px' },
  '.cm-gutters': { backgroundColor: 'transparent', color: 'var(--im-muted)', border: 'none', borderRight: '1px solid color-mix(in srgb, var(--im-line) 70%, transparent)' },
  '.cm-lineNumbers .cm-gutterElement': { padding: '0 10px 0 14px', minWidth: '4ch', fontVariantNumeric: 'tabular-nums' },
  '.cm-activeLine': { backgroundColor: 'color-mix(in srgb, var(--im-line-strong) 9%, transparent)' },
  '.cm-activeLineGutter': { backgroundColor: 'transparent', color: 'var(--im-text-2)' },
  '.cm-cursor, .cm-dropCursor': { borderLeftColor: 'var(--im-accent)' },
  '&.cm-focused > .cm-scroller > .cm-selectionLayer .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection': { backgroundColor: 'var(--im-selected)' },
  '.cm-matchingBracket, &.cm-focused .cm-matchingBracket': { backgroundColor: 'transparent', outline: '1px solid var(--im-line-strong)' },
  '.cm-selectionMatch': { backgroundColor: 'color-mix(in srgb, var(--im-info) 14%, transparent)' },
  '.cm-searchMatch': { backgroundColor: 'color-mix(in srgb, var(--im-warn) 22%, transparent)', outline: '1px solid color-mix(in srgb, var(--im-warn) 60%, transparent)' },
  '.cm-searchMatch.cm-searchMatch-selected': { backgroundColor: 'color-mix(in srgb, var(--im-accent) 30%, transparent)' },
  '.cm-foldPlaceholder': { backgroundColor: 'transparent', border: '1px solid var(--im-line-strong)', color: 'var(--im-text-2)', padding: '0 4px', borderRadius: '0' },
  '.cm-foldGutter .cm-gutterElement': { padding: '0 4px', color: 'var(--im-muted)' },
  '.cm-panels': { backgroundColor: 'var(--im-sheet-solid)', color: 'var(--im-text)' },
  '.cm-panels.cm-panels-top': { borderBottom: '1px solid var(--im-line)' },
  '.cm-panels.cm-panels-bottom': { borderTop: '1px solid var(--im-line)' },
}, { dark: true });

export type CodeView = {
  /** Shows a file; an editable one is written back through `changed`. */
  open(path: string, text: string, editable: boolean): Promise<void>;
  /** The text a visual edit or another view produced, replacing the shown file's text. */
  replace(path: string, text: string): void;
  forget(path: string): void; clear(): void; focus(): void; destroy(): void;
};

export function createCodeView(host: HTMLElement, changed: (path: string, text: string) => void): CodeView {
  const language = new Compartment(); const access = new Compartment();
  const states = new Map<string, EditorState>(); let shown: string | null = null; let opening = 0;
  const base: Extension[] = [
    lineNumbers(), foldGutter({ markerDOM: open => { const mark = document.createElement('span'); mark.className = `cm-fold-mark${open ? ' open' : ''}`; return mark; } }),
    highlightSpecialChars(), history(), drawSelection(), indentOnInput(), bracketMatching(),
    highlightActiveLine(), highlightActiveLineGutter(), highlightSelectionMatches(), search({ top: true }),
    syntaxHighlighting(highlight), theme, EditorState.tabSize.of(4),
    keymap.of([...defaultKeymap, ...historyKeymap, ...searchKeymap, ...foldKeymap, indentWithTab]),
    // Keep drafts current for an immediate save, file switch or discard decision.
    EditorView.updateListener.of(update => {
      if (update.docChanged && shown) changed(shown, update.state.doc.toString());
    }),
  ];
  const view = new EditorView({ parent: host, state: EditorState.create({ doc: '', extensions: [base, language.of([]), access.of(EditorState.readOnly.of(true))] }) });
  const keep = () => { if (shown) states.set(shown, view.state); };
  return {
    async open(path, text, editable) {
      const ticket = ++opening; keep();
      const kept = states.get(path);
      const entry = entryFor(path);
      const grammar = entry ? await entry[2](path.split(/[\\/]/).pop() ?? path).catch(() => []) : [];
      // Prose wraps at the pane's width; code keeps its lines and scrolls.
      const support = !entry || entry[1] === 'Markdown' ? [grammar, EditorView.lineWrapping] : grammar;
      if (ticket !== opening) return;
      const fresh = !kept || kept.doc.toString() !== text;
      const state = fresh
        ? EditorState.create({ doc: text, extensions: [base, language.of(support), access.of([EditorState.readOnly.of(!editable), EditorView.editable.of(true)])] })
        : kept;
      shown = path; view.setState(state);
      if (!fresh) view.dispatch({ effects: [language.reconfigure(support), access.reconfigure([EditorState.readOnly.of(!editable), EditorView.editable.of(true)])] });
    },
    replace(path, text) {
      if (shown !== path || view.state.doc.toString() === text) return;
      view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } });
    },
    forget(path) { states.delete(path); },
    clear() { ++opening; states.clear(); shown = null; view.setState(EditorState.create({ doc: '', extensions: [base, language.of([]), access.of(EditorState.readOnly.of(true))] })); },
    focus() { view.focus(); },
    destroy() { ++opening; states.clear(); view.destroy(); },
  };
}

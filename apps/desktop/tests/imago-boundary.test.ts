import assert from 'node:assert/strict';
import { readFileSync, existsSync, readdirSync } from 'node:fs';
import { dirname, extname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import test from 'node:test';
import { parseSync } from 'vite';
import postcss from 'postcss';
import { htmlLanguage } from '@codemirror/lang-html';

const desktop = fileURLToPath(new URL('../', import.meta.url));
const publicImports = new Set(['ui', 'shell', 'tauri', 'graph', 'graph-layout', 'tokens', 'react', 'css/imago.css']);

// Native engine mount points, identified by source path, owning binding and element type.
const engineNodes = new Set([
  'src/browser-tab.ts:canvas:canvas', 'src/code-view.ts:mark:span',
  'src/files-panel.ts:img:img', 'src/terminal-history-view.ts:screen:div',
]);
const constructors = new Set(['createElement', 'createElementNS', 'createContextualFragment', 'insertAdjacentHTML',
  'setHTML', 'setHTMLUnsafe', 'parseFromString', 'insertRule', 'replaceSync']);
const contentProperties = new Set(['innerHTML', 'outerHTML', 'srcdoc', 'cssText', 'adoptedStyleSheets']);
const tag = /<\s*(?:\/?(?:button|input|dialog|select|option|label|form|textarea|style|div|span|p|nav|section|article|main|aside|header|footer|svg|path|table|tr|li|ul|a|img|h[1-6]))(?:\s|\/?>)/i;

type Node = { type: string; start: number; [key: string]: any };
function walk(node: Node, visit: (node: Node, parents: Node[]) => void, parents: Node[] = []) {
  visit(node, parents);
  parents.push(node);
  for (const value of Object.values(node)) {
    const children = Array.isArray(value) ? value : [value];
    for (const child of children) if (child && typeof child.type === 'string') walk(child, visit, parents);
  }
  parents.pop();
}

function staticText(node: Node | undefined): string | undefined {
  if (node?.type === 'Literal' && typeof node.value === 'string') return node.value;
  if (node?.type === 'TemplateLiteral' && !node.expressions.length) return node.quasis[0].value.cooked;
  if (node?.type === 'BinaryExpression' && node.operator === '+') {
    const left = staticText(node.left), right = staticText(node.right);
    if (left !== undefined && right !== undefined) return left + right;
  }
}
function propertyName(node: Node): string | undefined {
  const key = node.property ?? node.key;
  return node.computed ? staticText(key) : key?.name ?? staticText(key);
}

function inspectModule(file: string, source: string) {
  const { program, errors } = parseSync(file, source, { lang: 'ts' });
  assert.deepEqual(errors, [], file + ' must parse');
  const issues: string[] = [];
  const dependencies: string[] = [];
  const renderers = new Set<string>();
  const componentMounters = new Set<string>();
  const uiNamespaces = new Set<string>();
  const report = (node: Node, issue: string) => issues.push(file + ':' + source.slice(0, node.start).split('\n').length + ' ' + issue);
  for (const entry of program.body as Node[]) {
    if (entry.type !== 'ImportDeclaration' || entry.source.value !== '@acaprino/imago/ui') continue;
    for (const spec of entry.specifiers) {
      if (spec.type === 'ImportNamespaceSpecifier') uiNamespaces.add(spec.local.name);
      else if (spec.imported?.name === 'componentElement') componentMounters.add(spec.local.name);
      else if (spec.imported?.name?.endsWith('Html')) renderers.add(spec.local.name);
    }
  }
  const toolkitCall = (node: Node, names: Set<string>, nameTest: (name: string) => boolean) =>
    node.type === 'CallExpression' && (names.has(node.callee.name) ||
      (node.callee.type === 'MemberExpression' && uiNamespaces.has(node.callee.object.name) && nameTest(propertyName(node.callee) ?? '')));
  const trustedMarkup = (node?: Node) => !!node && toolkitCall(node, renderers, name => name.endsWith('Html'));
  walk(program as Node, (node, parents) => {
    const parent = parents.at(-1);
    if (node.type === 'ImportDeclaration' || node.type === 'ImportExpression' || node.type.startsWith('Export') && node.source) {
      const spec = staticText(node.source);
      if (!spec) { report(node, 'nonliteral module path'); return; }
      dependencies.push(spec);
      if (/\.css(?:[?#]|$)/.test(spec) && !spec.startsWith('.') && !spec.startsWith('/') && spec !== '@acaprino/imago/css/imago.css') {
        report(node, 'unapproved external stylesheet ' + spec);
      }
      if (spec.startsWith('@acaprino/imago') && !publicImports.has(spec.slice('@acaprino/imago/'.length))) report(node, 'private Imago import ' + spec);
    }
    if (node.type === 'NewExpression' && ['Option', 'Image', 'DOMParser', 'CSSStyleSheet'].includes(node.callee.name ?? propertyName(node.callee))) {
      report(node, 'native UI constructor');
    }
    if (node.type === 'Property' && parent?.type === 'ObjectPattern' && constructors.has(propertyName(node) ?? '')) report(node, 'aliased native DOM operation');
    if (node.type === 'MemberExpression') {
      const key = propertyName(node);
      if (key && constructors.has(key)) {
        const declaration = parents.at(-2);
        const allowed = key === 'createElement' && node.object.name === 'document' && parent?.type === 'CallExpression' &&
          parent.callee === node && declaration?.type === 'VariableDeclarator' && declaration.init === parent &&
          engineNodes.has([file, declaration.id.name, staticText(parent.arguments[0])].join(':'));
        if (!allowed) report(node, 'native DOM operation ' + key);
      }
      if (key && contentProperties.has(key) && parent?.type === 'AssignmentExpression' && parent.left === node) {
        const rhs = parent.right;
        const approved = key === 'innerHTML' && parent.operator === '=' && (trustedMarkup(rhs) ||
          file === 'src/files-panel.ts' && node.object.name === 'preview' && rhs.type === 'MemberExpression' &&
          rhs.object.name === 'result' && propertyName(rhs) === 'html');
        if (!approved) report(node, 'unapproved markup or style sink ' + key);
      }
    }
    if (toolkitCall(node, componentMounters, key => key === 'componentElement') && !trustedMarkup(node.arguments[0])) {
      report(node, 'componentElement must mount Imago renderer output');
    }
    if (node.type === 'CallExpression' && node.callee.type === 'MemberExpression' &&
        ['write', 'writeln'].includes(propertyName(node.callee) ?? '') && node.callee.object.name === 'document') report(node, 'document markup writer');
    if (node.type === 'TemplateElement' ? tag.test(node.value.raw) : tag.test(staticText(node) ?? '')) report(node, 'raw GUI markup');
  });
  return { issues, dependencies };
}

const terminalSelectors = new Set([
  '.terminal-view', '.xterm', '.xterm-viewport', '.terminal-view.has-minimap', '.terminal-view.drop-target',
  '.terminal-view.has-minimap > .xterm .xterm-scrollable-element > .scrollbar.vertical',
  '.terminal-view.has-minimap > .xterm .xterm-viewport', '.terminal-view > .terminal-minimap', '.terminal-earlier', '.terminal-history-screen',
]);
const fileSelectors = new Set([
  '.cm-fold-mark', '.cm-fold-mark.open', '.files-preview .md-link', '.files-preview .md-image', '.files-tab',
]);
// The frame's one rule: the tiling workspace and the Settings view fill the content panel's body.
const shellSelectors = new Set(['.workspace-host', '.settings-view']);
// The screens' layout (src/screens.css): Imago's DOM parts carry names and almost no layout, so a screen's own classes are
// laid out here, from tokens. A selector is built from the screens' classes, attributes and tags, never from an Imago class
// (imago-check holds the values to tokens, and a name of Imago's to the package).
const screenSelector = (selector: string) => /^\.[a-z]/.test(selector) && !/(?<![\w-])\.im-|--im-\w+\s*:/.test(selector);
function inspectCss(file: string, source: string): string[] {
  const scope = file === 'src/terminal-content.css' ? terminalSelectors : file === 'src/files-panel.css' ? fileSelectors : file === 'src/shell-content.css' ? shellSelectors : null;
  const screens = file === 'src/screens.css';
  if (!scope && !screens) return [file + ' is not a specialist content stylesheet'];
  const issues: string[] = [];
  const root = postcss.parse(source, { from: file });
  root.walkRules(rule => {
    for (const selector of rule.selectors) if (scope ? !scope.has(selector) : !screenSelector(selector)) issues.push(file + ' unsupported selector ' + selector);
  });
  root.walkAtRules(rule => {
    if (file === 'src/screens.css' && rule.name === 'container') return; // the shell's own breakpoints, as Imago's CSS writes them
    if (!(file === 'src/terminal-content.css' && rule.name === 'import' && /^(['"])@xterm\/xterm\/css\/xterm\.css\1$/.test(rule.params))) {
      issues.push(file + ' unsupported CSS at-rule @' + rule.name);
    }
  });
  return issues;
}

test('both shipped desktop entry closures compose public Imago controls', () => {
  const visited = new Set<string>();
  const issues: string[] = [];
  function visit(path: string) {
    if (visited.has(path)) return;
    visited.add(path);
    const file = relative(desktop, path).replaceAll('\\', '/');
    const source = readFileSync(path, 'utf8');
    if (file.endsWith('.css')) { issues.push(...inspectCss(file, source)); return; }
    const result = inspectModule(file, source);
    issues.push(...result.issues);
    for (const spec of result.dependencies) {
      if (!spec.startsWith('.') && !spec.startsWith('/')) continue;
      const dep = resolve(spec.startsWith('/') ? desktop : dirname(path), spec.replace(/^\//, '').split('?')[0]);
      if (/\.(?:png|jpg|svg|webp|woff2)$/.test(dep)) continue;
      const target = [dep, dep + '.ts', resolve(dep, 'index.ts')].find(p => extname(p) && existsSync(p));
      assert.ok(target, file + ' unresolved local import ' + spec);
      visit(target);
    }
  }
  for (const [html, mount] of [['index.html', 'app'], ['widget.html', 'widget']]) {
    const source = readFileSync(resolve(desktop, html), 'utf8');
    const tags: string[] = [];
    const entries: string[] = [];
    let mountCount = 0;
    htmlLanguage.parser.parse(source).iterate({ enter(cursor) {
      if (cursor.name !== 'OpenTag' && cursor.name !== 'SelfClosingTag') return;
      const element = cursor.node;
      const value = (range: { from: number; to: number }) => source.slice(range.from, range.to);
      const name = value(element.getChild('TagName')!).toLowerCase();
      tags.push(name);
      const attrs = Object.fromEntries(element.getChildren('Attribute').map(attr => {
        const val = attr.getChild('AttributeValue');
        return [value(attr.getChild('AttributeName')!).toLowerCase(), val ? value(val).replace(/^['"]|['"]$/g, '') : ''];
      }));
      if (name === 'div' && attrs.id === mount) mountCount++;
      if (name === 'script' && attrs.type === 'module' && /^\/src\/[\w/-]+\.ts$/.test(attrs.src)) entries.push(attrs.src);
      if (name === 'link') assert.equal(attrs.rel, 'icon', html + ' unexpected linked resource');
      for (const attr of Object.keys(attrs)) assert.ok(!/^on|^style$/.test(attr), html + ' inline UI or handler');
    } });
    const bootstrap = new Set(['html', 'head', 'body', 'meta', 'title', 'link']);
    assert.deepEqual(tags.filter(name => !bootstrap.has(name)), ['div', 'script'], html + ' may contain only a mount and a module entry');
    assert.equal(mountCount, 1, html + ' missing shell mount');
    assert.equal(entries.length, 1, html + ' missing module entry');
    visit(resolve(desktop, entries[0].slice(1)));
  }
  for (const name of readdirSync(resolve(desktop, 'src'), { recursive: true, encoding: 'utf8' })) {
    if (name.endsWith('.css')) visit(resolve(desktop, 'src', name));
  }
  assert.deepEqual(issues, []);
});

test('engine files cannot host extra controls or raw markup through an alias', () => {
  const source = [
    "import { componentElement as mount } from '@acaprino/imago/ui';",
    "import '@acaprino/imago/ts/ui/menu';",
    "const canvas = document.createElement('canvas');",
    "const toolbar = document.createElement('button');",
    "const {createElement: make} = document; make('select');",
    "const bound = document['create' + 'Element'].bind(document);",
    "const text = '<bu' + 'tton>Go</button>'; mount(text);",
    "canvas['inner' + 'HTML'] = text;",
  ].join('\n');
  const { issues } = inspectModule('src/browser-tab.ts', source);
  for (const reason of ['private Imago import', 'native DOM operation', 'aliased native DOM operation',
    'raw GUI markup', 'must mount Imago renderer output', 'unapproved markup']) assert.ok(issues.some(issue => issue.includes(reason)), reason);
  assert.ok(!issues.some(issue => issue.startsWith('src/browser-tab.ts:3 ')), 'the Moli canvas mount remains allowed');
  assert.ok(issues.some(issue => issue.startsWith('src/browser-tab.ts:4 ')), 'the same engine cannot build toolbar buttons');
  assert.ok(issues.some(issue => issue.startsWith('src/browser-tab.ts:6 ')), 'computed method aliases are checked');
  assert.deepEqual(inspectCss('src/files-panel.css', '.files-preview .md-link, .application-button {color:red}'),
    ['src/files-panel.css unsupported selector .application-button']);
});

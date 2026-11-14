import test from 'node:test';
import assert from 'node:assert/strict';
import { classifyTarget, findLinks, hoverText } from '../src/terminal-links.ts';

const targets = (line: string) => findLinks(line).map(link => link.target);

test('finds http and https and trims trailing punctuation', () => {
  assert.deepEqual(targets('see https://a.b/c). and http://x.y/z, done'), ['https://a.b/c', 'http://x.y/z']);
  assert.deepEqual(targets('"https://q.r/s?t=1#u"'), ['https://q.r/s?t=1#u']);
});

test('keeps balanced parentheses', () => {
  assert.deepEqual(targets('read https://en.wikipedia.org/wiki/Rust_(programming_language).'), ['https://en.wikipedia.org/wiki/Rust_(programming_language)']);
});

test('the link span covers exactly the target', () => {
  const [link] = findLinks('> https://a.b/c.');
  assert.equal('> https://a.b/c.'.slice(link.start, link.end), 'https://a.b/c');
});

test('finds local html paths but not inside a web url', () => {
  assert.deepEqual(targets(String.raw`wrote docs\report.html and D:\p\site\index.htm`), [String.raw`docs\report.html`, String.raw`D:\p\site\index.htm`]);
  assert.deepEqual(targets('https://a.b/page.html'), ['https://a.b/page.html']);
  assert.deepEqual(targets('x.htmlx y.html5'), []);
});

test('refuses mailto and custom schemes', () => {
  assert.equal(classifyTarget('mailto:x@y.z', 'D:\\p').kind, 'refused');
  assert.equal(classifyTarget('vscode://file/x', 'D:\\p').kind, 'refused');
  assert.equal(classifyTarget('javascript:alert(1)', 'D:\\p').kind, 'refused');
  assert.equal(classifyTarget('file:///D:/p/notes.txt', 'D:\\p').kind, 'refused');
});

test('an html path becomes a file under the terminal folder', () => {
  assert.deepEqual(classifyTarget(String.raw`docs\a.html`, String.raw`D:\p`), { kind: 'file', path: String.raw`D:\p\docs\a.html` });
  assert.deepEqual(classifyTarget(String.raw`..\b.html`, String.raw`D:\p\q`), { kind: 'file', path: String.raw`D:\p\b.html` });
  assert.deepEqual(classifyTarget('D:/x/y.htm', 'C:\\'), { kind: 'file', path: String.raw`D:\x\y.htm` });
  assert.deepEqual(classifyTarget('file:///D:/a%20b/c.html', 'C:\\'), { kind: 'file', path: String.raw`D:\a b\c.html` });
});

test('web targets are normalised URLs', () => {
  assert.deepEqual(classifyTarget('https://EXAMPLE.com/a b', ''), { kind: 'web', url: 'https://example.com/a%20b' });
});

test('osc8 hover shows the real target', () => {
  // An OSC 8 link whose visible text says good.example but whose URI is evil.example.
  const hover = hoverText(classifyTarget('https://evil.example/', 'D:\\p'));
  assert.match(hover, /evil\.example/);
  assert.doesNotMatch(hover, /good\.example/);
});

// Cases from the second review. node --experimental-strip-types --test mod-tests/
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { rewrite } from '../shn-render/hooks/register.ts'
import { mathToUnicode } from '../shn-render/hooks/lib/math.ts'

const same = (text: string) => assert.equal(rewrite(text, 120), text)
const flow = (head: string, body: string) => '```mermaid\nflowchart ' + head + '\n' + body + '\n```'
const drawn = (text: string) => rewrite(text, 120).startsWith('```text')

test('fences on list lines, nested, indented or ended by their container', () => {
  assert.equal(rewrite('- ```bash\n  echo $x^2$\n  ```\n\nThen $y_1$ here\n\n```\ncode $z^2$\n```', 120),
    '- ```bash\n  echo $x^2$\n  ```\n\nThen y₁ here\n\n```\ncode $z^2$\n```')
  same('```markdown\n> ```\n$x^2$ in code\n```')
  same('```md\n    ```\necho $x^2$\n```')
  same('Text\n\n    ```\n\nprose\n\n```\necho $x^2$\n```')
  same('> ```\n> a\n\nText\n\n```\necho $x^2$\n```')
})

test('code spans and HTML code', () => {
  same('Run `echo\n$$ x^2 $$\nfoo` now')
  same('    echo `\n`$x^2$` is code')
  same('<code>$x^2$</code> and <pre>$y_1$</pre>')
  same('\\\\`$x^2$`')
})

test('quoted display math keeps every character', () => {
  same('> $$\nx = y\n> $$')
  assert.equal(rewrite('> $$\n>a = b\n> $$', 120), '> a = b')
})

test('plain text that looks a little like math', () => {
  same('\\[TODO\\]')
  same('Refs:\n\n\\[1\\]\n')
  same('Paths like ${HOME}/${USER} stay')
  same('https://example.com/$foo$/bar')
  same('[a](http://x.com/?q=$x^2$)')
  same('Group with \\(foo_bar\\) in sed')
  same('$snake_case$ and $x$')
})

test('a conversion that would break a table is not made', () => {
  same('| a | b |\n|---|---|\n| $x \\mid y$ | 2 |')
  assert.equal(rewrite('| a | b |\n|---|---|\n| $x^2$ | 2 |', 120), '| a | b |\n|---|---|\n| x² | 2 |')
})

test('fractions in braces, double scripts, factorials', () => {
  assert.equal(mathToUnicode('x/{\\frac{a}{b}}'), 'x/(a/b)')
  assert.equal(mathToUnicode('{\\frac{a}{b}}c'), '(a/b)c')
  assert.equal(mathToUnicode('{x^2}^3'), '(x²)³')
  assert.equal(mathToUnicode('x_i^2'), 'xᵢ²')
  assert.equal(mathToUnicode('\\frac{a}{b}!'), '(a/b)!')
  assert.equal(mathToUnicode('\\mathbf{x_1}'), 'x₁')
  assert.equal(mathToUnicode('\\frac𝑎𝑏'), null)
  assert.equal(mathToUnicode('\\sqrt 𝑥'), null)
})

test('diagrams that cannot be checked stay as code', () => {
  assert.ok(!drawn(flow('TD', '  U[Utente richiesta] -- richiesta --> S[Server]\n  S --> D[Database]')), 'edge label inside a box label')
  assert.ok(!drawn(flow('LR', '  A[✅ Done] --> B[⚡ Fast]')), 'emoji')
  assert.ok(!drawn(flow('LR', '  subgraph S1[Group]\n    B --> C\n  end\n  A --> S1')), 'subgraph')
  same('1. Flow:\n   ```mermaid\n   flowchart LR\n     A --> B\n   ```')
  assert.ok(drawn(flow('LR', '  A[Client] -->|HTTP| B[API]\n  B -->|SQL| C[Database]')), 'a plain flowchart is still drawn')
})

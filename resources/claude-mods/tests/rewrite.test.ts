// node --experimental-strip-types --test test/rewrite.test.ts
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { rewrite } from '../shn-render/hooks/register.ts'
import { mathToUnicode } from '../shn-render/hooks/lib/math.ts'

const same = (text: string) => assert.equal(rewrite(text, 120), text)

test('code is never touched', () => {
  same('1. Run:\n   ```bash\n   echo $$ ; kill $$\n   sed \'s/\\(a\\)/\\1/\' $x$\n   ```')
  same('> ```sh\n> echo $x$ and $$\n> ```')
  same('Text\n\n    indented code $x^2$ \\(a_1\\)\n\nmore')
  same('Use `$x$` and ``a `$y$` b`` here.')
  same('Open fence still streaming:\n```python\nprint("$x$")')
})

test('an empty or longer-closed block does not swallow what follows', () => {
  assert.equal(rewrite('```\n```\nthen $x^2$ here', 120), '```\n```\nthen x² here')
  assert.equal(rewrite('````\ncode $x$\n`````\nand $y_1$', 120), '````\ncode $x$\n`````\nand y₁')
})

test('ordinary text with dollars and backslashes stays', () => {
  same('PID $$ and again $$')
  same('use \\( and \\) to group, escaped \\[link\\] text')
  same('Costs $5 and then $10, or $HOME/$USER.')
  same('Just $x$ alone')
  same('Escaped \\$x\\$ stays.')
  same('Unknown $\\foo{x}$ stays.')
  same('Aligned $$a & b$$ stays')
})

test('math is converted where it is plainly math', () => {
  assert.equal(rewrite('Ok $x^2 + y_1 \\le \\alpha$.', 120), 'Ok x² + y₁ ≤ α.')
  assert.equal(rewrite('Inline \\(a_1 + b^2\\) here', 120), 'Inline a₁ + b² here')
  assert.equal(rewrite('Law:\n\n$$\nS = \\frac{1}{(1 - p) + \\frac{p}{s}}\n$$\n\nend', 120), 'Law:\n\nS = 1/((1 − p) + p/s)\n\nend')
  assert.equal(rewrite('\\[ \\sum_{i=1}^{n} t_i \\]', 120), '∑ᵢ₌₁ⁿ tᵢ')
  assert.equal(rewrite('> $$ x^2 $$', 120), '> x²')
})

test('fractions and roots keep their meaning', () => {
  assert.equal(mathToUnicode('\\frac{a}{b}^2'), '(a/b)²')
  assert.equal(mathToUnicode('\\sqrt{x}^2'), '(√x)²')
  assert.equal(mathToUnicode('\\frac{a}{b}c'), '(a/b)c')
  assert.equal(mathToUnicode('x/\\frac{a}{b}'), 'x/(a/b)')
  assert.equal(mathToUnicode('\\frac{a}{bc}'), 'a/(bc)')
  assert.equal(mathToUnicode('\\frac{a+b}{c}'), '(a+b)/c')
  assert.equal(mathToUnicode('1 - \\frac{a}{b}'), '1 − a/b')
  assert.equal(mathToUnicode('e^{i\\pi}'), 'e^(iπ)')
  assert.equal(mathToUnicode('\\sqrt[3]{x}'), null)
})

test('braces and sets', () => {
  assert.equal(mathToUnicode('\\{1,2\\}'), '{1,2}')
  assert.equal(mathToUnicode('\\{x \\mid x > 0\\}'), '{x | x > 0}')
  assert.equal(mathToUnicode('\\left\\{ a \\right\\}'), '{ a }')
  assert.equal(mathToUnicode('\\text{e-mail}'), 'e-mail')
  assert.equal(mathToUnicode('a}'), null)
})

const flow = (body: string) => '```mermaid\nflowchart LR\n' + body + '\n```'
const drawn = (text: string) => rewrite(text, 120).startsWith('```text')

test('a simple flowchart is drawn, with every label', () => {
  const out = rewrite(flow('  A[Client] -->|HTTP| B[API]\n  B -->|SQL| C[Database]\n  B --> D[Cache]'), 120)
  assert.ok(out.startsWith('```text\n'))
  for (const label of ['Client', 'HTTP', 'API', 'SQL', 'Database', 'Cache']) assert.ok(out.includes(label), label)
})

test('diagrams that could be drawn wrong stay as code', () => {
  assert.ok(!drawn(flow('  A[Client] -->|HTTP| B[API]\n  B -->|SQL| C[(Database)]\n  C -->|310 ms| B')), 'cycle')
  assert.ok(!drawn(flow('  A -- yes --> B\n  B -- no --> A\n  A -- maybe --> C\n  C -- sure --> A')), 'text edge labels on a cycle')
  assert.ok(!drawn(flow('  A[' + '数据库'.repeat(10) + '] --> B[x]')), 'wide characters')
  assert.ok(!drawn(flow('  A["**bold**"] --> B[x]')), 'formatting in a label')
  assert.ok(!drawn('```mermaid\nsequenceDiagram\n  A->>B: hi\n```'), 'other diagram types')
  assert.equal(rewrite(flow('  A --> B'), 20), flow('  A --> B'), 'too wide for the pane')
  assert.ok(!drawn('```mermaid\nflowchart TD\n  U[Utente] -- richiesta --> S[Server]\n  S --> D[Database]\n```'), 'edge label drawn into a border')
  same('     ```mermaid\n     flowchart LR\n       A --> B\n     ```')
})

test('text edge labels are checked too', () => {
  const out = rewrite(flow('  A -- yes --> B\n  A -- no --> C'), 120)
  if (out.startsWith('```text')) for (const label of ['yes', 'no']) assert.ok(out.includes(label), label)
})

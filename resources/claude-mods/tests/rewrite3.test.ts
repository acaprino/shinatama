// Cases from the third review.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { rewrite } from '../shn-render/hooks/register.ts'
import { mathToUnicode } from '../shn-render/hooks/lib/math.ts'

const same = (text: string) => assert.equal(rewrite(text, 120), text)
const flow = (head: string, body: string) => '```mermaid\nflowchart ' + head + '\n' + body + '\n```'
const drawn = (text: string) => rewrite(text, 120).startsWith('```text')

test('statements the renderer would drop or misread keep the diagram as code', () => {
  for (const body of [
    '  A[Start] --x B[End]', '  A[Start] --o B[End]', '  A[Start] x--x B[End]', '  A[Start] ~~~ B[End]',
    '  A[Start] --> |go| B[End]', '  A --x C[Cancel]\n  A --> B[End]', '  A[Start] --> B[End];B-->C[X]',
    '  direction TB\n  A --> B', '  A --> B\n  click A "http://x"', '  accTitle: Hello\n  A --> B',
    '  A[Start] --> B\n  A[Begin]', '  A & B --> C', '  A --> B\n  style A fill:#f00',
  ]) assert.ok(!drawn(flow('LR', body)), body)
})

test('only forests are drawn: no two edges into one node', () => {
  assert.ok(!drawn(flow('TD', '  A[Start] --> B[End]\n  B --> C[Fin]\n  C --> D[Last]\n  A --> D')), 'merging lines')
  assert.ok(drawn(flow('LR', '  A[Client] -->|HTTP| B[API]\n  B -->|SQL| C[Database]\n  B --> D[Cache]')), 'a tree is drawn')
})

test('fullwidth letters and digits stay as code', () => {
  assert.ok(!drawn(flow('LR', '  A[ＡＰＩ] --> B[DB]')))
  assert.ok(!drawn(flow('LR', '  A[ｘ１２] --> B[DB]')))
})

test('shell and template expansions are not math', () => {
  same('Compare "${a}" = "${b}" in bash')
  same('Use ${A:=x}${B} here')
  same('Run $(foo)=$(bar) now')
  same('if ${{ a }}==${{ b }} then')
  same('T = ${x}<${y}')
})

test('an escaped backslash before ( is not a math opener', () => {
  same('a\\\\(b = c\\)')
  same('see \\\\(x_1, x_2\\) here')
})

test('a fraction next to brackets, bars and other factors keeps its meaning', () => {
  assert.equal(mathToUnicode('\\frac{h}{2}[f(a)+f(b)]'), '(h/2)[f(a)+f(b)]')
  assert.equal(mathToUnicode('\\frac{1}{2}|x|'), '(1/2)|x|')
  assert.equal(mathToUnicode('\\frac{1}{2}\\|x\\|'), '(1/2)‖x‖')
  assert.equal(mathToUnicode('\\frac{a}{b}\\langle x\\rangle'), '(a/b)⟨ x⟩')
  assert.equal(mathToUnicode('\\frac{a}{b}\\infty'), '(a/b)∞')
  assert.equal(mathToUnicode('\\frac{a}{b} = 1'), 'a/b = 1')
})

test('a one-character script without a Unicode form is closed before a factor', () => {
  assert.equal(mathToUnicode('x^\\alpha\\beta'), 'x^(α)β')
  assert.equal(mathToUnicode('e^st'), 'e^(s)t')
  assert.equal(mathToUnicode('a_Bc'), 'a_(B)c')
  assert.equal(mathToUnicode('e^\\alpha + 1'), 'e^α + 1')
  assert.equal(mathToUnicode('x^\\alpha_1'), 'x^(α)₁')
})

test('a fraction at the edge of a group is judged by its real neighbours', () => {
  assert.equal(mathToUnicode('{x+\\frac{a}{b}}c'), 'x+(a/b)c')
  assert.equal(mathToUnicode('\\mathbf{x+\\frac{a}{b}}c'), 'x+(a/b)c')
  assert.equal(mathToUnicode('{a+b}^2'), '(a+b)²')
})

test('a TeX comment means the formula is left alone', () => {
  same('$$\nx = 1 % only if y > 0\n$$')
  assert.equal(mathToUnicode('50\\% = x'), '50% = x')
})

test('a long answer with one unsafe cell stays fast', () => {
  const body = Array.from({ length: 39 }, (_, i) => `Line ${i} has $x_${i % 10} + y^2$ in it.`).join('\n\n')
  const text = body + '\n\n| a | b |\n|---|---|\n| $x \\mid y$ | 2 |\n\n' + 'filler text. '.repeat(1500)
  const start = performance.now()
  const out = rewrite(text, 120)
  const ms = performance.now() - start
  assert.ok(out.includes('Line 0 has x₀ + y² in it.'))
  assert.ok(out.includes('| $x \\mid y$ | 2 |'))
  assert.ok(ms < 1000, `took ${ms.toFixed(0)} ms`)
  // The same answer again (a repaint) comes from the caches.
  const again = performance.now(); rewrite(text + ' ', 120)
  assert.ok(performance.now() - again < 500, 'a streamed chunk reuses the checked blocks')
})

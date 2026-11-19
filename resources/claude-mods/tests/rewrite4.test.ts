// Cases from the fourth review.
import { test } from 'node:test'
import assert from 'node:assert/strict'
import { rewrite } from '../shn-render/hooks/register.ts'
import { mathToUnicode } from '../shn-render/hooks/lib/math.ts'

const same = (text: string) => assert.equal(rewrite(text, 120), text)
const flow = (head: string, body: string) => '```mermaid\nflowchart ' + head + '\n' + body + '\n```'
const drawn = (text: string) => rewrite(text, 120).startsWith('```text')

test('branches are drawn only where the layout reads unambiguously', () => {
  assert.ok(!drawn(flow('TD', '  A[P] --> B[x1]\n  A --> C[x2]\n  D[Q] --> E[y1]\n  D --> F[y2]')), 'two parents, top-down')
  assert.ok(!drawn(flow('TD', '  R[Root] --> P[P1]\n  R --> Q[Q1]\n  P --> B[x1]\n  P --> C[x2]')), 'nested branches')
  assert.ok(!drawn(flow('LR', '  A[P] --> B[x1]\n  A --> C[x2]\n  D[Q] --> E[y1]\n  D --> F[y2]')), 'two roots')
  assert.ok(!drawn(flow('TD', '  A[Radice] --> B[Uno]\n  A --> C[Due]')), 'top-down branch')
  assert.ok(!drawn(flow('BT', '  A[Uno] --> B[Due]')), 'bottom-up')
  assert.ok(!drawn(flow('LR', '  A[API] -->|SQL| B[Database]\n  A -->|GET| C[Cache]')), 'labelled branch drawn apart from its box')
  assert.ok(drawn(flow('TD', '  A[Uno] --> B[Due]\n  B --> C[Tre]')), 'a top-down chain')
  assert.ok(drawn(flow('LR', '  A[Radice] --> B[Uno]\n  A --> C[Due]\n  A --> D[Tre]')), 'one left-right branch')
})

test('labels must be told apart', () => {
  assert.ok(!drawn(flow('LR', '  A[DB] --> B[DB replica]')), 'a label inside another')
  assert.ok(!drawn(flow('LR', '  A[S] -->|ab| B[T]\n  A -->|ab| C[U]')), 'a repeated edge label')
  assert.ok(!drawn(flow('LR', '  A[x] -->|HTTP| B[y]\n  B -->|HTTPS| C[z]')), 'an edge label inside another')
})

test('shape syntax and zero-width characters stay as code', () => {
  assert.ok(!drawn(flow('LR', '  A[/para/] --> B[x]')), 'parallelogram')
  assert.ok(!drawn(flow('LR', '  A[\\\\trap\\\\] --> B[x]')), 'trapezoid')
  assert.ok(!drawn(flow('LR', '  A[co\u00ADop] --> B[x]')), 'soft hyphen')
  assert.ok(!drawn(flow('LR', '  A[a\u0483b] --> B[x]')), 'combining Cyrillic mark')
})

test('isotopes, identifiers, half-convertible display blocks', () => {
  assert.equal(mathToUnicode('{}^{14}C'), '¹⁴C')
  assert.equal(rewrite('Carbon \\({}^{14}C\\) decays', 120), 'Carbon ¹⁴C decays')
  same('Set $MY_VAR_2$ in the IDE')
  same('Use $FILE_X$ here')
  assert.equal(rewrite('Ok $x_1$ and $a_b$', 120), 'Ok x₁ and a_b')
  same('$$\na $b_1$ c \\foo\n$$')
})

test('text inside \\text{} with TeX-special characters, and operator spacing', () => {
  assert.equal(mathToUnicode('\\text{Mr.~Smith} = x'), null)
  assert.equal(mathToUnicode('\\text{50% off}'), null)
  assert.equal(mathToUnicode('\\ln{x} + \\exp{y}'), 'ln x + exp y')
  assert.equal(mathToUnicode('\\log{n}'), 'log n')
  assert.equal(mathToUnicode('\\sin(x)'), 'sin(x)')
  assert.equal(mathToUnicode('\\lim_{x \\to 0} f(x)'), 'lim_(x → 0) f(x)')
})

test('many math blocks stay fast while streaming', () => {
  const text = Array.from({ length: 200 }, (_, i) => `Paragraph ${i} with $x_${i % 10} + y^2$ and more words here.`).join('\n\n')
  rewrite(text, 120)
  const start = performance.now()
  rewrite(text + ' tail', 120)
  const ms = performance.now() - start
  assert.ok(ms < 250, `a streamed chunk took ${ms.toFixed(1)} ms`)
})

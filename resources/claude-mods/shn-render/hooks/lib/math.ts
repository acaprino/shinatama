// LaTeX to one line of Unicode. Returns null when anything is not understood,
// so the caller keeps the original source: no half-converted formula is drawn.
//
// The formula is parsed into atoms first, then joined, so a fraction or a root
// gets parentheses wherever the plain-text reading would otherwise change its
// meaning: before a script (`(a/b)²`), next to another factor (`(a/b)c`), after
// a division (`x/(a/b)`).

const SYMBOLS: Record<string, string> = {
  alpha: 'α', beta: 'β', gamma: 'γ', delta: 'δ', epsilon: 'ε', varepsilon: 'ε', zeta: 'ζ', eta: 'η', theta: 'θ', iota: 'ι', kappa: 'κ',
  lambda: 'λ', mu: 'μ', nu: 'ν', xi: 'ξ', pi: 'π', rho: 'ρ', sigma: 'σ', tau: 'τ', phi: 'φ', varphi: 'φ', chi: 'χ', psi: 'ψ', omega: 'ω',
  Gamma: 'Γ', Delta: 'Δ', Theta: 'Θ', Lambda: 'Λ', Xi: 'Ξ', Pi: 'Π', Sigma: 'Σ', Phi: 'Φ', Psi: 'Ψ', Omega: 'Ω',
  sum: '∑', prod: '∏', int: '∫', oint: '∮', partial: '∂', nabla: '∇', infty: '∞', cdot: '·', times: '×', div: '÷', pm: '±', mp: '∓',
  le: '≤', leq: '≤', ge: '≥', geq: '≥', ne: '≠', neq: '≠', approx: '≈', equiv: '≡', sim: '∼', propto: '∝', ll: '≪', gg: '≫',
  to: '→', rightarrow: '→', leftarrow: '←', Rightarrow: '⇒', Leftarrow: '⇐', leftrightarrow: '↔', iff: '⇔', implies: '⇒', mapsto: '↦',
  in: '∈', notin: '∉', subset: '⊂', subseteq: '⊆', cup: '∪', cap: '∩', emptyset: '∅', forall: '∀', exists: '∃', neg: '¬', land: '∧', lor: '∨',
  ldots: '…', cdots: '⋯', dots: '…', angle: '∠', perp: '⊥', parallel: '∥', circ: '∘', star: '⋆', hbar: 'ℏ', ell: 'ℓ', Re: 'ℜ', Im: 'ℑ',
  lim: 'lim', log: 'log', ln: 'ln', sin: 'sin', cos: 'cos', tan: 'tan', exp: 'exp', min: 'min', max: 'max', det: 'det',
  quad: '  ', qquad: '    ', displaystyle: '', langle: '⟨', rangle: '⟩', mid: '|',
}
// Literal braces travel as private-use placeholders, so a stray brace in the
// output can only mean a parse that went wrong.
const OPEN = '', CLOSE = ''
const DELIMITERS: Record<string, string> = { '(': '(', ')': ')', '[': '[', ']': ']', '|': '|', '.': '', '\\{': OPEN, '\\}': CLOSE, '\\lbrace': OPEN, '\\rbrace': CLOSE, '\\langle': '⟨', '\\rangle': '⟩', '\\|': '‖' }
// Text commands keep their words as written; font commands hold math.
const TEXT = new Set(['text', 'textrm', 'textbf', 'textit', 'operatorname'])
const FONT = new Set(['mathrm', 'mathbf', 'mathit', 'mathsf', 'boldsymbol', 'bm'])
const BLACKBOARD: Record<string, string> = { R: 'ℝ', N: 'ℕ', Z: 'ℤ', Q: 'ℚ', C: 'ℂ' }
const SUP: Record<string, string> = { '0': '⁰', '1': '¹', '2': '²', '3': '³', '4': '⁴', '5': '⁵', '6': '⁶', '7': '⁷', '8': '⁸', '9': '⁹', '+': '⁺', '−': '⁻', '=': '⁼', '(': '⁽', ')': '⁾', n: 'ⁿ', i: 'ⁱ', T: 'ᵀ', a: 'ᵃ', b: 'ᵇ', c: 'ᶜ', d: 'ᵈ', e: 'ᵉ', k: 'ᵏ', m: 'ᵐ', p: 'ᵖ', t: 'ᵗ', x: 'ˣ', y: 'ʸ' }
const SUB: Record<string, string> = { '0': '₀', '1': '₁', '2': '₂', '3': '₃', '4': '₄', '5': '₅', '6': '₆', '7': '₇', '8': '₈', '9': '₉', '+': '₊', '−': '₋', '=': '₌', '(': '₍', ')': '₎', a: 'ₐ', e: 'ₑ', h: 'ₕ', i: 'ᵢ', j: 'ⱼ', k: 'ₖ', l: 'ₗ', m: 'ₘ', n: 'ₙ', o: 'ₒ', p: 'ₚ', r: 'ᵣ', s: 'ₛ', t: 'ₜ', u: 'ᵤ', v: 'ᵥ', x: 'ₓ' }
const SCRIPT_CHARS = new Set([...Object.values(SUP), ...Object.values(SUB)].join(''))
const MAX_SOURCE = 400

type Kind = 'plain' | 'space' | 'group' | 'frac' | 'sqrt' | 'scripted' | 'operator'
// Named operators, set apart from a following letter: `ln x`, not `lnx`.
const OPERATORS = new Set(['lim', 'log', 'ln', 'sin', 'cos', 'tan', 'exp', 'min', 'max', 'det'])
// `alt` is the text with the last script parenthesized: `x^α` reads fine
// alone, `x^(α)β` is needed when a factor follows.
interface Atom { text: string; kind: Kind; sup?: boolean; sub?: boolean; alt?: string }
class Unknown extends Error {}

// One letter, or one number, with any scripts already on it.
function isToken(text: string): boolean {
  const core = [...text].filter(ch => !SCRIPT_CHARS.has(ch)).join('')
  return /^(\p{L}|\p{N}+(\.\p{N}+)?)$/u.test(core)
}
// A numerator needs no parentheses when it is a run of letters and digits.
const isProduct = (text: string) => /^[\p{L}\p{N}.]+$/u.test([...text].filter(ch => !SCRIPT_CHARS.has(ch)).join(''))
// Neighbours after which, or before which, a bare `a/b` or `√x` still reads
// as one unit: operators, relations, separators and brackets that open or
// close around it. Next to anything else it gets parentheses; extra ones are
// harmless, missing ones change the meaning.
const SAFE_AFTER = /^[=<>≤≥≠≈≡∼∝≪≫+−±∓×·,;:.)\]⟩→←⇒⇐↔⇔↦∈∉⊂⊆∪∩∧∨…⋯]/
const SAFE_BEFORE = /[=<>≤≥≠≈≡∼∝≪≫+−±∓×·,;:(\[⟨→←⇒⇐↔⇔↦∈∉⊂⊆∪∩∧∨]$/

function script(base: Atom, body: string, sup: boolean): Atom {
  const table = sup ? SUP : SUB
  const flat = body.replace(/ /g, '')
  if (!flat) throw new Unknown()
  // A second script of the same kind would run on: x² then ³ reads x²³.
  const again = sup ? base.sup : base.sub
  const wrapBase = base.kind === 'frac' || base.kind === 'sqrt' || (base.kind === 'group' && !isToken(base.text)) || !!again
  const text = base.alt ?? base.text
  const head = wrapBase ? `(${text})` : text
  const mark = sup ? '^' : '_'
  const kept = wrapBase ? {} : { sup: base.sup, sub: base.sub }
  const kind = { kind: 'scripted' as const, ...kept, ...(sup ? { sup: true } : { sub: true }) }
  if ([...flat].every(ch => table[ch])) return { text: head + [...flat].map(ch => table[ch]).join(''), ...kind }
  if ([...flat].length === 1) return { text: head + mark + flat, alt: `${head}${mark}(${flat})`, ...kind }
  return { text: `${head}${mark}(${body.trim()})`, ...kind }
}

// Joins atoms; a fraction, a root or an open script next to another factor is
// parenthesized.
function join(atoms: Atom[]): string {
  const solid = (index: number, step: 1 | -1) => {
    for (let i = index + step; i >= 0 && i < atoms.length; i += step) if (atoms[i].kind !== 'space' && atoms[i].text) return atoms[i]
    return undefined
  }
  return atoms.map((atom, index) => {
    const before = solid(index, -1), after = solid(index, 1)
    if (atom.alt !== undefined) return after !== undefined && !SAFE_AFTER.test(after.text) ? atom.alt : atom.text
    if (atom.kind === 'operator') {
      const touching = atoms[index + 1]
      return touching && touching.kind !== 'space' && /^[\p{L}\p{N}√∂]/u.test(touching.text) ? atom.text + ' ' : atom.text
    }
    if (atom.kind !== 'frac' && atom.kind !== 'sqrt') return atom.text
    const wrap = (after !== undefined && !SAFE_AFTER.test(after.text)) || (before !== undefined && !SAFE_BEFORE.test(before.text))
    return wrap ? `(${atom.text})` : atom.text
  }).join('')
}

class Parser {
  i = 0
  readonly src: string
  constructor(src: string) { this.src = src }

  // `{...}` or one atom, as text.
  argument(): string {
    this.skipSpaces()
    if (this.src[this.i] === '{') { this.i++; return join(this.sequence('}')).trim() }
    const atom = this.atom()
    if (!atom) throw new Unknown()
    return join(Array.isArray(atom) ? atom : [atom])
  }
  skipSpaces() { while (this.src[this.i] === ' ') this.i++ }

  // The raw text of `{...}` for \text and friends: no commands, no nesting.
  literal(): string {
    this.skipSpaces()
    if (this.src[this.i] !== '{') throw new Unknown()
    const end = this.src.indexOf('}', this.i)
    const body = end < 0 ? '' : this.src.slice(this.i + 1, end)
    // TeX reads these specially even in text (`~` a space, `%` a comment).
    if (end < 0 || /[\\{$~%&#^_]/.test(body)) throw new Unknown()
    this.i = end + 1
    return body
  }

  delimiter(): string {
    this.skipSpaces()
    for (const [key, value] of Object.entries(DELIMITERS)) {
      if (this.src.startsWith(key, this.i) && !(key.length > 2 && /[A-Za-z]/.test(this.src[this.i + key.length] ?? ''))) { this.i += key.length; return value }
    }
    throw new Unknown()
  }

  command(): Atom | Atom[] {
    this.i++
    const name = /^[A-Za-z]+/.exec(this.src.slice(this.i))?.[0]
    if (!name) {
      const ch = this.src[this.i++]
      if (ch === ',' || ch === ';' || ch === ':' || ch === ' ') return { text: ' ', kind: 'space' }
      if (ch === '!') return { text: '', kind: 'space' }
      if (ch === '{') return { text: OPEN, kind: 'plain' }
      if (ch === '}') return { text: CLOSE, kind: 'plain' }
      if (ch === '|') return { text: '‖', kind: 'plain' }
      if (ch !== undefined && '%$_&#'.includes(ch)) return { text: ch, kind: 'plain' }
      throw new Unknown()
    }
    this.i += name.length
    if (name === 'frac' || name === 'dfrac' || name === 'tfrac') {
      const top = this.argument(), bottom = this.argument()
      return { text: `${isProduct(top) ? top : `(${top})`}/${isToken(bottom) ? bottom : `(${bottom})`}`, kind: 'frac' }
    }
    if (name === 'sqrt') {
      this.skipSpaces()
      if (this.src[this.i] === '[') throw new Unknown()
      const body = this.argument()
      return { text: isToken(body) ? `√${body}` : `√(${body})`, kind: 'sqrt' }
    }
    if (name === 'mathbb') {
      const letter = this.argument()
      if (!BLACKBOARD[letter]) throw new Unknown()
      return { text: BLACKBOARD[letter], kind: 'plain' }
    }
    if (TEXT.has(name)) return { text: this.literal(), kind: 'group' }
    if (FONT.has(name)) {
      this.skipSpaces()
      if (this.src[this.i] !== '{') { const atom = this.atom(); if (!atom) throw new Unknown(); return atom }
      return this.group()
    }
    if (name === 'left' || name === 'right') return { text: this.delimiter(), kind: 'plain' }
    if (name === 'lbrace') return { text: OPEN, kind: 'plain' }
    if (name === 'rbrace') return { text: CLOSE, kind: 'plain' }
    if (!(name in SYMBOLS)) throw new Unknown()
    const text = SYMBOLS[name]
    return { text, kind: OPERATORS.has(name) ? 'operator' : text.trim() ? 'plain' : 'space' }
  }

  // One atom (a braced group may give several), or undefined at the end.
  atom(): Atom | Atom[] | undefined {
    const ch = this.src[this.i]
    if (ch === undefined) return undefined
    if (ch === '\\') return this.command()
    if (ch === '{') return this.group()
    // `%` starts a TeX comment: what follows is not part of the formula.
    if (ch === '}' || ch === '&' || ch === '^' || ch === '_' || ch === '#' || ch === '$' || ch === '%') throw new Unknown()
    this.i++
    if (ch === ' ' || ch === '~') return { text: ' ', kind: 'space' }
    if (ch === '-') return { text: '−', kind: 'plain' }
    return { text: ch, kind: 'plain' }
  }

  // `{...}`. Braces are transparent for reading, so their atoms join their
  // neighbours' (a fraction at the edge of a group is still judged by what is
  // next to it). Only before a script is a group one unit: `(a+b)²`.
  group(): Atom | Atom[] {
    this.i++
    const atoms = this.sequence('}')
    let j = this.i
    while (this.src[j] === ' ') j++
    if (this.src[j] !== '^' && this.src[j] !== '_') return atoms
    const solid = atoms.filter(atom => atom.kind !== 'space')
    // `{}^{14}C`: an empty base carries the script alone.
    if (solid.length === 0) return { text: '', kind: 'plain' }
    if (solid.length === 1) return solid[0]
    return { text: join(atoms), kind: 'group' }
  }

  sequence(stop: string | null): Atom[] {
    const atoms: Atom[] = []
    while (this.i < this.src.length) {
      const ch = this.src[this.i]
      if (stop && ch === stop) { this.i++; return atoms }
      if (ch === '^' || ch === '_') {
        this.i++
        let at = atoms.length - 1
        while (at >= 0 && atoms[at].kind === 'space') at--
        if (at < 0) throw new Unknown()
        const body = this.argument()
        atoms.splice(at, atoms.length - at, script(atoms[at], body, ch === '^'))
        continue
      }
      const atom = this.atom()
      if (Array.isArray(atom)) atoms.push(...atom)
      else if (atom) atoms.push(atom)
    }
    if (stop) throw new Unknown()
    return atoms
  }
}

export function mathToUnicode(latex: string): string | null {
  const src = latex.replace(/\s+/g, ' ').trim()
  // The parser steps through UTF-16 units: a character outside the basic
  // plane would be cut in half.
  if (!src || src.length > MAX_SOURCE || /[\uD800-\uDFFF]/.test(src)) return null
  try {
    const out = join(new Parser(src).sequence(null)).replace(/ {5,}/g, '    ').trim()
    if (!out || /[\\{}]/.test(out)) return null
    return out.replaceAll(OPEN, '{').replaceAll(CLOSE, '}')
  } catch { return null }
}

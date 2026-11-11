// Prototype. Draws ```mermaid flowcharts and LaTeX math of an answer as
// Unicode text. Display only: the hook hands a rewritten `text` to the engine's
// own drawing; the stored message, the next model request and ctrl+o keep the
// original.
//
// Safety: every math conversion is checked by parsing the whole answer again
// with a CommonMark/GFM lexer. A conversion is kept only when the parse is the
// same as before except for plain text: every code block, code span, HTML,
// link address and the structure (lists, tables, emphasis) unchanged. What is
// not understood for certain is left exactly as the engine would draw it.
import type { Register } from 'claude-code'
import { lex } from './lib/markdown.js'
import { parseMermaid, renderMermaidASCII } from './lib/mermaid.js'
import { mathToUnicode } from './lib/math.ts'

const DEFAULT_COLUMNS = 80
const MAX_NODES = 40
const MAX_CANDIDATES = 40
const CACHE_SIZE = 64

// A tiny LRU: work is reused while an answer streams and on every repaint.
function cache<V>(size = CACHE_SIZE) {
  const map = new Map<string, V>()
  return (key: string, make: () => V): V => {
    if (map.has(key)) { const value = map.get(key)!; map.delete(key); map.set(key, value); return value }
    const value = make()
    map.set(key, value)
    if (map.size > size) map.delete(map.keys().next().value!)
    return value
  }
}
const diagrams = cache<string | null>()
const answers = cache<string>()

// --- The parse that decides what may change ---------------------------------

interface Token { type: string; raw: string; text?: string; lang?: string; href?: string; title?: string | null; tokens?: Token[]; items?: Token[]; header?: Cell[]; rows?: Cell[][]; align?: unknown[]; ordered?: boolean; start?: number | ''; loose?: boolean; task?: boolean; checked?: boolean; depth?: number; codeBlockStyle?: string }
interface Cell { tokens: Token[] }

// The parse as a list of parts: structure, code, HTML and link addresses as
// they are, and each run of plain text as `T` or, with `words`, as its text.
function skeleton(tokens: Token[], words: boolean, out: string[] = []): string[] {
  const text = (piece: string) => {
    const last = out.length - 1
    if (last >= 0 && out[last].startsWith('T')) { if (words) out[last] += piece }
    else out.push(words ? 'T' + piece : 'T')
  }
  for (const token of tokens) {
    switch (token.type) {
      case 'text': case 'escape':
        if (token.tokens) skeleton(token.tokens, words, out)
        else text(token.text ?? '')
        break
      case 'code': case 'codespan': case 'html':
        out.push(`${token.type}:${token.raw}`)
        break
      case 'link': case 'image':
        out.push(`${token.type}:${token.href}\u0000${token.title ?? ''}`)
        if (token.tokens) skeleton(token.tokens, words, out)
        out.push('/')
        break
      default:
        out.push(`${token.type}:${token.depth ?? ''}:${token.ordered ?? ''}:${token.start ?? ''}:${token.loose ?? ''}:${token.task ?? ''}:${token.checked ?? ''}:${JSON.stringify(token.align ?? '')}`)
        if (token.tokens) skeleton(token.tokens, words, out)
        for (const item of token.items ?? []) skeleton([item], words, out)
        for (const cell of token.header ?? []) { out.push('|'); skeleton(cell.tokens, words, out) }
        for (const row of token.rows ?? []) { out.push('-'); for (const cell of row) { out.push('|'); skeleton(cell.tokens, words, out) } }
        out.push('/')
    }
  }
  return out
}
const shape = (text: string, words: boolean) => skeleton(lex(text) as Token[], words).join('\u0001')

// Changes are safe when (1) a neutral placeholder in each changed place leaves
// the structure as it was, so no change is inside code, HTML or an address;
// and (2) the real replacements read back as exactly that text, word for
// word, with nothing new parsed out of them (a table cell, emphasis, a list).
const mark = (index: number) => `${index}`
function safe(text: string, changes: Change[], before: string): boolean {
  const held = apply(text, changes.map((change, index) => ({ ...change, text: mark(index) })))
  if (shape(held, false) !== before) return false
  let expected = shape(held, true)
  changes.forEach((change, index) => { expected = expected.split(mark(index)).join(change.text) })
  return shape(apply(text, changes), true) === expected
}

// --- Mermaid ----------------------------------------------------------------

// Labels drawn one character per cell: ASCII letters and digits, the Latin,
// Greek and Cyrillic letters of the basic blocks (no fullwidth forms), spaces
// and plain punctuation. Anything else stays as code.
const PLAIN_LABEL = /^[A-Za-z0-9À-ÿĀ-ɏΆ-ώЀ-҂Ҋ-ӿ .,:;!?'/%+#@=-]+$/
const CORNERS = /[┌┐└┘╭╮╰╯]/
const EDGE = /[─━►▶◄◀▲▼]/
const CROSSING = /[┼╪╫╬]/

// The one grammar drawn: nodes `id`, `id[label]`, `id(label)`, `id{label}`,
// `id([label])`, `id[(label)]`, and edges `a --> b`, `a -->|label| b`,
// `a -- label --> b`, one statement per line. Any other line (directives,
// other arrows, `;`, `&`, styles, clicks) means the diagram stays as code.
const SHAPES: Record<string, string> = { '([': '])', '[(': ')]', '[': ']', '(': ')', '{': '}' }
const NODE = String.raw`([A-Za-z][A-Za-z0-9_]*)(?:(\(\[|\[\(|\[|\(|\{)([^\[\](){}|"<>;&]+)(\]\)|\)\]|\]|\)|\}))?`
const NODE_LINE = new RegExp(`^${NODE}$`)
const PIPE_EDGE = new RegExp(`^${NODE}\\s*-->\\s*(?:\\|([^|"<>;&]+)\\|\\s*)?${NODE}$`)
const TEXT_EDGE = new RegExp(`^${NODE}\\s+--\\s+([^\\-|"<>;&]+?)\\s+-->\\s*${NODE}$`)
const RESERVED = new Set(['end', 'graph', 'flowchart', 'subgraph', 'style', 'class', 'classdef', 'click', 'direction', 'linkstyle', 'acctitle', 'accdescr', 'default'])

interface Graph { labels: Map<string, string>; edges: { from: string; to: string; label: string }[] }

function strictGraph(source: string): Graph | null {
  const lines = source.split('\n').map(line => line.trim()).filter(line => line && !line.startsWith('%%'))
  const labels = new Map<string, string>()
  const edges: Graph['edges'] = []
  const node = (id: string, open?: string, text?: string, close?: string): boolean => {
    if (RESERVED.has(id.toLowerCase())) return false
    if (open === undefined) { if (!labels.has(id)) labels.set(id, id); return true }
    if (SHAPES[open] !== close) return false
    const label = text!.trim()
    // A node given two different labels is drawn with one of them.
    const known = labels.get(id)
    if (known !== undefined && known !== id && known !== label) return false
    labels.set(id, label)
    return true
  }
  for (const line of lines.slice(1)) {
    let m: RegExpExecArray | null
    if ((m = NODE_LINE.exec(line))) { if (!node(m[1], m[2], m[3], m[4])) return null; continue }
    if ((m = PIPE_EDGE.exec(line))) {
      if (!node(m[1], m[2], m[3], m[4]) || !node(m[6], m[7], m[8], m[9])) return null
      edges.push({ from: m[1], to: m[6], label: (m[5] ?? '').trim() }); continue
    }
    if ((m = TEXT_EDGE.exec(line))) {
      if (!node(m[1], m[2], m[3], m[4]) || !node(m[6], m[7], m[8], m[9])) return null
      edges.push({ from: m[1], to: m[6], label: m[5].trim() }); continue
    }
    return null
  }
  return labels.size ? { labels, edges } : null
}

// Our reading and the renderer's must agree exactly: the same nodes with the
// same labels and the same edges with the same labels.
function agrees(graph: Graph, source: string): boolean {
  const parsed = parseMermaid(source)
  if (parsed.subgraphs.length || parsed.nodes.size !== graph.labels.size) return false
  for (const [id, label] of graph.labels) if (parsed.nodes.get(id)?.label.trim() !== label) return false
  const key = (from: string, to: string, label: string) => `${from}\u0000${to}\u0000${label}`
  const ours = graph.edges.map(edge => key(edge.from, edge.to, edge.label)).sort()
  const theirs = parsed.edges.map(edge => key(edge.source, edge.target, (edge.label ?? '').trim())).sort()
  return ours.length === theirs.length && ours.every((edge, index) => edge === theirs[index])
}

// Node labels (boxes) and edge labels, or null for a graph not drawn. Only the
// shapes the text layout draws unambiguously: one tree (a single root, at most
// one edge into each node, so no lines from different sources meet) with at
// most one node that branches. Top-down, only a plain chain: the layout draws
// a branch as one bar of arrowheads that cannot be read by parent.
function labelsOf(source: string, direction: string): { boxes: string[]; edges: string[] } | null {
  const graph = strictGraph(source)
  if (!graph || graph.labels.size > MAX_NODES || !agrees(graph, source)) return null
  const into = new Map<string, number>()
  for (const edge of graph.edges) into.set(edge.to, (into.get(edge.to) ?? 0) + 1)
  if ([...into.values()].some(count => count > 1)) return null
  const next = new Map<string, string[]>()
  for (const edge of graph.edges) next.set(edge.from, [...(next.get(edge.from) ?? []), edge.to])
  const branching = [...next.values()].filter(targets => targets.length > 1).length
  if (branching > (direction === 'LR' ? 1 : 0)) return null
  const roots = [...graph.labels.keys()].filter(id => !into.has(id))
  if (roots.length !== 1) return null
  const seen = new Set<string>()
  const visit = (id: string) => { if (seen.has(id)) return; seen.add(id); for (const to of next.get(id) ?? []) visit(to) }
  visit(roots[0])
  // One root, in-degree at most one, every node reached: a tree, no cycle.
  if (seen.size !== graph.labels.size) return null
  const boxes = [...graph.labels.values()], edges = graph.edges.map(edge => edge.label).filter(Boolean)
  // Every label must be findable on its own: none repeated, none inside another.
  const all = [...boxes, ...edges]
  if (all.some((label, i) => all.some((other, j) => i !== j && other.includes(label)))) return null
  return { boxes, edges }
}

// A line that leaves a box border and then a gap before it starts: the edge
// would look as if it began nowhere (or at another box).
const DETACHED = /│ [├┤┬┴└┘┌┐─►◄]/

// An edge label is readable when it sits on an edge line, apart from box corners.
function onEdge(art: string, label: string): boolean {
  for (const line of art.split('\n')) {
    for (let at = line.indexOf(label); at >= 0; at = line.indexOf(label, at + 1)) {
      let start = at, end = at + label.length
      while (start > 0 && line[start - 1] !== ' ') start--
      while (end < line.length && line[end] !== ' ') end++
      const run = line.slice(start, end)
      if (EDGE.test(run) && !CORNERS.test(run)) return true
    }
  }
  return false
}

function draw(source: string, columns: number): string | null {
  const header = /^\s*(?:flowchart|graph)\s+(TD|TB|LR)\s*$/i.exec(source.split('\n').find(line => line.trim()) ?? '')
  if (!header) return null
  try {
    const found = labelsOf(source, header[1].toUpperCase())
    if (!found) return null
    const { boxes, edges } = found
    if (![...boxes, ...edges].every(label => PLAIN_LABEL.test(label) && !/^[/\\]|[/\\]$/.test(label))) return null
    const raw = renderMermaidASCII(source, { useAscii: false, colorMode: 'none', paddingX: 3, paddingY: 2 })
    const lines = raw.split('\n').map(line => line.replace(/\s+$/, ''))
    while (lines.length && !lines[0]) lines.shift()
    while (lines.length && !lines[lines.length - 1]) lines.pop()
    if (!lines.length || lines.some(line => [...line].length > columns - 8 || CROSSING.test(line) || DETACHED.test(line))) return null
    const art = lines.join('\n')
    return boxes.every(label => art.includes(label)) && edges.every(label => onEdge(art, label)) ? art : null
  } catch { return null }
}

// A top-level fenced block, closed, whose language is mermaid. Blocks inside
// lists or quotes stay as code.
function drawnBlock(token: Token, columns: number): string | null {
  if (token.type !== 'code' || token.codeBlockStyle === 'indented') return null
  const language = (token.lang ?? '').trim().split(/\s+/)[0].toLowerCase()
  if (language !== 'mermaid' && language !== 'mmd') return null
  const body = token.raw.replace(/\n+$/, '')
  const open = /^ {0,3}(`{3,}|~{3,})/.exec(body)
  const close = /\n {0,3}(`{3,}|~{3,})[ \t]*$/.exec(body)
  if (!open || !close || close[1][0] !== open[1][0] || close[1].length < open[1].length) return null
  const source = token.text ?? ''
  const art = diagrams(`${columns}\u0000${source}`, () => draw(source, columns))
  if (!art || art.includes('```') || art.includes('~~~')) return null
  return '```text\n' + art + '\n```' + token.raw.slice(body.length)
}

// --- Math -------------------------------------------------------------------

interface Change { start: number; end: number; text: string }

// LaTeX-looking: a command, a script on a single character or a group, or a
// relation. Plain words, shell names (`${HOME}`) and snake_case are not.
const LOOKS_LIKE_MATH = /\\[A-Za-z]+|[\^_](\{|[\p{L}\p{N}](?![\p{L}\p{N}]))|[=<>]/u
// A name like `MY_VAR_2` or `FILE_X`: a word of two letters or more joined by
// an underscore. Math variables are single letters (`x_1`, `a_b`).
const IDENTIFIER = /^[A-Za-z0-9_]+$/
const WORD_JOIN = /[A-Za-z]{2}_|_[A-Za-z]{2}/
const convert = (body: string) =>
  LOOKS_LIKE_MATH.test(body) && !(IDENTIFIER.test(body) && WORD_JOIN.test(body)) ? mathToUnicode(body) : null

// Inline `$x$` with tight delimiters: not money (`$5 or $10`) and not shell or
// template expansions (`${a}`, `$(cmd)`, `${{ x }}`), which put `{` or `(`
// right after a dollar. And `\(x\)` whose backslash is not itself escaped.
const INLINE = /(?<![\w$\\])\$(?![\s${(])([^$\n]{1,200}?)(?<![\s\\])\$(?![\w${(])|(?<!\\)\\\(([^\n]{1,200}?)\\\)/g
// Code-like HTML whose words are not plain text.
const HTML_CODE = /<(pre|code|kbd|samp|tt|var|script|style|textarea)\b[^>]*>[\s\S]*?<\/\1\s*>/gi

function mathChanges(text: string, keep: [number, number][]): Change[] {
  for (const match of text.matchAll(HTML_CODE)) keep.push([match.index, match.index + match[0].length])
  const free = (start: number, end: number) => keep.every(([a, b]) => end <= a || start >= b)
  const changes: Change[] = []
  // Display: `$$`/`\[` and `$$`/`\]` alone on their lines (any blockquote
  // prefix repeated exactly), or the whole line `$$ x $$`.
  const lines = text.split('\n')
  const starts: number[] = []
  for (let i = 0, at = 0; i < lines.length; at += lines[i].length + 1, i++) starts.push(at)
  for (let i = 0; i < lines.length; i++) {
    const [, prefix, head] = /^((?:[ \t]*>[ ]?)*[ \t]*)(.*)$/.exec(lines[i])!
    for (const [open, close] of [['$$', '$$'], ['\\[', '\\]']]) {
      let body: string | null = null, last = i
      const trimmed = head.trim()
      if (trimmed.length > open.length + close.length && trimmed.startsWith(open) && trimmed.endsWith(close)) {
        const inner = trimmed.slice(open.length, -close.length)
        if (!inner.includes(open) && !inner.includes(close)) body = inner
      } else if (trimmed === open) {
        const parts: string[] = []
        for (let j = i + 1; j < lines.length && j <= i + 20; j++) {
          if (!lines[j].startsWith(prefix.replace(/[ \t]+$/, '')) ) break
          const line = lines[j].slice(prefix.replace(/[ \t]+$/, '').length).trim()
          if (!line) break
          if (line === close) { body = parts.join(' '); last = j; break }
          if (line.includes(open) || line.includes(close)) break
          parts.push(line)
        }
      }
      if (body === null) continue
      const start = starts[i] + prefix.length, end = starts[last] + lines[last].length
      const math = convert(body)
      if (math && free(starts[i], end)) changes.push({ start, end, text: math })
      // A display formula is converted whole or not at all: never piece by piece.
      else keep.push([starts[i], end])
      i = last
      break
    }
  }
  for (const match of text.matchAll(INLINE)) {
    const start = match.index, end = start + match[0].length
    if (!free(start, end) || changes.some(change => start < change.end && end > change.start)) continue
    const math = convert(match[1] ?? match[2])
    if (math) changes.push({ start, end, text: math })
  }
  return changes.sort((a, b) => a.start - b.start)
}

function apply(text: string, changes: Change[]): string {
  let out = '', at = 0
  for (const change of changes) { out += text.slice(at, change.start) + change.text; at = change.end }
  return out + text.slice(at)
}

// The safe changes of one top-level block: all at once if they are, else one
// by one. Kept by the block's text, so a streamed answer checks each finished
// block once.
const checked = cache<Change[]>(512)
function safeInBlock(raw: string, changes: Change[]): Change[] {
  return checked(`${raw}\u0002${JSON.stringify(changes)}`, () => {
    if (changes.length > MAX_CANDIDATES) return []
    const before = shape(raw, false)
    if (safe(raw, changes, before)) return changes
    const kept: Change[] = []
    for (const change of changes) if (safe(raw, [...kept, change], before)) kept.push(change)
    return kept
  })
}

// Checks changes block by block, then the kept ones once more against the
// whole answer; if that last check fails, no math is converted at all.
function safeChanges(text: string, changes: Change[], blocks: [number, number][], before: string): Change[] {
  if (!changes.length) return []
  const kept: Change[] = []
  for (const [start, end] of blocks) {
    const inside = changes.filter(change => change.start >= start && change.end <= end)
    if (!inside.length) continue
    const local = inside.map(change => ({ ...change, start: change.start - start, end: change.end - start }))
    for (const change of safeInBlock(text.slice(start, end), local)) kept.push({ ...change, start: change.start + start, end: change.end + start })
  }
  return kept.length && safe(text, kept, before) ? kept : []
}

// --- The rewrite ------------------------------------------------------------

export function rewrite(text: string, columns: number): string {
  const tokens = lex(text) as Token[]
  if (tokens.reduce((sum, token) => sum + token.raw.length, 0) !== text.length) return text
  // Top-level code and HTML blocks are never searched for math; mermaid blocks
  // are swapped for their drawing at the end.
  const keep: [number, number][] = []
  const ranges: [number, number][] = []
  const drawings: Change[] = []
  let at = 0
  for (const token of tokens) {
    const end = at + token.raw.length
    ranges.push([at, end])
    if (token.type === 'code' || token.type === 'html') keep.push([at, end])
    const drawn = drawnBlock(token, columns)
    if (drawn) drawings.push({ start: at, end, text: drawn })
    at = end
  }
  const math = safeChanges(text, mathChanges(text, keep), ranges, skeleton(tokens, false).join('\u0001'))
  return apply(text, [...math, ...drawings].sort((a, b) => a.start - b.start))
}

export const register: Register = on => {
  on('ui.render', { component: 'AssistantMessage' }, ($, e, next) => {
    if (e.surface !== 'terminal' || e.props.isSummary) return next(e)
    const columns = e.viewport?.columns ?? DEFAULT_COLUMNS
    let text: string
    try { text = answers(`${columns}\u0000${e.props.text}`, () => rewrite(e.props.text, columns)) } catch { return next(e) }
    return text === e.props.text ? next(e) : next({ ...e, props: { ...e.props, text } })
  })
}

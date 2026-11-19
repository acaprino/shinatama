// The team map's model (spec 2026-11-17-thread-team-design.md, sections 1, 2, 10, 11): the formation of a
// thread as the window edits it, and the arrangement of its cards. Roles, checks and the sentences sent to
// the sessions are computed by the engine (`crates/engine/src/team.rs`) and only read here. The wire format
// is camelCase, the one of that module; every function returns a new formation and leaves its argument alone.
export type Share = 'split' | 'same' | 'free';
export type Kind = 'work' | 'observes';
export type Report = 'done' | 'steps' | 'strict';
export type ModelRef = { program: string; connection: string | null; model: string | null };
export type Agent = { id: string; session: string | null; model: ModelRef | null; name: string; notes: string; share: Share; x: number; y: number };
export type Link = { from: string; to: string; kind: Kind; report: Report; words: string };
export type Formation = { revision: number; agents: Agent[]; links: Link[] };
export type Issue = { level: 'error' | 'warning'; code: string; text: string; agents: string[]; links: number[] };
export type Evaluation = {
  roles: Record<string, { role: string; detail: string; mukade: string; handOut: string | null }>;
  issues: Issue[];
  sentences: [string, string][];
};
export type Point = { x: number; y: number };
export type Rect = { x: number; y: number; width: number; height: number };
/** A session of the thread, as `thread_chat::project` gives it (the same shape `thread-chat-model.ts` reads). */
export type TeamSession = {
  sessionId: string; harness: string | null; title: string | null; state: string;
  memberName: string | null; connection: 'in_room' | 'launched' | 'not_connected' | 'unsupported'; wakes: number | null;
};
/** What was last told, by agent id; the key `notice` is the last notice, not an agent. */
export type TeamTold = Record<string, { revision: number; member: string }>;
/** Why the announcer could not tell: by agent id, `service` (Mukade is stopped or unreachable) or `notice` (the message for the other members); kept until something changes. */
export type TeamRefused = Record<string, string>;
export type TeamModel = { program: string; connection: string | null; model: string | null; label: string };
/** The answer of `team_get`. */
export type TeamSnapshot = {
  enabled: boolean; formation: Formation | null; told: TeamTold; refused: TeamRefused; sessions: TeamSession[]; models: TeamModel[]; evaluation: Evaluation | null;
};

export const CARD = { width: 190, height: 78 } as const;
export const MAX_AGENTS = 24;
export const MAX_LINKS = 64;
export const MAX_NAME = 40;
/** Narrower than this and the map is laid out as if it were this wide (the mockup's floor). */
export const MIN_WIDTH = 560;

const SHARES: readonly string[] = ['split', 'same', 'free'];
const KINDS: readonly string[] = ['work', 'observes'];
const REPORTS: readonly string[] = ['done', 'steps', 'strict'];
const ROLES: readonly string[] = ['lead', 'worker', 'leadWorker', 'observer', 'peer'];
const MUKADE_ROLES: readonly string[] = ['peer', 'lead', 'worker', 'observer'];
const CONNECTIONS: readonly string[] = ['in_room', 'launched', 'unsupported'];

// ---------------------------------------------------------------------------------------------- reading

const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const str = (value: unknown, cap = 65536): value is string => typeof value === 'string' && value.length <= cap;
const nullable = (value: unknown, cap = 65536): string | null | undefined => value === null || value === undefined ? null : str(value, cap) ? value : undefined;
const whole = (value: unknown): value is number => typeof value === 'number' && Number.isSafeInteger(value);

function parseModel(value: unknown): ModelRef | null | undefined {
  if (value === null || value === undefined) return null;
  if (!record(value) || !str(value.program, 64)) return undefined;
  const connection = nullable(value.connection, 128), model = nullable(value.model, 256);
  return connection === undefined || model === undefined ? undefined : { program: value.program, connection, model };
}

/** A formation as the engine stores it; null when it is not one, or its arrows join agents that are not on the map. */
export function parseFormation(value: unknown): Formation | null {
  if (!record(value) || !whole(value.revision) || value.revision < 0) return null;
  if (!Array.isArray(value.agents) || !Array.isArray(value.links) || value.agents.length > MAX_AGENTS || value.links.length > MAX_LINKS) return null;
  const agents: Agent[] = [], known = new Set<string>();
  for (const item of value.agents) {
    if (!record(item) || !str(item.id, 64) || !item.id || known.has(item.id) || !str(item.name, 256) || !str(item.notes ?? '', 65536)) return null;
    const session = nullable(item.session, 64), model = parseModel(item.model);
    if (session === undefined || model === undefined || (item.share !== undefined && !SHARES.includes(item.share as string))) return null;
    if (!(typeof item.x === 'number' && Number.isFinite(item.x) && typeof item.y === 'number' && Number.isFinite(item.y))) return null;
    known.add(item.id);
    agents.push({ id: item.id, session, model, name: item.name, notes: (item.notes as string | undefined) ?? '', share: (item.share as Share | undefined) ?? 'split',
      x: Math.round(item.x), y: Math.round(item.y) });
  }
  const links: Link[] = [];
  for (const item of value.links) {
    if (!record(item) || !str(item.from, 64) || !str(item.to, 64) || item.from === item.to || !known.has(item.from) || !known.has(item.to)) return null;
    if (!KINDS.includes(item.kind as string) || (item.report !== undefined && !REPORTS.includes(item.report as string)) || !str(item.words ?? '', 4096)) return null;
    links.push({ from: item.from, to: item.to, kind: item.kind as Kind, report: (item.report as Report | undefined) ?? 'done', words: (item.words as string | undefined) ?? '' });
  }
  return { revision: value.revision, agents, links };
}

export function parseEvaluation(value: unknown): Evaluation | null {
  if (!record(value) || !record(value.roles) || !Array.isArray(value.issues) || !Array.isArray(value.sentences)) return null;
  const roles: Evaluation['roles'] = {};
  for (const [agent, view] of Object.entries(value.roles)) {
    if (!record(view) || !ROLES.includes(view.role as string) || !str(view.detail, 1024) || !MUKADE_ROLES.includes(view.mukade as string)) return null;
    const handOut = nullable(view.handOut, 1024);
    if (handOut === undefined) return null;
    roles[agent] = { role: view.role as string, detail: view.detail, mukade: view.mukade as string, handOut };
  }
  const issues: Issue[] = [];
  for (const item of value.issues) {
    if (!record(item) || (item.level !== 'error' && item.level !== 'warning') || !str(item.code, 64) || !str(item.text, 4096)) return null;
    if (!Array.isArray(item.agents) || !item.agents.every((a) => str(a, 64)) || !Array.isArray(item.links) || !item.links.every((k) => whole(k) && k >= 0)) return null;
    issues.push({ level: item.level, code: item.code, text: item.text, agents: [...item.agents] as string[], links: [...item.links] as number[] });
  }
  const sentences: [string, string][] = [];
  for (const item of value.sentences) {
    if (!Array.isArray(item) || item.length !== 2 || !str(item[0], 8192) || !str(item[1], 8192)) return null;
    sentences.push([item[0], item[1]]);
  }
  return { roles, issues, sentences };
}

export function parseTeam(value: unknown): TeamSnapshot | null {
  if (!record(value) || typeof value.enabled !== 'boolean') return null;
  if (!value.enabled) return { enabled: false, formation: null, told: {}, refused: {}, sessions: [], models: [], evaluation: null };
  if (!Array.isArray(value.sessions)) return null;
  let formation: Formation | null = null, evaluation: Evaluation | null = null;
  if (value.formation !== null && value.formation !== undefined && !(formation = parseFormation(value.formation))) return null;
  if (value.evaluation !== null && value.evaluation !== undefined && !(evaluation = parseEvaluation(value.evaluation))) return null;
  const told: TeamTold = {};
  if (record(value.told)) {
    for (const [key, item] of Object.entries(value.told)) {
      if (record(item) && whole(item.revision)) told[key] = { revision: item.revision, member: str(item.member, 128) ? item.member : '' };
    }
  }
  const sessions: TeamSession[] = [];
  for (const item of value.sessions.slice(0, 256)) {
    if (!record(item) || !str(item.sessionId, 14) || !str(item.state, 16)) continue;
    sessions.push({ sessionId: item.sessionId, harness: nullable(item.harness, 64) ?? null, title: nullable(item.title, 256) ?? null, state: item.state,
      memberName: nullable(item.memberName, 32) ?? null, connection: CONNECTIONS.includes(item.connection as string) ? item.connection as TeamSession['connection'] : 'not_connected',
      wakes: whole(item.wakes) ? item.wakes : null });
  }
  const models: TeamModel[] = [];
  if (Array.isArray(value.models)) {
    for (const item of value.models.slice(0, 256)) {
      if (!record(item) || !str(item.program, 64) || !str(item.label, 256)) continue;
      const connection = nullable(item.connection, 128), model = nullable(item.model, 256);
      if (connection !== undefined && model !== undefined) models.push({ program: item.program, connection, model, label: item.label });
    }
  }
  const refused: TeamRefused = {};
  if (record(value.refused)) {
    for (const [key, reason] of Object.entries(value.refused).slice(0, 256)) {
      if (key && key.length <= 128 && typeof reason === 'string' && reason) refused[key] = reason.slice(0, 400);
    }
  }
  return { enabled: true, formation, told, refused, sessions, models, evaluation };
}

// ---------------------------------------------------------------------------------------------- editing

/** `a_` and 12 of `[a-z2-7]`, the form `validate` in the engine accepts. */
export function newAgentId(): string {
  const letters = 'abcdefghijklmnopqrstuvwxyz234567', bytes = new Uint8Array(12);
  crypto.getRandomValues(bytes);
  return `a_${Array.from(bytes, (b) => letters[b % 32]).join('')}`;
}

/**
 * What the engine refuses in a name, a note or the words of an arrow besides `@`: control characters, line and paragraph
 * separators, and format characters (soft hyphen, zero-width and bidirectional marks, tag characters). Kept in step with
 * `is_unsafe_char` in `crates/engine/src/team.rs`.
 */
const HIDDEN_RANGES: ReadonlyArray<readonly [number, number]> = [
  [0x00ad, 0x00ad], [0x061c, 0x061c], [0x180e, 0x180e], [0x200b, 0x200f], [0x2028, 0x202e], [0x2060, 0x206f], [0xfeff, 0xfeff], [0xfff9, 0xfffb], [0xe0000, 0xe007f],
];
const isHidden = (mark: string) => /\p{Cc}/u.test(mark) || HIDDEN_RANGES.some(([from, to]) => { const code = mark.codePointAt(0) ?? -1; return code >= from && code <= to; });
const withoutHidden = (text: string, instead: (mark: string) => string) => Array.from(text, (mark) => (isHidden(mark) ? instead(mark) : mark)).join('');
/** Typed text as the engine accepts it: without `@` and the hidden characters (a tab and a newline stay, for the notes). */
export const plainText = (text: string) => withoutHidden(text.replace(/@/g, ''), (mark) => (mark === '\t' || mark === '\n' ? mark : ''));

/** What the engine accepts as a name: one line, no `@`, 1 to 40 characters. */
function nameFrom(text: string): string {
  const clean = Array.from(withoutHidden(text.replace(/@/g, ' '), () => ' ').replace(/\s+/g, ' ').trim()).slice(0, MAX_NAME).join('').trim();
  return clean || 'Agent';
}

/** The session joins the map as a new agent at `at`; a session already on the map, or a full map, changes nothing. */
export function addSession(f: Formation, session: string, name: string, at: Point): Formation {
  if (f.agents.length >= MAX_AGENTS || f.agents.some((a) => a.session === session)) return f;
  const agent: Agent = { id: newAgentId(), session, model: null, name: nameFrom(name), notes: '', share: 'split', x: Math.round(at.x), y: Math.round(at.y) };
  return { ...f, agents: [...f.agents, agent] };
}

export function removeAgent(f: Formation, agent: string): Formation {
  return { ...f, agents: f.agents.filter((a) => a.id !== agent), links: f.links.filter((l) => l.from !== agent && l.to !== agent) };
}

const onMap = (f: Formation, id: string) => f.agents.some((a) => a.id === id);
const between = (l: Link, a: string, b: string) => (l.from === a && l.to === b) || (l.from === b && l.to === a);

/** An arrow from `from` to `to` that gives work, reported when done; it replaces whatever joined the two before. */
export function connect(f: Formation, from: string, to: string): Formation {
  if (from === to || !onMap(f, from) || !onMap(f, to)) return f;
  const rest = f.links.filter((l) => !between(l, from, to));
  if (rest.length >= MAX_LINKS) return f;
  return { ...f, links: [...rest, { from, to, kind: 'work', report: 'done', words: '' }] };
}

const agentKey = (a: Agent) => JSON.stringify([a.id, a.session, a.model && [a.model.program, a.model.connection, a.model.model], a.name, a.notes, a.share]);
const linkKey = (l: Link) => JSON.stringify([l.from, l.to, l.kind, l.kind === 'work' ? l.report : null, l.words]);
const keysOf = (f: Formation) => ({ agents: f.agents.map(agentKey).sort(), links: f.links.map(linkKey).sort() });

/** Whether the draft differs from what was applied in anything the agents are told. Places and the revision do not count. */
export function changed(applied: Formation | null, draft: Formation): boolean {
  const mine = keysOf(draft), theirs = keysOf(applied ?? { revision: 0, agents: [], links: [] });
  return JSON.stringify(mine) !== JSON.stringify(theirs);
}

/** The draft's arrows (by index) that the applied formation does not have as they are: the ones drawn dashed. */
export function pendingLinks(applied: Formation | null, draft: Formation): Set<number> {
  const known = new Set((applied?.links ?? []).map(linkKey));
  return new Set(draft.links.flatMap((l, k) => known.has(linkKey(l)) ? [] : [k]));
}

/** Where the line from the centre of `card` towards `towards` leaves the card, `gap` beyond its edge; never past half way. */
export function borderPoint(card: Rect, towards: Point, gap: number): Point {
  const cx = card.x + card.width / 2, cy = card.y + card.height / 2, dx = towards.x - cx, dy = towards.y - cy;
  if (!dx && !dy) return { x: cx, y: cy };
  const s = Math.min(dx ? (card.width / 2 + gap) / Math.abs(dx) : Infinity, dy ? (card.height / 2 + gap) / Math.abs(dy) : Infinity, 0.5);
  return { x: cx + dx * s, y: cy + dy * s };
}

// -------------------------------------------------------------------------------------------- arrangement

const NW = CARD.width, NH = CARD.height;

const observesAny = (f: Formation, id: string) => f.links.some((l) => l.kind === 'observes' && l.from === id);

/** Tidy up (spec section 11): givers above takers, each layer ordered to cross few arrows, a wide layer wrapping, observers at
 *  the right edge level with what they observe, agents with no arrow in a row at the bottom. A loop does not hang it. */
export function tidy(f: Formation, roles: Evaluation['roles'], width: number): Record<string, Point> {
  const total = Math.max(width, MIN_WIDTH);
  const side = f.agents.filter((a) => (roles[a.id] ? roles[a.id].role === 'observer' : observesAny(f, a.id))).map((a) => a.id);
  const sideSet = new Set(side), all = f.agents.map((a) => a.id);
  const work = f.links.filter((l) => l.kind === 'work' && !sideSet.has(l.from) && !sideSet.has(l.to));
  const tied = new Set(work.flatMap((l) => [l.from, l.to]));
  const main = all.filter((id) => tied.has(id)), alone = all.filter((id) => !tied.has(id) && !sideSet.has(id));
  const rank: Record<string, number> = {};
  const depthOf = (id: string, trail = new Set<string>()): number => {
    if (rank[id] !== undefined) return rank[id];
    if (trail.has(id)) return 0; // a loop: stop here, the checks report it
    trail.add(id);
    const above = work.filter((l) => l.to === id).map((l) => depthOf(l.from, trail));
    trail.delete(id);
    return (rank[id] = above.length ? 1 + Math.max(...above) : 0);
  };
  const layers: string[][] = [];
  for (const id of main) (layers[depthOf(id)] ??= []).push(id);
  const rows = layers.filter((layer) => layer);
  const room = total - (side.length ? NW + 56 : 0) - 16, cx: Record<string, number> = {}, out: Record<string, Point> = {};
  const spread = (layer: string[]) => layer.forEach((id, i) => { cx[id] = (i + 0.5) * room / layer.length; });
  rows.forEach(spread);
  const pull = (id: string, mine: 'from' | 'to', other: 'from' | 'to') => {
    const xs = work.filter((l) => l[mine] === id && cx[l[other]] !== undefined).map((l) => cx[l[other]]);
    return xs.length ? xs.reduce((a, b) => a + b, 0) / xs.length : cx[id];
  };
  for (let sweep = 0; sweep < 6; sweep += 1) {
    const down = sweep % 2 === 0, list = down ? rows.slice(1) : rows.slice(0, -1).reverse();
    for (const layer of list) {
      const key = Object.fromEntries(layer.map((id) => [id, down ? pull(id, 'to', 'from') : pull(id, 'from', 'to')]));
      layer.sort((a, b) => key[a] - key[b]);
      spread(layer);
    }
  }
  const per = Math.max(1, Math.floor(room / (NW + 30)));
  let y = 26;
  const row = (list: string[], gap: number) => {
    if (!list.length) return;
    list.forEach((id, i) => {
      const line = Math.floor(i / per), n = Math.min(per, list.length - line * per);
      out[id] = { x: Math.max(8, Math.round(8 + ((i % per) + 0.5) * room / n - NW / 2)), y: y + line * (NH + 24) };
    });
    y += Math.ceil(list.length / per) * (NH + 24) + gap;
  };
  rows.forEach((layer) => row(layer, 66));
  row(alone, 40);
  const level = (id: string) => out[f.links.find((l) => l.from === id)?.to ?? '']?.y ?? 26;
  let last = -Infinity;
  for (const id of [...side].sort((a, b) => level(a) - level(b))) {
    last = Math.max(level(id), last + NH + 18);
    out[id] = { x: Math.max(8, total - NW - 22), y: last };
  }
  return out;
}

/** Arrange as Star, Chain or Peers: draws the work arrows and places the cards (the mockup's `shape` and `arrange`).
 *  `observes` arrows are always kept; a work arrow that stays keeps its report and words. */
export function arrange(f: Formation, shape: 'star' | 'chain' | 'peers', hub: string | null, width: number): Formation {
  const total = Math.max(width, MIN_WIDTH);
  const side = f.agents.filter((a) => observesAny(f, a.id)).map((a) => a.id);
  const ids = f.agents.map((a) => a.id).filter((id) => !side.includes(id));
  if (!ids.length) return f;
  const first = hub && ids.includes(hub) ? hub : ids.find((id) => f.links.some((l) => l.kind === 'work' && l.from === id)) ?? ids[0];
  const rest = ids.filter((id) => id !== first), line = [first, ...rest];
  const observes = f.links.filter((l) => l.kind === 'observes');
  const old = (from: string, to: string): Link => f.links.find((l) => l.kind === 'work' && l.from === from && l.to === to) ?? { from, to, kind: 'work', report: 'done', words: '' };
  const pairs: [string, string][] = shape === 'star' ? rest.map((id) => [first, id]) : shape === 'chain' ? line.slice(1).map((id, i) => [line[i], id]) : [];
  const links = [...observes, ...pairs.map(([from, to]) => old(from, to))];

  const out: Record<string, Point> = {};
  const put = (id: string, cx: number, cy: number) => {
    out[id] = { x: Math.round(Math.max(8, Math.min(cx - NW / 2, total - NW - 22))), y: Math.round(Math.max(8, cy - NH / 2)) };
  };
  const roomy = total - (side.length ? 220 : 0), centre = roomy / 2;
  if (shape === 'star' && ids.length > 2) {
    const rx = Math.min(centre - NW / 2 - 14, 300), ry = 170, cy = 60 + ry + NH / 2;
    put(first, centre, cy);
    // Few spokes sit at corners, so the arrows have room; more go round an ellipse.
    const corners: Record<number, [number, number][]> = { 2: [[-1, 0], [1, 0]], 3: [[-1, -1], [1, -1], [0, 1.1]], 4: [[-1, -1], [1, -1], [-1, 1], [1, 1]] };
    rest.forEach((id, i) => {
      const t = -Math.PI / 2 + Math.PI / rest.length + i * 2 * Math.PI / rest.length;
      const [ux, uy] = corners[rest.length]?.[i] ?? [Math.cos(t), Math.sin(t)];
      put(id, centre + rx * ux, cy + ry * uy);
    });
  } else if (shape === 'chain' && ids.length * (NW + 50) <= roomy) {
    line.forEach((id, i) => put(id, (i + 0.5) * roomy / line.length, 190));
  } else {
    const per = Math.max(1, Math.floor(roomy / (NW + 40))), list = shape === 'chain' ? line : ids;
    list.forEach((id, i) => {
      const row = Math.floor(i / per), n = Math.min(per, list.length - row * per);
      put(id, ((i % per) + 0.5) * roomy / n, 110 + row * 160);
    });
  }
  side.forEach((id, i) => put(id, total - 22 - NW / 2, 80 + i * (NH + 20)));
  return { ...f, agents: f.agents.map((a) => ({ ...a, ...out[a.id] })), links };
}

// ----------------------------------------------------------------------------------------------------------- templates
/** A saved team (spec section 10): agents without a session, their arrows and their places. It is the engine's `Template`. */
export type Template = { id: string; name: string; agents: Agent[]; links: Link[] };

/** The answer of `team_templates`, one by one: what is not a template is left out; null when the answer is not a list. */
export function parseTemplates(value: unknown): Template[] | null {
  if (!Array.isArray(value)) return null;
  const found: Template[] = [];
  for (const item of value.slice(0, 64)) {
    if (!record(item) || !str(item.id, 64) || !item.id || !str(item.name, 256)) continue;
    const parsed = parseFormation({ revision: 0, agents: item.agents, links: item.links });
    if (parsed) found.push({ id: item.id, name: item.name, agents: parsed.agents.map((a) => ({ ...a, session: null })), links: parsed.links });
  }
  return found;
}

/** A new agent with no session, at `at`; a full map changes nothing. A name already on the map gets a number. */
export function addAgent(f: Formation, name: string, at: Point): Formation {
  if (f.agents.length >= MAX_AGENTS) return f;
  const base = nameFrom(name), taken = new Set(f.agents.map((a) => a.name));
  let chosen = base;
  for (let n = 2; taken.has(chosen) && n < 100; n += 1) chosen = `${Array.from(base).slice(0, MAX_NAME - String(n).length - 1).join('')} ${n}`;
  const agent: Agent = { id: newAgentId(), session: null, model: null, name: chosen, notes: '', share: 'split', x: Math.round(at.x), y: Math.round(at.y) };
  return { ...f, agents: [...f.agents, agent] };
}

/** The first agent that has no session: Apply opens one for it. */
export function emptyAgent(f: Formation): Agent | null { return f.agents.find((a) => !a.session) ?? null; }

/** The session takes the place of an agent that has none: its name, notes, arrows and card stay. One session fills one agent. */
export function fillAgent(f: Formation, agent: string, session: string): Formation {
  const target = f.agents.find((a) => a.id === agent);
  if (!target || target.session || f.agents.some((a) => a.session === session)) return f;
  return { ...f, agents: f.agents.map((a) => a.id === agent ? { ...a, session, model: null } : a) };
}

const overlap = (agents: readonly Agent[]) => agents.some((a, i) => agents.slice(i + 1).some((b) => Math.abs(a.x - b.x) < CARD.width && Math.abs(a.y - b.y) < CARD.height));

/** Whether the model menu holds this choice (the Models panel may have changed since the template was saved). */
export const holdsModel = (models: readonly TeamModel[], m: ModelRef) => models.some((x) => x.program === m.program && x.connection === m.connection && x.model === m.model);

/** The team a template makes: new ids, no session, revision 0. An agent keeps its model only if `models` (the menu) still holds
 *  it, otherwise it has none. Its places keep their shape and are scaled down to fit `width` (a layout that would then overlap
 *  is tidied instead). */
export function fromTemplate(t: Template, width: number, id: () => string, models: readonly TeamModel[]): Formation {
  if (!t.agents.length) return { revision: 0, agents: [], links: [] };
  const ids = new Map(t.agents.map((a) => [a.id, id()] as const));
  const links = t.links.filter((l) => ids.has(l.from) && ids.has(l.to)).map((l) => ({ ...l, from: ids.get(l.from)!, to: ids.get(l.to)! }));
  const total = Math.max(width, MIN_WIDTH), room = total - NW - 22 - 8;
  const xs = t.agents.map((a) => a.x), minX = Math.min(...xs), minY = Math.min(...t.agents.map((a) => a.y)), span = Math.max(...xs) - minX;
  const scale = span > room ? room / span : 1, centre = scale === 1 ? Math.max(0, (room - span) / 2) : 0;
  const agents = t.agents.map((a): Agent => ({ ...a, id: ids.get(a.id)!, session: null, model: a.model && holdsModel(models, a.model) ? a.model : null,
    x: Math.round(8 + centre + (a.x - minX) * scale), y: Math.round(a.y - minY + 26) }));
  const formation: Formation = { revision: 0, agents, links };
  if (!overlap(agents)) return formation;
  const places = tidy(formation, {}, width);
  return { ...formation, agents: agents.map((a) => ({ ...a, x: Math.round(places[a.id]?.x ?? a.x), y: Math.round(places[a.id]?.y ?? a.y) })) };
}

/** The drawing of a template in a 64 × 40 box: the centres of its cards and the lines between them. */
export function miniature(t: Template): { cards: Point[]; lines: { a: Point; b: Point; kind: Kind }[] } {
  if (!t.agents.length) return { cards: [], lines: [] };
  const BOX = { width: 64, height: 40, pad: 4 };
  const minX = Math.min(...t.agents.map((a) => a.x)), minY = Math.min(...t.agents.map((a) => a.y));
  const width = Math.max(...t.agents.map((a) => a.x)) + CARD.width - minX, height = Math.max(...t.agents.map((a) => a.y)) + CARD.height - minY;
  const scale = Math.min((BOX.width - 2 * BOX.pad) / width, (BOX.height - 2 * BOX.pad) / height);
  const dx = (BOX.width - width * scale) / 2, dy = (BOX.height - height * scale) / 2, tenth = (n: number) => Math.round(n * 10) / 10;
  const centres = new Map(t.agents.map((a) => [a.id, { x: tenth(dx + (a.x - minX + CARD.width / 2) * scale), y: tenth(dy + (a.y - minY + CARD.height / 2) * scale) }] as const));
  return {
    cards: t.agents.map((a) => centres.get(a.id)!),
    lines: t.links.flatMap((l) => { const a = centres.get(l.from), b = centres.get(l.to); return a && b ? [{ a, b, kind: l.kind }] : []; }),
  };
}

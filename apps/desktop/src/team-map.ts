// The team map (spec 2026-11-17-thread-team-design.md, sections 1, 2, 4, 5, 11): the agents of a thread as cards the user places, their
// relations as arrows the user draws, and Apply, which has Shinatama tell each session its place. Roles, checks and the sentences
// sent to the sessions are the engine's (`team_evaluate`); this view asks and draws, and computes none of them (ADR-0057: it never
// reads, waits for or acknowledges a message, and holds no member handle or credential).
// Phase 1 only: the agents are the thread's sessions. Seams for the later phases are marked "phase 2" (templates, agents without a
// session) and "phase 3" (a model per agent); nothing of them is built here.
import './team-map.css';
import { confirmsField } from './desktop-shell-model.ts';
import { createEvaluator } from './team-map-evaluate.ts';
import {
  CARD, MAX_AGENTS, MAX_LINKS, MAX_NAME, addSession, arrange, borderPoint, changed, connect, parseEvaluation, parseFormation, parseTeam, pendingLinks,
  removeAgent, tidy, type Agent, type Evaluation, type Formation, type Link, type Point, type Report, type Share, type TeamSession, type TeamSnapshot,
} from './team-map-model.ts';

export type TeamThread = { id: string; projectId?: string };
export type TeamMapDeps = {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  /** The thread's name as the sidebar shows it. */
  title(threadId: string): string;
  harnessLabel(harness: string): string;
  /** Shows the session's tab; starts nothing. */
  focusSession(sessionId: string): void;
};

const POLL_MS = 10_000;
const EVALUATE_GAP_MS = 150;
const MAX_NOTES_BYTES = 1024;
const MAX_WORDS = 120;
const KEEP_TIDY_KEY = 'shinatama.team.keepTidy';
const VIEW_PREFIX = 'team-';
const NS = 'http://www.w3.org/2000/svg';

const ROLE_NAME: Record<string, string> = { lead: 'Lead', worker: 'Worker', leadWorker: 'Lead + Worker', observer: 'Observer', peer: 'Peer' };
const SHARE_NAME: Record<Share, string> = { split: 'Splits it', same: 'Same to all', free: 'First free' };
const SHARE_SHORT: Record<Share, string> = { split: 'splits the work', same: 'same work to all', free: 'to whoever is free' };
const SHARE_TIP: Record<Share, string> = {
  split: 'A different part to each, then it puts the results together.', same: 'The same task to each, then it compares the results.', free: 'Each task to whichever of them is free.',
};
const KIND_NAME = { work: 'gives work to', observes: 'observes' } as const;
const KIND_TIP = {
  work: (a: string, b: string) => `${b} waits for work from ${a}, does it and reports.`,
  observes: (a: string, b: string) => `${a} follows what ${b} does, to explain it to you. Never writes to it.`,
};
const REPORTS: readonly [Report, string, string][] = [
  ['done', 'When done', 'It works on its own and reports the result.'],
  ['steps', 'At every step', 'Constant feedback: it says what it is doing as it goes.'],
  ['strict', 'Only done / blocked', 'It only executes: no questions, no messages, just done or blocked.'],
];
const SHAPES: readonly { id: 'star' | 'chain' | 'peers'; name: string; tip: string; icon: string }[] = [
  { id: 'star', name: 'Star', tip: 'One agent in the centre gives work to all the others. Open a card first to choose the centre.',
    icon: '<path d="M13 10L4 3M13 10L22 3M13 10L4 17M13 10L22 17" fill="none"/><circle cx="13" cy="10" r="3.2"/><circle cx="4" cy="3" r="2"/><circle cx="22" cy="3" r="2"/><circle cx="4" cy="17" r="2"/><circle cx="22" cy="17" r="2"/>' },
  { id: 'chain', name: 'Chain', tip: 'A pipeline: each agent gives work to the next.',
    icon: '<path d="M6 10H10M16 10H20" fill="none"/><circle cx="3.5" cy="10" r="2.6"/><circle cx="13" cy="10" r="2.6"/><circle cx="22.5" cy="10" r="2.6"/>' },
  { id: 'peers', name: 'Peers', tip: 'No one is in charge: the work arrows go, everyone talks to everyone.',
    icon: '<circle cx="5" cy="10" r="2.6"/><circle cx="13" cy="10" r="2.6"/><circle cx="21" cy="10" r="2.6"/>' },
];

const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string) => {
  const node = document.createElement(tag); if (className) node.className = className; if (text !== undefined) node.textContent = text; return node;
};
const svg = <K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string> = {}) => {
  const node = document.createElementNS(NS, tag); for (const key of Object.keys(attrs)) node.setAttribute(key, attrs[key]); return node;
};
const quietButton = (label: string, title?: string) => { const b = el('button', 'quiet', label); b.type = 'button'; if (title) b.title = title; return b; };
const message = (error: unknown) => typeof error === 'string' ? error : (error as { message?: string } | null)?.message ?? 'Something went wrong.';
const clone = <T,>(value: T): T => structuredClone(value);
const bytes = (text: string) => new TextEncoder().encode(text).length;
const emptyFormation = (): Formation => ({ revision: 0, agents: [], links: [] });
/** What the engine refuses in names, words and notes: `@` (it would wake a member) and control characters. */
const plain = (text: string) => text.replace(/@/g, '').replace(/[\u0000-\u0008\u000b-\u001f\u007f]/g, '');
const rectOf = (a: Agent) => ({ x: a.x, y: a.y, width: CARD.width, height: CARD.height });
const centreOf = (a: Agent): Point => ({ x: a.x + CARD.width / 2, y: a.y + CARD.height / 2 });
const readKeepTidy = () => { try { return localStorage.getItem(KEEP_TIDY_KEY) === '1'; } catch { return false; } };
const writeKeepTidy = (on: boolean) => { try { localStorage.setItem(KEEP_TIDY_KEY, on ? '1' : '0'); } catch { /* a convenience only */ } };
/** What the arrows and agents are, without names, notes, words or places: when it changes, the engine's last answer no longer fits. */
const structureOf = (f: Formation) => JSON.stringify([f.agents.map(a => a.id), f.links.map(l => [l.from, l.to, l.kind, l.report])]);

type Selection = { agent: string } | { link: { from: string; to: string } } | null;
type Grab = {
  kind: 'move' | 'link' | 'session'; id: string; pointer: number; target: HTMLElement; startX: number; startY: number; dx: number; dy: number;
  moved: boolean; ghost?: HTMLElement; rubber?: SVGPathElement;
};
type Box = { key: string; element: HTMLElement; update(): void };

export function threadOfView(viewId: string): string | null { return viewId.startsWith(VIEW_PREFIX) ? viewId.slice(VIEW_PREFIX.length) : null; }
export const teamViewId = (threadId: string) => `${VIEW_PREFIX}${threadId}`;

function build(thread: TeamThread, deps: TeamMapDeps, onClose: () => void) {
  const viewId = teamViewId(thread.id);
  // ---------------------------------------------------------------------------------------------------------------- state
  let snap: TeamSnapshot | null = null;
  let applied: Formation | null = null;
  let draft: Formation = emptyFormation();
  let evaluation: Evaluation | null = null;
  let evaluatedFor = '';
  let evaluating = false;
  let selection: Selection = null;
  let trials: Record<string, string> = {};
  let trialsFor = '';
  let status: { text: string; tone: 'info' | 'error' } = { text: 'Reading the team…', tone: 'info' };
  let keepTidy = readKeepTidy();
  let visible = false; let reading = false; let applying = false; let disposed = false;
  let grab: Grab | null = null;
  let box: Box | null = null;
  let timer = 0;

  const evaluator = createEvaluator<Formation, Evaluation>(async formation => {
    const value = parseEvaluation(await deps.invoke<unknown>('team_evaluate', { request: { worksetId: thread.id, formation } }));
    if (!value) throw new Error('the answer was not understood');
    return value;
  }, EVALUATE_GAP_MS);
  const ask = <T,>(command: string, extra: Record<string, unknown> = {}) => deps.invoke<T>(command, { request: { worksetId: thread.id, ...extra } });

  // -------------------------------------------------------------------------------------------------------------- skeleton
  const root = el('div', 'team-map'); root.dataset.viewId = viewId; root.tabIndex = -1;
  const bar = el('div', 'team-bar'); bar.setAttribute('role', 'toolbar'); bar.setAttribute('aria-label', 'Team map');
  const shapesLabel = el('span', 'team-bar-label', 'Arrange as');
  const shapes = el('div', 'team-shapes'); shapes.setAttribute('role', 'group'); shapes.setAttribute('aria-label', 'Arrange the team as');
  const shapeButtons = SHAPES.map(shape => {
    const b = el('button'); b.type = 'button'; b.dataset.shape = shape.id; b.title = shape.tip;
    const icon = svg('svg', { viewBox: '0 0 26 20', 'aria-hidden': 'true' }); icon.innerHTML = shape.icon; b.append(icon, shape.name);
    b.onclick = () => arrangeAs(shape.id); shapes.append(b); return b;
  });
  const tidyButton = el('button', 'team-tidy', '⤢ Tidy up'); tidyButton.type = 'button';
  tidyButton.title = 'Place the cards by their arrows: who gives work above, who takes it below, observers on the right';
  const autoLabel = el('label', 'team-auto'); autoLabel.title = 'Rearrange by itself whenever an arrow or a card changes';
  const autoBox = el('input'); autoBox.type = 'checkbox'; autoBox.checked = keepTidy; autoLabel.append(autoBox, ' keep tidy');
  const state = el('span', 'team-state'); state.setAttribute('role', 'status');
  const discard = el('button', 'quiet', 'Discard'); discard.type = 'button'; discard.title = 'Throw away what is not applied and read the team again';
  const apply = el('button', 'primary', 'Apply'); apply.type = 'button';
  bar.append(shapesLabel, shapes, tidyButton, autoLabel, state, discard, apply);
  const problems = el('ul', 'team-problems'); problems.setAttribute('aria-label', 'Problems with this team'); problems.hidden = true;
  const body = el('div', 'team-body');
  const side = el('aside', 'team-side'); side.setAttribute('aria-label', 'Sessions of the thread');
  const sideList = el('ul', 'team-sessions'); const sideNote = el('p');
  side.append(el('h2', undefined, 'Sessions'), sideList, sideNote);
  const stage = el('div', 'team-stage');
  const scroll = el('div', 'team-scroll'); const canvas = el('div', 'team-canvas');
  const wires = svg('svg', { class: 'team-wires' }); 
  const defs = svg('defs');
  for (const kind of ['work', 'observes', 'bad', 'rubber']) {
    const marker = svg('marker', { id: `${viewId}-head-${kind}`, viewBox: '0 0 10 10', refX: '9', refY: '5', markerWidth: '11', markerHeight: '11', markerUnits: 'userSpaceOnUse', orient: 'auto' });
    const arrow = svg('path', { d: 'M0 0L10 5L0 10z', class: 'team-head' }); arrow.dataset.kind = kind === 'observes' ? 'observes' : 'work';
    if (kind === 'bad') arrow.setAttribute('data-bad', ''); if (kind === 'rubber') arrow.setAttribute('data-rubber', '');
    marker.append(arrow); defs.append(marker);
  }
  const wireLayer = svg('g'); wires.append(defs, wireLayer);
  const nodes = el('div', 'team-nodes');
  const hint = el('p', 'team-hint'); hint.innerHTML = '<b>Drag the sessions from the list on the left onto this map</b> (or press one to add it).<br>Then pull the → of a card onto another to connect them, and click the arrow to say what kind of relation it is.';
  canvas.append(wires, nodes, hint); scroll.append(canvas);
  const tray = el('div', 'team-tray'); tray.dataset.zone = 'tray';
  const statusLine = el('p', 'team-status'); statusLine.hidden = true;
  stage.append(scroll, tray, statusLine); body.append(side, stage);
  const off = el('p', 'team-off'); off.hidden = true;
  root.append(bar, problems, body, off);

  // ------------------------------------------------------------------------------------------------------------- lookups
  const agentOf = (id: string) => draft.agents.find(a => a.id === id);
  const sessionOf = (a: Agent): TeamSession | undefined => a.session ? snap?.sessions.find(s => s.sessionId === a.session) : undefined;
  const programOf = (a: Agent) => { const h = sessionOf(a)?.harness; return h ? deps.harnessLabel(h) : 'Agent'; };
  const current = () => evaluation ?? snap?.evaluation ?? null;
  const fresh = () => current() !== null && evaluatedFor === structureOf(draft);
  const pending = () => changed(applied, draft);
  const errors = () => fresh() ? current()!.issues.filter(i => i.level === 'error') : [];
  const linkIndex = (from: string, to: string) => draft.links.findIndex(l => l.from === from && l.to === to);
  const selectedLink = () => selection && 'link' in selection ? linkIndex(selection.link.from, selection.link.to) : -1;
  const selectedAgent = () => selection && 'agent' in selection ? agentOf(selection.agent) ?? null : null;
  const canvasWidth = () => scroll.clientWidth || 800;
  const nameOf = (id: string) => agentOf(id)?.name ?? 'an agent';
  const workOut = (id: string) => draft.links.filter(l => l.kind === 'work' && l.from === id).length;

  function setStatus(text: string, tone: 'info' | 'error' = 'info') {
    status = { text, tone }; statusLine.textContent = text; statusLine.dataset.tone = tone; statusLine.hidden = !text;
    statusLine.setAttribute('role', tone === 'error' ? 'alert' : 'status');
  }

  // ----------------------------------------------------------------------------------------------------------- reading
  /** Reads the stored team. What the user has edited and not applied is kept, unless `force`. */
  async function refresh(force = false) {
    if (reading || disposed) return; reading = true;
    try {
      const value = parseTeam(await ask<unknown>('team_get'));
      if (disposed) return;
      if (!value) { setStatus('The team could not be read.', 'error'); return; }
      snap = value;
      if (!value.enabled) { applied = null; draft = emptyFormation(); evaluation = null; selection = null; draw(); return; }
      if (status.text === 'Reading the team…') setStatus('');
      const stored = value.formation;
      if (force || !pending()) {
        const before = new Map(draft.agents.map(a => [a.id, a] as const));
        applied = stored; draft = stored ? clone(stored) : emptyFormation();
        // A card the user moved keeps its place (it is stored when its move ends, so the two agree again soon).
        if (!force) for (const a of draft.agents) { const mine = before.get(a.id); if (mine) { a.x = mine.x; a.y = mine.y; } }
        if (value.evaluation) { evaluation = value.evaluation; evaluatedFor = structureOf(draft); evaluating = false; }
        else request();
        if (selection && ('agent' in selection ? !agentOf(selection.agent) : selectedLink() < 0)) selection = null;
      } else if (stored && stored.revision !== applied?.revision) {
        setStatus(`The team was changed elsewhere (revision ${stored.revision}). Discard reads it again.`, 'error');
      }
      draw();
    } catch (error) { if (!disposed) setStatus(message(error), 'error'); }
    finally { reading = false; }
  }

  /** Asks the engine about the draft, at most once per EVALUATE_GAP_MS; the latest request is the one answered. */
  function request() {
    if (!draft.agents.length) { evaluation = null; evaluatedFor = ''; evaluating = false; draw(); return; }
    if (!pending() && snap?.evaluation) { evaluation = snap.evaluation; evaluatedFor = structureOf(draft); evaluating = false; return; }
    const asked = clone(draft); const key = structureOf(asked); evaluating = true;
    void evaluator.request('draft', asked).then(result => {
      // null: a newer request took its place and its answer comes; a different shape: the draft moved on.
      if (disposed || result === null || key !== structureOf(draft)) return;
      evaluating = false; evaluation = result; evaluatedFor = key;
      if (status.tone === 'error' && status.text.startsWith('The team could not be checked')) setStatus('');
      draw();
    }, error => { if (disposed) return; evaluating = false; setStatus(`The team could not be checked: ${message(error)}`, 'error'); draw(); });
  }

  /** The reason, if any, the engine gives against trial formations of the selected arrow (a struck-out option). */
  function requestTrials() {
    const index = selectedLink(); const key = `${structureOf(draft)}|${index}`;
    if (key === trialsFor) return;
    trialsFor = key; trials = {};
    if (index < 0) return;
    const l = draft.links[index];
    const trial = (name: string, change: Partial<Link>) => {
      const formation: Formation = { ...clone(draft), links: draft.links.map((x, i) => i === index ? { ...x, ...change } : { ...x }) };
      void evaluator.request(`trial:${name}`, formation).then(result => {
        if (disposed || trialsFor !== key || !result) return;
        const why = result.issues.find(i => i.level === 'error' && i.links.includes(index));
        trials[name] = why ? why.text : ''; box?.update();
      }, () => { /* no reason known: the option stays open and Apply is refused by the engine if it cannot work */ });
    };
    trial(l.kind === 'work' ? 'observes' : 'work', { kind: l.kind === 'work' ? 'observes' : 'work', report: 'done' });
    if (l.kind === 'work' && l.report !== 'strict') trial('strict', { report: 'strict' });
  }

  // ------------------------------------------------------------------------------------------------------------- editing
  /** Every change of the draft goes through here: the engine is asked again, the cards may be tidied, the view is drawn. */
  function edit(next: Formation, options: { tidy?: boolean } = {}) {
    if (applying || next === draft) return;
    const before = draft; draft = next;
    if (selection && ('agent' in selection ? !agentOf(selection.agent) : selectedLink() < 0)) selection = null;
    if (keepTidy && options.tidy !== false) placeByTidy();
    request(); requestTrials(); draw();
    const moved = draft.agents.filter(a => { const was = before.agents.find(x => x.id === a.id); return !!was && (was.x !== a.x || was.y !== a.y); }).map(a => a.id);
    if (moved.length) storePlaces(moved);
  }
  function placeByTidy() {
    const places = tidy(draft, current()?.roles ?? {}, canvasWidth());
    draft = { ...draft, agents: draft.agents.map(a => places[a.id] ? { ...a, x: Math.round(places[a.id].x), y: Math.round(places[a.id].y) } : a) };
  }
  /** Places are stored without a revision when nothing else is pending; otherwise they travel with Apply. */
  function storePlaces(ids: readonly string[]) {
    if (!applied || pending() || !ids.length) return;
    const places: Record<string, [number, number]> = {};
    for (const id of ids) { const a = agentOf(id); if (a) places[id] = [Math.round(a.x), Math.round(a.y)]; }
    if (!Object.keys(places).length) return;
    const stored = applied; applied = { ...stored, agents: stored.agents.map(a => places[a.id] ? { ...a, x: places[a.id][0], y: places[a.id][1] } : a) };
    void ask('team_place', { places }).catch(error => { if (!disposed) setStatus(`The places could not be stored: ${message(error)}`, 'error'); });
  }
  const updateAgent = (id: string, change: Partial<Agent>) => edit({ ...draft, agents: draft.agents.map(a => a.id === id ? { ...a, ...change } : a) }, { tidy: false });
  const updateLink = (index: number, change: Partial<Link>) => edit({ ...draft, links: draft.links.map((l, i) => i === index ? { ...l, ...change } : l) }, { tidy: 'kind' in change });

  function addSessionAt(session: TeamSession, at: Point) {
    const label = session.harness ? deps.harnessLabel(session.harness) : 'Session';
    const base = session.title && session.title !== label ? session.title : label;
    const taken = new Set(draft.agents.map(a => a.name)); let name = base;
    for (let n = 2; taken.has(name) && n < 100; n += 1) name = `${base} ${n}`;
    const next = addSession(draft, session.sessionId, name, at);
    if (next === draft) { setStatus(draft.agents.length >= MAX_AGENTS ? `The map holds ${MAX_AGENTS} agents at most.` : 'That session is already on the map.', 'error'); draw(); return; }
    setStatus(''); edit(next);
    const added = next.agents.find(a => a.session === session.sessionId);
    if (added) cards.get(added.id)?.focus({ preventScroll: true });
  }
  function freeSpot(): Point {
    for (let n = 0; n < 200; n += 1) {
      const p = { x: 30 + (n % 3) * (CARD.width + 36), y: 40 + Math.floor(n / 3) * (CARD.height + 30) };
      if (!draft.agents.some(a => Math.abs(a.x - p.x) < CARD.width && Math.abs(a.y - p.y) < CARD.height)) return p;
    }
    return { x: 30, y: 40 };
  }
  function connectAgents(from: string, to: string) {
    const next = connect(draft, from, to);
    if (next === draft) { setStatus(from === to ? 'An agent cannot give work to itself.' : `The map holds ${MAX_LINKS} arrows at most.`, 'error'); draw(); return; }
    setStatus(''); selection = { link: { from, to } }; edit(next);
    focusBox();
  }
  function takeOff(id: string) {
    if (selection && 'agent' in selection && selection.agent === id) selection = null;
    edit(removeAgent(draft, id));
  }
  function tidyNow() {
    placeByTidy(); draw(); storePlaces(draft.agents.map(a => a.id));
  }
  function arrangeAs(shape: 'star' | 'chain' | 'peers') {
    if (applying || draft.agents.length < 2) return;
    const hub = selection && 'agent' in selection ? selection.agent : null;
    const next = arrange(draft, shape, hub, canvasWidth());
    edit(next, { tidy: false });
  }

  async function applyNow() {
    if (applying || !pending() || errors().length || !fresh()) return;
    const sent = clone(draft); applying = true; setStatus(''); draw();
    try {
      const result = await ask<{ formation?: unknown }>('team_apply', { formation: sent, expectedRevision: applied?.revision ?? 0 });
      const stored = parseFormation(result?.formation);
      if (!stored) throw 'The answer to Apply was not understood. The team may have been stored: Discard reads it again.';
      applied = stored; draft = clone(stored); selection = null; trialsFor = ''; evaluation = null; evaluatedFor = ''; if (snap) snap = { ...snap, evaluation: null };
      setStatus(`Applied: revision ${stored.revision}. Each session is told its place when it is in the room.`);
      applying = false; request(); draw();
      await refresh();
    } catch (error) { applying = false; setStatus(message(error), 'error'); draw(); }
  }
  async function discardNow() {
    selection = null; trialsFor = ''; evaluator.cancel(); setStatus('');
    await refresh(true);
  }

  // -------------------------------------------------------------------------------------------------------- the pieces
  const cards = new Map<string, HTMLElement>();

  /** What the agents are told about one agent: its own fields and the arrows it is on (places and the revision are not). */
  const signature = (f: Formation, a: Agent) => JSON.stringify([[a.session, a.name, a.notes, a.share, a.model],
    f.links.filter(l => l.from === a.id || l.to === a.id).map(l => [l.from, l.to, l.kind, l.kind === 'work' ? l.report : null, l.words]).sort()]);
  function toldState(a: Agent): [string, string] {
    const before = applied?.agents.find(x => x.id === a.id);
    if (!applied || !before) return ['edited', 'new · not applied'];
    if (signature(draft, a) !== signature(applied, before)) return ['edited', 'changed · not applied'];
    const told = snap?.told[a.id];
    if (told) return ['told', `told · revision ${told.revision}`];
    return ['pending', sessionOf(a)?.connection === 'in_room' ? 'not told yet' : 'told when it joins'];
  }

  function drawCards() {
    const ids = new Set(draft.agents.map(a => a.id));
    for (const [id, node] of cards) if (!ids.has(id)) { node.remove(); cards.delete(id); }
    const bad = new Set(errors().flatMap(i => i.agents));
    const roles = current()?.roles ?? {};
    for (const a of draft.agents) {
      let node = cards.get(a.id);
      if (!node) {
        node = el('div', 'team-card'); node.dataset.agent = a.id; node.tabIndex = 0; node.setAttribute('role', 'button'); node.setAttribute('aria-haspopup', 'dialog');
        cards.set(a.id, node); nodes.append(node);
        const id = a.id;
        node.onpointerdown = event => press(event, id, 'move');
        node.onkeydown = event => cardKey(event, id);
      }
      node.style.left = `${a.x}px`; node.style.top = `${a.y}px`;
      const session = sessionOf(a); const harness = session?.harness ?? '';
      node.dataset.harness = harness;
      node.setAttribute('aria-pressed', String(!!selection && 'agent' in selection && selection.agent === a.id));
      node.toggleAttribute('data-in', session?.connection === 'in_room');
      const [told, toldText] = toldState(a);
      node.toggleAttribute('data-pending', told === 'edited');
      node.toggleAttribute('data-bad', bad.has(a.id));
      const head = el('div', 'team-card-top'); const dot = el('span', 'team-dot'); dot.setAttribute('aria-hidden', 'true');
      head.append(dot, el('span', 'team-card-name', a.name || '(no name)'));
      if (bad.has(a.id)) { const mark = el('span', 'team-card-bad', '✕'); mark.title = 'A problem with this agent: see the list above the map'; head.append(mark); }
      const view = roles[a.id]; const roleName = view ? ROLE_NAME[view.role] ?? view.role : 'checking…';
      const role = el('div', 'team-card-role', view && workOut(a.id) > 1 ? `${roleName} · ${SHARE_SHORT[a.share]}` : roleName);
      if (view) role.title = `${roleName}: ${view.detail}. Inferred from its arrows.`;
      const meta = el('div', 'team-card-meta', `${programOf(a)}${session?.title ? ` · ${session.title}` : ''}${session?.memberName ? ` · ${session.memberName}` : ''}`);
      const toldNode = el('span', 'team-told', toldText); toldNode.dataset.s = told;
      const left = session?.wakes != null && session.wakes >= 7 ? `${Math.max(0, 10 - session.wakes)} wakes left` : '';
      const wakes = el('span', 'team-wakes', left);
      const foot = el('div', 'team-card-foot'); foot.append(toldNode, wakes);
      const port = el('button', 'team-port', '→'); port.type = 'button'; port.tabIndex = -1; port.setAttribute('aria-hidden', 'true');
      port.title = 'Pull onto another card to connect them (the keyboard: open the card and use Connect to…)';
      const id = a.id; port.onpointerdown = event => { event.stopPropagation(); press(event, id, 'link'); };
      node.replaceChildren(head, role, meta, foot, port);
      node.setAttribute('aria-label', `${a.name}. ${roleName}${view && workOut(a.id) > 1 ? `, ${SHARE_SHORT[a.share]}` : ''}. ${toldText || 'in sync'}. ${bad.has(a.id) ? 'Has a problem. ' : ''}Enter opens its box.`);
    }
    const height = Math.max(380, ...draft.agents.map(a => a.y + CARD.height + 60));
    canvas.style.height = `${height}px`;
  }

  function endsOf(l: Link): [Point, Point] | null {
    const from = agentOf(l.from), to = agentOf(l.to); if (!from || !to) return null;
    return [borderPoint(rectOf(from), centreOf(to), 3), borderPoint(rectOf(to), centreOf(from), 8)];
  }
  function drawWires() {
    // The arrows are drawn again at every change; one that had the focus gets it back.
    const held = (document.activeElement as Element | null)?.closest<SVGElement>('.team-link');
    const keep = held && wireLayer.contains(held) ? { from: held.dataset.from, to: held.dataset.to } : null;
    wireLayer.replaceChildren();
    const mark = pendingLinks(applied, draft); const bad = new Set(errors().flatMap(i => i.links));
    const selected = selectedLink();
    draft.links.forEach((l, i) => {
      const ends = endsOf(l); if (!ends) return;
      const [a, b] = ends; const d = `M${a.x} ${a.y}L${b.x} ${b.y}`;
      const group = svg('g', { class: 'team-link', tabindex: '0', role: 'button' });
      group.dataset.from = l.from; group.dataset.to = l.to;
      const wrong = bad.has(i);
      const said = l.kind === 'work' ? (l.report === 'strict' ? 'gives work · only done / blocked' : l.report === 'steps' ? 'gives work · every step' : 'gives work to') : 'observes';
      const short = l.words.length > 26 ? `${l.words.slice(0, 25)}…` : l.words;
      group.setAttribute('aria-label', `${wrong ? 'Problem: ' : ''}${nameOf(l.from)} ${KIND_NAME[l.kind]} ${nameOf(l.to)}${l.kind === 'work' ? `, ${REPORTS.find(r => r[0] === l.report)?.[1].toLowerCase()}` : ''}${mark.has(i) ? ', not applied' : ''}. Enter opens its box.`);
      const hit = svg('path', { class: 'team-hit', d });
      const line = svg('path', { class: 'team-wire', d, 'marker-end': `url(#${viewId}-head-${wrong ? 'bad' : l.kind === 'observes' ? 'observes' : 'work'})` });
      line.dataset.kind = l.kind; if (mark.has(i)) line.setAttribute('data-pending', ''); if (selected === i) line.setAttribute('data-on', ''); if (wrong) line.setAttribute('data-bad', '');
      const label = svg('text', { class: 'team-wire-label', x: String((a.x + b.x) / 2), y: String((a.y + b.y) / 2 - 6), 'text-anchor': 'middle' });
      label.dataset.kind = l.kind; if (wrong) label.setAttribute('data-bad', '');
      label.textContent = `${wrong ? '✕ ' : ''}${said}${short ? ` · “${short}”` : ''}`;
      const title = svg('title'); title.textContent = `${nameOf(l.from)} ${KIND_NAME[l.kind]} ${nameOf(l.to)}: click to change`;
      group.append(title, hit, line, label);
      const open = () => { selection = selectedLink() === i ? null : { link: { from: l.from, to: l.to } }; requestTrials(); draw(); if (selection) focusBox(); };
      group.addEventListener('pointerdown', event => { event.stopPropagation(); });
      group.addEventListener('click', open);
      group.addEventListener('keydown', event => { if (confirmsField(event) || event.key === ' ') { event.preventDefault(); open(); } });
      wireLayer.append(group);
    });
    if (keep) wireLayer.querySelector<SVGElement>(`.team-link[data-from="${keep.from}"][data-to="${keep.to}"]`)?.focus({ preventScroll: true });
  }

  function drawSide() {
    const onMap = new Set(draft.agents.map(a => a.session));
    const rest = (snap?.sessions ?? []).filter(s => !onMap.has(s.sessionId));
    const full = draft.agents.length >= MAX_AGENTS;
    sideList.replaceChildren(...rest.map(s => {
      const item = el('li'); const b = el('button', 'team-session'); b.type = 'button'; b.dataset.session = s.sessionId; b.dataset.harness = s.harness ?? '';
      const label = s.harness ? deps.harnessLabel(s.harness) : 'Session';
      b.append(el('b', undefined, s.title && s.title !== label ? s.title : label), el('span', undefined, `${label} · ${s.state.replace('_', ' ')}${s.connection === 'unsupported' ? ' · cannot take part' : ''}`));
      b.disabled = s.connection === 'unsupported' || full;
      b.title = s.connection === 'unsupported' ? 'Shinatama gives this agent no Mukade tools: it cannot take part in a team.' : full ? `The map holds ${MAX_AGENTS} agents at most.` : 'Drag onto the map, or press to add it';
      b.setAttribute('aria-label', `Add ${b.firstElementChild?.textContent ?? label} (${label}) to the map`);
      b.onpointerdown = event => press(event, s.sessionId, 'session');
      b.onclick = event => { if (event.detail === 0) addSessionAt(s, freeSpot()); };
      item.append(b); return item;
    }));
    sideNote.textContent = !snap?.sessions.length ? 'This thread has no session yet. + on its row adds one.' : rest.length ? '' : 'Every session of the thread is on the map.';
  }

  function drawTray() {
    tray.replaceChildren();
    for (const kind of ['work', 'observes'] as const) { const key = el('span', 'team-key'); const line = el('i'); line.dataset.kind = kind; key.append(line, KIND_NAME[kind]); tray.append(key); }
    tray.append(el('span', 'team-sep'), draft.agents.length ? 'Drag a card down here to take it off the map (its session stays open)' : '');
  }

  function drawProblems() {
    const issues = fresh() ? current()!.issues : [];
    problems.hidden = !issues.length;
    problems.replaceChildren(...issues.map(issue => {
      const li = el('li'); const b = el('button'); b.type = 'button'; b.dataset.level = issue.level;
      b.append(issue.level === 'error' ? 'Error: ' : 'Warning: ', issue.text);
      b.onclick = () => {
        const l = draft.links[issue.links[0]];
        selection = l ? { link: { from: l.from, to: l.to } } : issue.agents[0] && agentOf(issue.agents[0]) ? { agent: issue.agents[0] } : null;
        requestTrials(); draw(); if (selection) focusBox();
      };
      li.append(b); return li;
    }));
  }

  function drawBar() {
    const bad = errors().length; const dirty = pending();
    const members = applied?.agents.length ?? 0;
    const waiting = (applied?.agents ?? []).filter(a => !snap?.told[a.id]).length;
    let text: string; let tone: string;
    if (applying) { text = 'applying…'; tone = 'dirty'; }
    else if (bad) { text = `✕ ${bad} problem${bad > 1 ? 's' : ''} to fix before it can be applied`; tone = 'error'; }
    else if (dirty) { text = 'not applied: nothing was sent yet'; tone = 'dirty'; }
    else if (!members) { text = 'no team'; tone = ''; }
    else if (waiting) { text = `revision ${applied!.revision} · ${waiting} told when ${waiting > 1 ? 'they join' : 'it joins'}`; tone = 'dirty'; }
    else { text = `revision ${applied!.revision} · everyone was told`; tone = 'ok'; }
    state.textContent = text; state.dataset.tone = tone;
    const ready = !draft.agents.length || (fresh() && !evaluating);
    apply.disabled = !snap?.enabled || !dirty || bad > 0 || applying || !ready;
    apply.title = bad ? 'Fix the problems above first' : !dirty ? 'Nothing to apply' : !ready ? 'Checking the team…' : 'Tell each concerned session its place';
    discard.disabled = !dirty || applying;
    tidyButton.disabled = draft.agents.length < 1 || applying;
    for (const b of shapeButtons) b.disabled = draft.agents.length < 2 || applying;
  }

  // ------------------------------------------------------------------------------------------------------------- boxes
  function place(element: HTMLElement, x: number, y: number, width: number) {
    element.style.left = `${Math.max(8, Math.min(x, canvasWidth() - width - 8))}px`; element.style.top = `${Math.max(8, y)}px`;
  }
  const closeButton = (what: string) => { const b = quietButton('×'); b.setAttribute('aria-label', `Close ${what}`); b.onclick = () => closeBox(true); return b; };

  function agentBox(a: Agent): Box {
    const element = el('div', 'team-box team-box-agent'); element.setAttribute('role', 'dialog'); element.setAttribute('aria-label', `Agent ${a.name}`);
    const head = el('h3', undefined, 'Agent · running session'); head.append(closeButton('the agent box'));
    const name = el('input'); name.maxLength = MAX_NAME; name.value = a.name; name.id = `${viewId}-name`; name.dataset.first = '1';
    name.oninput = () => { const clean = plain(name.value); if (clean !== name.value) name.value = clean; if (clean.trim()) updateAgent(a.id, { name: clean }); };
    name.onblur = () => { if (!name.value.trim()) name.value = agentOf(a.id)?.name ?? 'Agent'; };
    const nameLabel = el('label', undefined, 'Name'); nameLabel.append(name);
    // Phase 3 puts a model menu here for an agent without a session; a running one keeps its own, as now.
    const model = el('input'); model.disabled = true; model.value = `${programOf(a)} · its own sign-in and model`; model.title = 'A running session keeps its model';
    const modelLabel = el('label', undefined, 'Model'); modelLabel.append(model);
    const role = el('p', 'role-line'); const roleName = el('b'); const roleDetail = el('span'); role.append(roleName, roleDetail);
    const inferred = el('p', undefined, 'The role is inferred from its arrows: change an arrow and it changes.');
    const handOut = el('div'); const handLabel = el('p', 'lab'); const seg = el('div', 'team-seg'); seg.setAttribute('role', 'group'); handOut.append(handLabel, seg);
    const shareButtons = (Object.keys(SHARE_NAME) as Share[]).map(key => {
      const b = el('button', undefined, SHARE_NAME[key]); b.type = 'button'; b.title = SHARE_TIP[key]; b.dataset.share = key;
      b.onclick = () => updateAgent(a.id, { share: key }); seg.append(b); return b;
    });
    const notes = el('textarea'); notes.value = a.notes; notes.placeholder = 'Anything the arrows do not say, in your words.'; notes.rows = 3;
    notes.oninput = () => { let clean = plain(notes.value); while (bytes(clean) > MAX_NOTES_BYTES) clean = clean.slice(0, -1); if (clean !== notes.value) notes.value = clean; updateAgent(a.id, { notes: clean }); };
    const notesLabel = el('label', undefined, 'Notes for it (optional)'); notesLabel.append(notes);
    const to = el('select'); to.setAttribute('aria-label', 'Connect to…'); to.title = 'The keyboard\'s way to draw an arrow: this agent gives work to the one chosen';
    const toLabel = el('label', undefined, 'Connect to…'); toLabel.append(to);
    to.onchange = () => { const id = to.value; to.value = ''; if (id) connectAgents(a.id, id); };
    const fields = el('div', 'team-fields'); fields.append(nameLabel, modelLabel, role, inferred, handOut, notesLabel, toLabel);
    const show = quietButton('Show its tab', 'Show this session\'s tab in the window'); show.onclick = () => { if (a.session) deps.focusSession(a.session); }; show.hidden = !a.session;
    const out = quietButton('Take off the map', 'The session stays open; it only leaves the team map'); out.onclick = () => takeOff(a.id);
    const foot = el('div', 'foot'); foot.append(show, out);
    element.append(head, fields, foot);
    const update = () => {
      const live = agentOf(a.id); if (!live) return;
      const view = current()?.roles[a.id];
      roleName.textContent = view ? ROLE_NAME[view.role] ?? view.role : 'checking…'; roleDetail.textContent = view ? ` · ${view.detail}` : '';
      const gives = draft.links.filter(l => l.kind === 'work' && l.from === a.id).map(l => nameOf(l.to));
      handOut.hidden = gives.length < 2;
      handLabel.textContent = `How it hands out work to ${gives.length > 1 ? `${gives.slice(0, -1).join(', ')} and ${gives.at(-1)}` : ''}`;
      for (const b of shareButtons) b.setAttribute('aria-pressed', String(live.share === b.dataset.share));
      const others = draft.agents.filter(x => x.id !== a.id);
      const full = draft.links.length >= MAX_LINKS;
      const wanted = ['', ...others.map(x => x.id)].join('|');
      if (to.dataset.options !== wanted) { to.replaceChildren(new Option('Connect to…', ''), ...others.map(x => new Option(x.name, x.id))); to.dataset.options = wanted; }
      for (const option of Array.from(to.options)) if (option.value) option.textContent = nameOf(option.value);
      to.disabled = full || !others.length; to.title = full ? `The map holds ${MAX_LINKS} arrows at most.` : to.title;
    };
    const live = agentOf(a.id); if (live) { place(element, live.x + CARD.width + 22 + 262 < canvasWidth() ? live.x + CARD.width + 22 : live.x - 274, live.y - 16, 262); }
    update(); return { key: `agent:${a.id}`, element, update };
  }

  function linkBox(index: number): Box {
    const l = draft.links[index]; const from = l.from, to = l.to;
    const element = el('div', 'team-box team-box-link'); element.setAttribute('role', 'dialog');
    const head = el('h3'); const title = el('span'); head.append(title, closeButton('the arrow box'));
    const kinds = el('div', 'team-kinds'); kinds.setAttribute('role', 'group'); kinds.setAttribute('aria-label', 'Kind of relation');
    const kindButtons = (['work', 'observes'] as const).map(kind => {
      const b = el('button'); b.type = 'button'; b.dataset.kind = kind; b.dataset.first = '1'; const i = el('i'); i.dataset.kind = kind; const name = el('b'); const why = el('span'); b.append(i, name, why);
      b.onclick = () => { const at = linkIndex(from, to); if (at >= 0 && draft.links[at].kind !== kind) updateLink(at, { kind, report: 'done' }); };
      kinds.append(b); return { b, name, why, kind };
    });
    const howLabel = el('p', 'lab'); const seg = el('div', 'team-seg'); seg.setAttribute('role', 'group');
    const reportButtons = REPORTS.map(([key, text, tip]) => {
      const b = el('button', undefined, text); b.type = 'button'; b.title = tip; b.dataset.report = key;
      b.onclick = () => { const at = linkIndex(from, to); if (at >= 0) updateLink(at, { report: key }); }; seg.append(b); return b;
    });
    const reportReason = el('p', 'team-reason');
    const wordsLabel = el('label', undefined, 'In your words (optional)'); const words = el('input'); words.maxLength = MAX_WORDS; words.value = l.words;
    words.oninput = () => { const clean = plain(words.value).slice(0, MAX_WORDS); if (clean !== words.value) words.value = clean; const at = linkIndex(from, to); if (at >= 0) updateLink(at, { words: clean }); };
    wordsLabel.append(words);
    const says = el('p', 'team-says');
    const flip = quietButton('⇄ Reverse'); flip.dataset.act = 'reverse';
    flip.onclick = () => {
      const at = linkIndex(from, to); if (at < 0) return;
      const had = draft.links.findIndex(x => x.from === to && x.to === from); if (had >= 0) return;
      selection = { link: { from: to, to: from } }; edit({ ...draft, links: draft.links.map((x, i) => i === at ? { ...x, from: x.to, to: x.from } : x) });
      box?.element.querySelector<HTMLElement>('[data-act="reverse"]')?.focus();
    };
    const del = quietButton('Delete the arrow'); del.dataset.act = 'delete';
    del.onclick = () => { const at = linkIndex(from, to); if (at < 0) return; selection = null; edit({ ...draft, links: draft.links.filter((_, i) => i !== at) }); };
    const foot = el('div', 'foot'); foot.append(flip, del);
    const how = el('div'); how.append(howLabel, seg, reportReason);
    const fields = el('div', 'team-fields'); fields.append(kinds, how, wordsLabel, says);
    element.append(head, fields, foot);
    const update = () => {
      const at = linkIndex(from, to); if (at < 0) return; const now = draft.links[at];
      const A = nameOf(from), B = nameOf(to);
      title.textContent = `${A} → ${B}`; element.setAttribute('aria-label', `Arrow from ${A} to ${B}`);
      for (const { b, name, why, kind } of kindButtons) {
        const struck = kind !== now.kind ? trials[kind] : '';
        b.setAttribute('aria-pressed', String(now.kind === kind)); b.disabled = !!struck;
        name.textContent = `${A} ${KIND_NAME[kind]} ${B}`;
        why.textContent = struck ? `✕ ${struck}` : KIND_TIP[kind](A, B);
      }
      how.hidden = now.kind !== 'work'; howLabel.textContent = `How ${B} reports`;
      const strict = trials.strict ?? '';
      for (const b of reportButtons) {
        const struck = b.dataset.report === 'strict' && now.report !== 'strict' && !!strict;
        b.setAttribute('aria-pressed', String(now.report === b.dataset.report)); b.disabled = struck;
        if (b.dataset.report === 'strict') b.title = struck ? `✕ ${strict}` : REPORTS[2][2];
      }
      reportReason.textContent = now.report !== 'strict' && strict ? `✕ Only done / blocked: ${strict}` : '';
      reportReason.hidden = !reportReason.textContent;
      const pair = fresh() ? current()!.sentences[at] : undefined;
      says.textContent = pair ? `${A} is told: ${pair[0]}\n\n${B} is told: ${pair[1]}` : 'Checking what each is told…';
      flip.disabled = draft.links.some(x => x.from === to && x.to === from);
      const ends = endsOf(now); if (ends && !element.dataset.placed) { place(element, (ends[0].x + ends[1].x) / 2 - 165, (ends[0].y + ends[1].y) / 2 + 14, 330); element.dataset.placed = '1'; }
    };
    update(); return { key: `link:${from}>${to}`, element, update };
  }

  function drawBoxes() {
    const wanted = selection && 'agent' in selection ? (selectedAgent() ? `agent:${selection.agent}` : '') : selection && selectedLink() >= 0 ? `link:${selection.link.from}>${selection.link.to}` : '';
    if (box && box.key === wanted) { box.update(); return; }
    box?.element.remove(); box = null;
    if (!wanted) return;
    box = selection && 'agent' in selection ? agentBox(selectedAgent()!) : linkBox(selectedLink());
    canvas.append(box.element);
  }
  function focusBox() { const at = box?.element; (at?.querySelector<HTMLElement>('[data-first]:not(:disabled)') ?? at?.querySelector<HTMLElement>('input:not(:disabled)'))?.focus(); }
  function closeBox(refocus: boolean) {
    const was = selection; selection = null; draw();
    if (!refocus || !was) return;
    if ('agent' in was) cards.get(was.agent)?.focus({ preventScroll: true });
    else wireLayer.querySelector<SVGElement>(`.team-link[data-from="${was.link.from}"][data-to="${was.link.to}"]`)?.focus();
  }

  // --------------------------------------------------------------------------------------------------------------- draw
  function draw() {
    if (disposed || grab) return;
    const enabled = snap?.enabled !== false;
    off.hidden = enabled; body.hidden = !enabled; bar.hidden = !enabled;
    if (!enabled) { problems.hidden = true; off.textContent = 'The sessions of this thread do not talk to each other, so it has no team. Turn it on in the thread\'s settings (⋯).'; return; }
    drawBar(); drawProblems(); drawSide(); drawCards(); drawWires(); drawBoxes(); drawTray();
    hint.hidden = draft.agents.length > 0 || !snap;
    if (status.text === 'Reading the team…' && snap) setStatus('');
    root.dataset.pending = String(pending());
  }

  // -------------------------------------------------------------------------------------------------- pointer and keys
  const canvasPoint = (event: PointerEvent): Point => { const r = canvas.getBoundingClientRect(); return { x: event.clientX - r.left, y: event.clientY - r.top }; };
  const over = (event: PointerEvent) => document.elementFromPoint(event.clientX, event.clientY);
  const overTray = (event: PointerEvent) => !!over(event)?.closest('[data-zone="tray"]');
  const overMap = (event: PointerEvent) => !!over(event)?.closest('.team-scroll');

  function press(event: PointerEvent, id: string, kind: Grab['kind']) {
    if (event.button !== 0 || grab || applying) return;
    const target = event.currentTarget as HTMLElement;
    const rect = (kind === 'session' ? target : cards.get(id) ?? target).getBoundingClientRect();
    grab = { kind, id, pointer: event.pointerId, target, startX: event.clientX, startY: event.clientY, dx: event.clientX - rect.left, dy: event.clientY - rect.top, moved: false };
    if (kind !== 'session') event.preventDefault();
    try { target.setPointerCapture(event.pointerId); } catch { /* a synthetic pointer: the move events still arrive */ }
    target.addEventListener('pointermove', moveGrab); target.addEventListener('pointerup', releaseGrab); target.addEventListener('pointercancel', cancelGrab);
  }
  function endGrab() {
    const g = grab; if (!g) return null; grab = null;
    g.target.removeEventListener('pointermove', moveGrab); g.target.removeEventListener('pointerup', releaseGrab); g.target.removeEventListener('pointercancel', cancelGrab);
    try { g.target.releasePointerCapture(g.pointer); } catch { /* already released */ }
    g.ghost?.remove(); g.rubber?.remove(); tray.removeAttribute('data-drop'); scroll.removeAttribute('data-drop');
    for (const node of cards.values()) { node.removeAttribute('data-over'); node.removeAttribute('data-lifted'); }
    return g;
  }
  function moveGrab(event: PointerEvent) {
    const g = grab; if (!g || event.pointerId !== g.pointer) return;
    if (!g.moved && Math.hypot(event.clientX - g.startX, event.clientY - g.startY) < 4) return;
    g.moved = true;
    if (g.kind === 'move') {
      const a = agentOf(g.id); if (!a) return; const p = canvasPoint(event);
      a.x = Math.max(0, Math.round(p.x - g.dx)); a.y = Math.max(0, Math.round(p.y - g.dy));
      const node = cards.get(g.id); if (node) { node.dataset.lifted = ''; node.style.left = `${a.x}px`; node.style.top = `${a.y}px`; }
      tray.toggleAttribute('data-drop', overTray(event)); drawWires();
    } else if (g.kind === 'link') {
      const a = agentOf(g.id); if (!a) return; const p = canvasPoint(event);
      const start = borderPoint(rectOf(a), p, 3);
      g.rubber ??= (() => { const r = svg('path', { class: 'team-wire rubber', 'marker-end': `url(#${viewId}-head-rubber)` }); wireLayer.append(r); return r; })();
      g.rubber.setAttribute('d', `M${start.x} ${start.y}L${p.x} ${p.y}`);
      const hover = over(event)?.closest<HTMLElement>('.team-card');
      for (const [id, node] of cards) node.toggleAttribute('data-over', node === hover && id !== g.id);
    } else {
      g.ghost ??= (() => { const gh = el('div', 'team-ghost', g.target.querySelector('b')?.textContent ?? 'Session'); gh.dataset.harness = g.target.dataset.harness ?? ''; document.body.append(gh); return gh; })();
      g.ghost.style.transform = `translate(${event.clientX + 12}px, ${event.clientY + 8}px)`;
      scroll.toggleAttribute('data-drop', overMap(event));
    }
  }
  function releaseGrab(event: PointerEvent) {
    const g = grab; if (!g || event.pointerId !== g.pointer) return;
    const droppedOnTray = overTray(event), droppedOnMap = overMap(event), hover = over(event)?.closest<HTMLElement>('.team-card[data-agent]');
    const point = canvasPoint(event);
    endGrab();
    if (g.kind === 'move') {
      if (!g.moved) { selection = selection && 'agent' in selection && selection.agent === g.id ? null : { agent: g.id }; requestTrials(); draw(); return; }
      if (droppedOnTray) { takeOff(g.id); return; }
      draw(); if (!pending()) storePlaces([g.id]);
      if (keepTidy) { /* a placed card is the user's word for now: tidying happens at the next change of arrows */ }
    } else if (g.kind === 'link') {
      const to = hover?.dataset.agent; if (g.moved && to && to !== g.id) connectAgents(g.id, to); else draw();
    } else {
      const s = snap?.sessions.find(x => x.sessionId === g.id); if (!s) return;
      if (g.moved && droppedOnMap) addSessionAt(s, { x: Math.max(0, Math.round(point.x - CARD.width / 2)), y: Math.max(0, Math.round(point.y - CARD.height / 2)) });
      else if (!g.moved) addSessionAt(s, freeSpot());
    }
  }
  function cancelGrab() { const g = endGrab(); if (g) draw(); }

  function cardKey(event: KeyboardEvent, id: string) {
    if (event.target !== event.currentTarget) return;
    if (confirmsField(event) || event.key === ' ') {
      event.preventDefault(); selection = selection && 'agent' in selection && selection.agent === id ? null : { agent: id }; requestTrials(); draw(); if (selection) focusBox();
    } else if (event.key.startsWith('Arrow') && !applying) {
      event.preventDefault(); const step = event.shiftKey ? 40 : 10; const a = agentOf(id); if (!a) return;
      const dx = event.key === 'ArrowLeft' ? -step : event.key === 'ArrowRight' ? step : 0, dy = event.key === 'ArrowUp' ? -step : event.key === 'ArrowDown' ? step : 0;
      a.x = Math.max(0, a.x + dx); a.y = Math.max(0, a.y + dy); draw(); cards.get(id)?.focus({ preventScroll: true });
      window.clearTimeout(nudge); nudge = window.setTimeout(() => { if (!pending()) storePlaces([id]); }, 400);
    }
  }
  let nudge = 0;

  root.addEventListener('keydown', event => {
    if (event.key === 'Escape' && selection && !event.defaultPrevented) { event.preventDefault(); event.stopPropagation(); closeBox(true); }
  });
  scroll.addEventListener('pointerdown', event => { if (!grab && selection && !(event.target as Element).closest('.team-box, .team-card, .team-link')) closeBox(false); });
  tidyButton.onclick = () => tidyNow();
  autoBox.onchange = () => { keepTidy = autoBox.checked; writeKeepTidy(keepTidy); if (keepTidy) tidyNow(); };
  apply.onclick = () => { void applyNow(); };
  discard.onclick = () => { void discardNow(); };
  new ResizeObserver(() => { if (!disposed && snap) { drawCards(); drawWires(); box?.update(); } }).observe(scroll);

  timer = window.setInterval(() => { if (visible && !document.hidden) void refresh(); }, POLL_MS);
  const visibility = () => { if (visible && !document.hidden) void refresh(); };
  document.addEventListener('visibilitychange', visibility);
  draw(); void refresh();

  return {
    viewId, element: root, thread,
    setVisible(next: boolean) { if (visible === next) return; visible = next; if (next) void refresh(); },
    focus() { (cards.values().next().value ?? root).focus({ preventScroll: true }); },
    dispose() { disposed = true; window.clearInterval(timer); window.clearTimeout(nudge); evaluator.cancel(); document.removeEventListener('visibilitychange', visibility); endGrab(); root.remove(); onClose(); },
  };
}

/** The team maps of a window, one per thread, with the shape `createBrowserTabs` gives the pane host. */
export function createTeamMap(deps: TeamMapDeps) {
  const maps = new Map<string, ReturnType<typeof build>>();
  return {
    has: (viewId: string) => maps.has(viewId),
    /** Opens the map of a thread, or returns the one already open. The caller places the view. */
    open(thread: TeamThread): { viewId: string; element: HTMLElement } {
      const viewId = teamViewId(thread.id); const held = maps.get(viewId);
      if (held) return { viewId, element: held.element };
      const map = build(thread, deps, () => maps.delete(viewId)); maps.set(viewId, map);
      return { viewId, element: map.element };
    },
    /** Closes one map, or all of them. The thread's team is not touched. */
    close(viewId?: string) { for (const [id, map] of [...maps]) if (!viewId || id === viewId) map.dispose(); },
    focus(viewId: string) { maps.get(viewId)?.focus(); },
    setVisible(viewId: string, visible: boolean) { maps.get(viewId)?.setVisible(visible); },
    label(viewId: string): string {
      const id = threadOfView(viewId); const name = id ? deps.title(id) : '';
      return `Team${name ? ` · ${name}` : ''}${maps.has(viewId) ? '' : ' · closed'}`.slice(0, 80);
    },
    /** A saved tab whose map is not open (after a restart): a button to open it again. */
    placeholder(viewId: string, reopen: (thread: TeamThread) => void): HTMLElement | null {
      if (maps.has(viewId)) return null;
      const id = threadOfView(viewId); const card = el('div', 'team-map team-closed');
      card.append(el('p', undefined, 'The team map of this thread is closed.'));
      if (id) { const b = el('button', undefined, 'Open the team map'); b.type = 'button'; b.onclick = () => reopen({ id }); card.append(b); }
      return card;
    },
  };
}
export type TeamMap = ReturnType<typeof createTeamMap>;

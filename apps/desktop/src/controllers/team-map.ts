// THE TEAM MAP of one thread (spec 2026-11-17-thread-team-design.md, sections 1, 2, 4, 5, 11): the agents of a thread as cards the person
// places, their relations as arrows the person draws, and Apply, which has Shinatama tell each session its place. Roles, checks and the
// sentences sent to the sessions are the engine's (`team_evaluate`); this asks and draws, and computes none of them (ADR-0057: it never
// reads, waits for or acknowledges a message, and holds no member handle or credential).
//
// An agent may have no session yet (+ New agent, or a template): it takes a model from the menu of the Models settings and Apply opens a
// session for it, or a session chosen for it fills it. The screens (`screens/team`) draw this class and call its methods; nothing here
// knows React or the DOM, so `npm test` drives it with a table of answers in place of Rust.
import { errorText } from '../api/core.ts';
import { teamApi } from '../api/team.ts';
import { Store } from '../store.ts';
import { createEvaluator } from '../team-map-evaluate.ts';
import {
  CARD, MAX_AGENTS, MAX_LINKS, MAX_NAME, addAgent, addSession, arrange, changed as formationChanged, connect, fillAgent, fromTemplate, holdsModel,
  newAgentId, parseEvaluation, parseFormation, parseTeam, parseTemplates, pendingLinks, plainText, removeAgent, tidy,
  type Agent, type Evaluation, type Formation, type Issue, type Link, type ModelRef, type Point, type Report, type Share, type TeamModel,
  type TeamSession, type TeamSnapshot, type Template,
} from '../team-map-model.ts';

export type TeamThread = { id: string };
/** What the map needs of the rest of the window. The controller of the team gives it; a test gives a table. */
export interface TeamHost {
  /** The thread's name as the sidebar shows it. */
  title(threadId: string): string;
  harnessLabel(harness: string): string;
  /** Shows the session's tab; starts nothing. */
  focusSession(sessionId: string): void;
  /** A notice; with `undo`, a button that takes the change back. */
  say(text: string, say?: { error?: boolean; undo?: () => void }): void;
  /** Asks before what cannot be taken back. */
  ask(question: { title: string; description: string; confirm: string; danger?: boolean }): Promise<boolean>;
}

/** What is chosen on the map: an agent, or an arrow by its two ends. */
export type Selection = { agent: string } | { link: { from: string; to: string } } | null;
export type Status = { text: string; tone: 'info' | 'error' };

export const POLL_MS = 10_000;
export const EVALUATE_GAP_MS = 150;
export const MAX_NOTES_BYTES = 1024;
export const MAX_WORDS = 120;
const KEEP_TIDY_KEY = 'shinatama.team.keepTidy';

export const ROLE_NAME: Record<string, string> = { lead: 'Lead', worker: 'Worker', leadWorker: 'Lead + Worker', observer: 'Observer', peer: 'Peer' };
export const SHARE_NAME: Record<Share, string> = { split: 'Splits it', same: 'Same to all', free: 'First free' };
export const SHARE_SHORT: Record<Share, string> = { split: 'splits the work', same: 'same work to all', free: 'to whoever is free' };
export const SHARE_TIP: Record<Share, string> = {
  split: 'A different part to each, then it puts the results together.', same: 'The same task to each, then it compares the results.', free: 'Each task to whichever of them is free.',
};
export const KIND_NAME = { work: 'gives work to', observes: 'observes' } as const;
export const KIND_TIP = {
  work: (a: string, b: string) => `${b} waits for work from ${a}, does it and reports.`,
  observes: (a: string, b: string) => `${a} follows what ${b} does, to explain it to you. Never writes to it.`,
};
export const REPORTS: readonly [Report, string, string][] = [
  ['done', 'When done', 'It works on its own and reports the result.'],
  ['steps', 'At every step', 'Constant feedback: it says what it is doing as it goes.'],
  ['strict', 'Only done / blocked', 'It only executes: no questions, no messages, just done or blocked.'],
];
export const SHAPES = [
  { id: 'star', name: 'Star', tip: 'One agent in the centre gives work to all the others. Open a card first to choose the centre.' },
  { id: 'chain', name: 'Chain', tip: 'A pipeline: each agent gives work to the next.' },
  { id: 'peers', name: 'Peers', tip: 'No one is in charge: the work arrows go, everyone talks to everyone.' },
] as const;
export type ShapeId = typeof SHAPES[number]['id'];

export const MAX_NAME_LENGTH = MAX_NAME;
const clone = <T,>(value: T): T => structuredClone(value);
const bytes = (text: string) => new TextEncoder().encode(text).length;
const emptyFormation = (): Formation => ({ revision: 0, agents: [], links: [] });
/** What the arrows and agents are, without names, notes, words or places: when it changes, the engine's last answer no longer fits. */
export const structureOf = (f: Formation): string => JSON.stringify([f.agents.map(a => a.id), f.links.map(l => [l.from, l.to, l.kind, l.report])]);
/** What the agents are told about one agent: its own fields and the arrows it is on (places and the revision are not). */
const signature = (f: Formation, a: Agent) => JSON.stringify([[a.session, a.name, a.notes, a.share, a.model],
  f.links.filter(l => l.from === a.id || l.to === a.id).map(l => [l.from, l.to, l.kind, l.kind === 'work' ? l.report : null, l.words]).sort()]);
const readKeepTidy = (): boolean => { try { return localStorage.getItem(KEEP_TIDY_KEY) === '1'; } catch { return false; } };
const writeKeepTidy = (on: boolean): void => { try { localStorage.setItem(KEEP_TIDY_KEY, on ? '1' : '0'); } catch { /* a convenience only */ } };

/** The words an agent's card says about what it was told. */
export type Told = { state: 'edited' | 'told' | 'failed' | 'pending'; text: string };

export class TeamMap extends Store {
  readonly thread: TeamThread;
  snap: TeamSnapshot | null = null;
  applied: Formation | null = null;
  draft: Formation = emptyFormation();
  evaluation: Evaluation | null = null;
  evaluatedFor = '';
  evaluating = false;
  selection: Selection = null;
  /** The reasons the engine gives against trial formations of the chosen arrow (a struck-out option), by option. */
  trials: Record<string, string> = {};
  status: Status = { text: 'Reading the team…', tone: 'info' };
  keepTidy = readKeepTidy();
  applying = false;
  /** The saved teams (null until read), what a template action last said, and whether one is under way. */
  templates: Template[] | null = null;
  templatesBusy = false;
  templateNote: Status = { text: '', tone: 'info' };
  /** Why the last Apply could not open the session of an agent (kept until the next Apply or Discard). */
  readonly failures = new Map<string, string>();
  /** The width of the map on screen, for tidying and arranging (the screen tells it). */
  width = 800;
  visible = false;
  /** Goes up whenever the places were laid out again (tidy, arrange, a template, an undo): the canvas then forgets the places it kept. */
  layoutRevision = 0;

  private readonly host: TeamHost;
  private disposed = false;
  private trialsFor = '';
  private reading = false;
  private readSeq = 0;
  private notBefore = 0;
  private inFlight: Promise<void> = Promise.resolve();
  private timer: ReturnType<typeof setInterval> | undefined;
  private readonly evaluator = createEvaluator<Formation, Evaluation>(async formation => {
    const value = parseEvaluation(await teamApi.evaluate({ worksetId: this.thread.id, formation }));
    if (!value) throw new Error('the answer was not understood');
    return value;
  }, EVALUATE_GAP_MS);

  constructor(thread: TeamThread, host: TeamHost) {
    super();
    this.thread = thread;
    this.host = host;
  }

  // --- life --------------------------------------------------------------------------------------------------

  /** The map is on screen (or not): while it is, the stored team is read again every `POLL_MS` and when the window comes back. */
  setVisible(visible: boolean): void {
    if (this.visible === visible || this.disposed) return;
    this.visible = visible;
    clearInterval(this.timer);
    if (visible) {
      this.timer = setInterval(() => { if (!document.hidden) void this.refresh(); }, POLL_MS);
      void this.refresh();
    }
  }

  dispose(): void {
    this.disposed = true;
    clearInterval(this.timer);
    this.evaluator.cancel();
  }

  setWidth(width: number): void {
    if (width > 0) this.width = Math.round(width);
  }

  private ask<T>(request: (extra: Record<string, unknown>) => Promise<T>, extra: Record<string, unknown> = {}): Promise<T> {
    return request({ worksetId: this.thread.id, ...extra });
  }

  // --- lookups -----------------------------------------------------------------------------------------------

  agentOf(id: string): Agent | undefined { return this.draft.agents.find(a => a.id === id); }
  sessionOf(a: Agent): TeamSession | undefined { return a.session ? this.snap?.sessions.find(s => s.sessionId === a.session) : undefined; }
  programOf(a: Agent): string { const h = this.sessionOf(a)?.harness; return h ? this.host.harnessLabel(h) : 'Agent'; }
  sessionLabel(s: TeamSession): string { const label = s.harness ? this.host.harnessLabel(s.harness) : 'Session'; return s.title && s.title !== label ? s.title : label; }
  get enabled(): boolean { return this.snap?.enabled !== false; }
  get models(): readonly TeamModel[] { return this.snap?.models ?? []; }
  current(): Evaluation | null { return this.evaluation ?? this.snap?.evaluation ?? null; }
  /** Whether the engine's answer is about the structure on the map now. */
  fresh(): boolean { return this.current() !== null && this.evaluatedFor === structureOf(this.draft); }
  pending(): boolean { return formationChanged(this.applied, this.draft); }
  /** What the engine found, once its answer fits the draft: an agent with neither session nor model (`no_model`) and a model the
   *  Models settings no longer hold (`model_gone`) are among it. */
  issues(): Issue[] { return this.fresh() ? this.current()!.issues : []; }
  errors(): Issue[] { return this.issues().filter(i => i.level === 'error'); }
  /** The agents Apply opens a session for. */
  lacking(): number { return this.draft.agents.filter(a => !a.session).length; }
  linkIndex(from: string, to: string): number { return this.draft.links.findIndex(l => l.from === from && l.to === to); }
  selectedLink(): number { return this.selection && 'link' in this.selection ? this.linkIndex(this.selection.link.from, this.selection.link.to) : -1; }
  selectedAgent(): Agent | null { return this.selection && 'agent' in this.selection ? this.agentOf(this.selection.agent) ?? null : null; }
  nameOf(id: string): string { return this.agentOf(id)?.name ?? 'an agent'; }
  workOut(id: string): number { return this.draft.links.filter(l => l.kind === 'work' && l.from === id).length; }
  modelText(m: ModelRef): string { return `${m.model ?? 'its own model'} · via ${this.host.harnessLabel(m.program)}`; }
  /** The agents with a problem, and the arrows (by index) with one. */
  badAgents(): Set<string> { return new Set(this.errors().flatMap(i => i.agents)); }
  badLinks(): Set<number> { return new Set(this.errors().flatMap(i => i.links)); }
  /** The arrows (by index) that the applied team does not have as they are. */
  pendingLinkSet(): Set<number> { return pendingLinks(this.applied, this.draft); }

  /** What the person is told about an agent: not applied, failed, told, or waiting for its session to join the room. */
  toldState(a: Agent): Told {
    if (!a.session) return { state: 'edited', text: this.failures.get(a.id) ?? (a.model ? 'opens when applied' : 'choose a model or a session') };
    const before = this.applied?.agents.find(x => x.id === a.id);
    if (!this.applied || !before) return { state: 'edited', text: 'new · not applied' };
    if (signature(this.draft, a) !== signature(this.applied, before)) return { state: 'edited', text: 'changed · not applied' };
    const told = this.snap?.told[a.id];
    // A message the announcer could not send (Mukade refused it, or the service is not running) says why, instead of "not told yet".
    const why = this.snap?.refused[a.id] ?? (told ? undefined : this.snap?.refused.service);
    if (why) return { state: 'failed', text: `could not be told: ${why}` };
    if (told) return { state: 'told', text: `told · revision ${told.revision}` };
    return { state: 'pending', text: this.sessionOf(a)?.connection === 'in_room' ? 'not told yet' : 'told when it joins' };
  }

  roleOf(a: Agent): string {
    const view = this.current()?.roles[a.id];
    if (!view) return 'checking…';
    const name = ROLE_NAME[view.role] ?? view.role;
    return this.workOut(a.id) > 1 ? `${name} · ${SHARE_SHORT[a.share]}` : name;
  }

  /** The line of the bar: where the team stands, and in which tone. */
  barState(): { text: string; tone: 'info' | 'warning' | 'danger' | 'success' } {
    const { snap, applied, applying } = this;
    const bad = this.errors().length, dirty = this.pending(), open = this.lacking();
    const members = applied?.agents.length ?? 0;
    const waiting = (applied?.agents ?? []).filter(a => !snap?.told[a.id]).length;
    const refused = (applied?.agents ?? []).filter(a => snap?.refused[a.id]).length;
    if (!snap) return { text: 'Reading the team…', tone: 'info' };
    if (applying) return { text: 'applying…', tone: 'warning' };
    if (bad) return { text: `${bad} problem${bad > 1 ? 's' : ''} to fix before it can be applied`, tone: 'danger' };
    if (dirty) return { text: open ? `not applied: ${open} session${open > 1 ? 's' : ''} to open, nothing was sent yet` : 'not applied: nothing was sent yet', tone: 'warning' };
    if (open) return { text: `${open} agent${open > 1 ? 's' : ''} still without a session: Apply opens ${open > 1 ? 'them' : 'it'}`, tone: 'warning' };
    if (!members) return { text: 'no team', tone: 'info' };
    const revision = applied!.revision;
    if (snap.refused.service) return { text: `revision ${revision} · could not be told: ${snap.refused.service}`, tone: 'danger' };
    if (refused) return { text: `revision ${revision} · ${refused} could not be told: see the cards`, tone: 'danger' };
    if (snap.refused.notice) return { text: `revision ${revision} · the notice to the other sessions could not be sent: ${snap.refused.notice}`, tone: 'danger' };
    if (waiting) return { text: `revision ${revision} · ${waiting} told when ${waiting > 1 ? 'they join' : 'it joins'}`, tone: 'warning' };
    return { text: `revision ${revision} · everyone was told`, tone: 'success' };
  }

  /** Whether Apply can run, and why not. */
  applyBlock(): string {
    const todo = this.pending() || this.lacking() > 0;
    const ready = !this.draft.agents.length || (this.fresh() && !this.evaluating);
    if (!this.snap?.enabled) return 'The team is off for this thread';
    if (this.errors().length) return 'Fix the problems first';
    if (!todo) return 'Nothing to apply';
    if (this.applying) return 'Applying…';
    if (!ready) return 'Checking the team…';
    return '';
  }

  /** What stops the draft being kept as a template: an empty map, or an error of the checks (as the engine decides it, not the window). */
  saveBlock(): string {
    return !this.draft.agents.length ? 'Put at least one agent on the map first.' : this.errors().length ? 'Fix the problems first.' : '';
  }

  /** The sessions of the thread that are not on the map. */
  freeSessions(): TeamSession[] {
    const onMap = new Set(this.draft.agents.map(a => a.session));
    return (this.snap?.sessions ?? []).filter(s => !onMap.has(s.sessionId));
  }

  /** The sessions an agent with none can take: the ones the engine lets take part and that are not on the map. */
  takeable(): TeamSession[] {
    return this.freeSessions().filter(s => s.connection !== 'unsupported');
  }

  setStatus(text: string, tone: Status['tone'] = 'info'): void {
    this.status = { text, tone };
    this.changed();
  }

  // --- reading -----------------------------------------------------------------------------------------------

  /** Reads the stored team. What the person edited and did not apply is kept, unless `force`. A refresh after Apply waits for a read
   *  that began after it. */
  async refresh(force = false): Promise<void> {
    while (this.reading && !this.disposed) {
      // A read under way is enough unless it began before the last Apply; either way the caller waits for it.
      const fresh = this.readSeq >= this.notBefore;
      await this.inFlight;
      if (fresh) return;
    }
    if (this.disposed) return;
    this.reading = true;
    const seq = ++this.readSeq;
    this.inFlight = (async () => {
      try {
        const value = parseTeam(await this.ask(extra => teamApi.get(extra)));
        if (this.disposed || seq < this.notBefore) return;
        if (!value) { this.status = { text: 'The team could not be read.', tone: 'error' }; return; }
        this.snap = value;
        if (!value.enabled) { this.applied = null; this.draft = emptyFormation(); this.evaluation = null; this.selection = null; return; }
        if (this.status.text === 'Reading the team…') this.status = { text: '', tone: 'info' };
        const stored = value.formation;
        if (force || !this.pending()) {
          if (force) this.layoutRevision += 1;
          const before = new Map(this.draft.agents.map(a => [a.id, a] as const));
          this.applied = stored;
          this.draft = stored ? clone(stored) : emptyFormation();
          // A card the person moved keeps its place (it is stored when its move ends, so the two agree again soon).
          if (!force) for (const a of this.draft.agents) { const mine = before.get(a.id); if (mine) { a.x = mine.x; a.y = mine.y; } }
          if (value.evaluation) { this.evaluation = value.evaluation; this.evaluatedFor = structureOf(this.draft); this.evaluating = false; }
          else this.request();
          this.dropStaleSelection();
        } else if (stored && stored.revision !== this.applied?.revision) {
          this.status = { text: `The team was changed elsewhere (revision ${stored.revision}). Discard reads it again.`, tone: 'error' };
        }
      } catch (error) {
        if (!this.disposed && seq >= this.notBefore) this.status = { text: errorText(error), tone: 'error' };
      } finally { this.reading = false; this.changed(); }
    })();
    await this.inFlight;
  }

  private dropStaleSelection(): void {
    if (this.selection && ('agent' in this.selection ? !this.agentOf(this.selection.agent) : this.selectedLink() < 0)) this.selection = null;
  }

  /** Asks the engine about the draft, at most once per EVALUATE_GAP_MS; the latest request is the one answered. */
  request(): void {
    if (!this.draft.agents.length) { this.evaluation = null; this.evaluatedFor = ''; this.evaluating = false; this.changed(); return; }
    if (!this.pending() && this.snap?.evaluation) { this.evaluation = this.snap.evaluation; this.evaluatedFor = structureOf(this.draft); this.evaluating = false; return; }
    const asked = clone(this.draft);
    const key = structureOf(asked);
    this.evaluating = true;
    void this.evaluator.request('draft', asked).then(result => {
      // null: a newer request took its place and its answer comes; a different shape: the draft moved on.
      if (this.disposed || result === null || key !== structureOf(this.draft)) return;
      this.evaluating = false;
      this.evaluation = result;
      this.evaluatedFor = key;
      if (this.status.tone === 'error' && this.status.text.startsWith('The team could not be checked')) this.status = { text: '', tone: 'info' };
      this.changed();
    }, error => {
      if (this.disposed) return;
      this.evaluating = false;
      this.status = { text: `The team could not be checked: ${errorText(error)}`, tone: 'error' };
      this.changed();
    });
    this.changed();
  }

  /** The reasons the engine gives against trial formations of the chosen arrow (a struck-out option). */
  requestTrials(): void {
    const index = this.selectedLink();
    const key = `${structureOf(this.draft)}|${index}`;
    if (key === this.trialsFor) return;
    this.trialsFor = key;
    this.trials = {};
    if (index < 0) return;
    const l = this.draft.links[index];
    const trial = (name: string, change: Partial<Link>) => {
      const formation: Formation = { ...clone(this.draft), links: this.draft.links.map((x, i) => i === index ? { ...x, ...change } : { ...x }) };
      void this.evaluator.request(`trial:${name}`, formation).then(result => {
        if (this.disposed || this.trialsFor !== key || !result) return;
        const why = result.issues.find(i => i.level === 'error' && i.links.includes(index));
        this.trials = { ...this.trials, [name]: why ? why.text : '' };
        this.changed();
      }, () => { /* no reason known: the option stays open and Apply is refused by the engine if it cannot work */ });
    };
    trial(l.kind === 'work' ? 'observes' : 'work', { kind: l.kind === 'work' ? 'observes' : 'work', report: 'done' });
    if (l.kind === 'work' && l.report !== 'strict') trial('strict', { report: 'strict' });
  }

  // --- choosing ----------------------------------------------------------------------------------------------

  select(selection: Selection): void {
    this.selection = selection;
    this.requestTrials();
    this.changed();
  }

  selectAgent(id: string | null): void {
    this.select(id && this.agentOf(id) ? { agent: id } : null);
  }

  selectLink(from: string, to: string): void {
    this.select(this.linkIndex(from, to) >= 0 ? { link: { from, to } } : null);
  }

  /** A problem of the list: the arrow or the agent it is about is chosen. */
  selectIssue(issue: Issue): void {
    const l = this.draft.links[issue.links[0]];
    this.select(l ? { link: { from: l.from, to: l.to } } : issue.agents[0] && this.agentOf(issue.agents[0]) ? { agent: issue.agents[0] } : null);
  }

  // --- editing -----------------------------------------------------------------------------------------------

  /** Every change of the draft goes through here: the engine is asked again, the cards may be tidied, the view is drawn. */
  private edit(next: Formation, options: { tidy?: boolean } = {}): void {
    if (this.applying || next === this.draft) return;
    const before = this.draft;
    this.draft = next;
    this.dropStaleSelection();
    if (this.keepTidy && options.tidy !== false) this.placeByTidy();
    this.request();
    this.requestTrials();
    const moved = this.draft.agents.filter(a => { const was = before.agents.find(x => x.id === a.id); return !!was && (was.x !== a.x || was.y !== a.y); }).map(a => a.id);
    if (moved.length) this.storePlaces(moved);
    this.changed();
  }

  private placeByTidy(): void {
    this.layoutRevision += 1;
    const places = tidy(this.draft, this.current()?.roles ?? {}, this.width);
    this.draft = { ...this.draft, agents: this.draft.agents.map(a => places[a.id] ? { ...a, x: Math.round(places[a.id].x), y: Math.round(places[a.id].y) } : a) };
  }

  /** Places are stored without a revision when nothing else is pending; otherwise they travel with Apply. */
  private storePlaces(ids: readonly string[]): void {
    if (!this.applied || this.pending() || !ids.length) return;
    const places: Record<string, [number, number]> = {};
    for (const id of ids) { const a = this.agentOf(id); if (a) places[id] = [Math.round(a.x), Math.round(a.y)]; }
    if (!Object.keys(places).length) return;
    const stored = this.applied;
    this.applied = { ...stored, agents: stored.agents.map(a => places[a.id] ? { ...a, x: places[a.id][0], y: places[a.id][1] } : a) };
    void this.ask(extra => teamApi.place({ ...extra, places })).catch(error => {
      if (!this.disposed) this.setStatus(`The places could not be stored: ${errorText(error)}`, 'error');
    });
  }

  updateAgent(id: string, change: Partial<Agent>): void {
    this.edit({ ...this.draft, agents: this.draft.agents.map(a => a.id === id ? { ...a, ...change } : a) }, { tidy: false });
  }

  updateLink(index: number, change: Partial<Link>): void {
    this.edit({ ...this.draft, links: this.draft.links.map((l, i) => i === index ? { ...l, ...change } : l) }, { tidy: 'kind' in change });
  }

  /** A card moved on the map: its place is the person's word (stored when nothing else is pending). */
  moveAgent(id: string, place: Point): void {
    const a = this.agentOf(id);
    if (!a || this.applying) return;
    const x = Math.max(0, Math.round(place.x)), y = Math.max(0, Math.round(place.y));
    if (a.x === x && a.y === y) return;
    this.draft = { ...this.draft, agents: this.draft.agents.map(one => one.id === id ? { ...one, x, y } : one) };
    this.storePlaces([id]);
    this.changed();
  }

  setName(id: string, text: string): void {
    const clean = plainText(text);
    if (clean.trim()) this.updateAgent(id, { name: Array.from(clean).slice(0, MAX_NAME).join('') });
  }

  setNotes(id: string, text: string): void {
    let clean = plainText(text);
    while (bytes(clean) > MAX_NOTES_BYTES) clean = clean.slice(0, -1);
    this.updateAgent(id, { notes: clean });
  }

  setWords(index: number, text: string): void {
    if (index >= 0) this.updateLink(index, { words: plainText(text).slice(0, MAX_WORDS) });
  }

  setModel(id: string, key: string): void {
    const chosen = this.models.find(m => modelKey(m) === key);
    this.updateAgent(id, { model: chosen ? { program: chosen.program, connection: chosen.connection, model: chosen.model } : null });
  }

  /** The kind of the chosen arrow. */
  setKind(from: string, to: string, kind: Link['kind']): void {
    const at = this.linkIndex(from, to);
    if (at >= 0 && this.draft.links[at].kind !== kind) this.updateLink(at, { kind, report: 'done' });
  }

  setReport(from: string, to: string, report: Report): void {
    const at = this.linkIndex(from, to);
    if (at >= 0) this.updateLink(at, { report });
  }

  /** The arrow goes the other way (not when one already goes that way). */
  reverse(from: string, to: string): void {
    const at = this.linkIndex(from, to);
    if (at < 0 || this.draft.links.some(x => x.from === to && x.to === from)) return;
    this.selection = { link: { from: to, to: from } };
    this.edit({ ...this.draft, links: this.draft.links.map((x, i) => i === at ? { ...x, from: x.to, to: x.from } : x) });
    this.requestTrials();
  }

  /** The arrow is taken off; the notice offers it back. */
  deleteLink(from: string, to: string): void {
    const at = this.linkIndex(from, to);
    if (at < 0) return;
    const before = this.draft;
    this.selection = null;
    this.edit({ ...this.draft, links: this.draft.links.filter((_, i) => i !== at) });
    this.host.say(`${this.nameOf(from)} no longer ${KIND_NAME[before.links[at].kind]} ${this.nameOf(to)}.`, { undo: () => this.restore(before) });
  }

  private restore(draft: Formation): void {
    this.layoutRevision += 1;
    this.selection = null;
    this.edit(draft, { tidy: false });
  }

  /** The session joins the map as a new agent at `at`. */
  addSessionAt(session: TeamSession, at: Point = this.freeSpot()): void {
    const label = session.harness ? this.host.harnessLabel(session.harness) : 'Session';
    const base = session.title && session.title !== label ? session.title : label;
    const taken = new Set(this.draft.agents.map(a => a.name));
    let name = base;
    for (let n = 2; taken.has(name) && n < 100; n += 1) name = `${base} ${n}`;
    const next = addSession(this.draft, session.sessionId, name, at);
    if (next === this.draft) {
      this.setStatus(this.draft.agents.length >= MAX_AGENTS ? `The map holds ${MAX_AGENTS} agents at most.` : 'That session is already on the map.', 'error');
      return;
    }
    this.status = { text: '', tone: 'info' };
    this.edit(next);
    const added = next.agents.find(a => a.session === session.sessionId);
    if (added) this.select({ agent: added.id });
  }

  freeSpot(): Point {
    for (let n = 0; n < 200; n += 1) {
      const p = { x: 30 + (n % 3) * (CARD.width + 36), y: 40 + Math.floor(n / 3) * (CARD.height + 30) };
      if (!this.draft.agents.some(a => Math.abs(a.x - p.x) < CARD.width && Math.abs(a.y - p.y) < CARD.height)) return p;
    }
    return { x: 30, y: 40 };
  }

  /** `from` gives work to `to`. */
  connectAgents(from: string, to: string): void {
    const next = connect(this.draft, from, to);
    if (next === this.draft) {
      this.setStatus(from === to ? 'An agent cannot give work to itself.' : `The map holds ${MAX_LINKS} arrows at most.`, 'error');
      return;
    }
    this.status = { text: '', tone: 'info' };
    this.selection = { link: { from, to } };
    this.edit(next);
    this.requestTrials();
  }

  /** An agent with no session, at a free place; it is filled by choosing a session of the thread for it. */
  addEmptyAgent(): void {
    const next = addAgent(this.draft, 'Agent', this.freeSpot());
    if (next === this.draft) { this.setStatus(`The map holds ${MAX_AGENTS} agents at most.`, 'error'); return; }
    this.status = { text: '', tone: 'info' };
    this.edit(next);
    const added = next.agents.at(-1);
    if (added) this.select({ agent: added.id });
  }

  /** The session takes the place of an agent that has none: its name and arrows stay. */
  fillWith(sessionId: string, agent: string): void {
    const session = this.snap?.sessions.find(x => x.sessionId === sessionId);
    if (!session) return;
    if (session.connection === 'unsupported') { this.setStatus('Shinatama gives this agent no Mukade tools: it cannot take part in a team.', 'error'); return; }
    const next = fillAgent(this.draft, agent, session.sessionId);
    if (next === this.draft) {
      this.setStatus(this.draft.agents.some(a => a.session === session.sessionId) ? 'That session is already on the map.' : 'That agent already has a session.', 'error');
      return;
    }
    this.status = { text: '', tone: 'info' };
    this.edit(next);
  }

  /** The agent leaves the map (its session stays open); the notice offers it back. */
  takeOff(id: string): void {
    const before = this.draft;
    const name = this.nameOf(id);
    if (this.selection && 'agent' in this.selection && this.selection.agent === id) this.selection = null;
    this.edit(removeAgent(this.draft, id));
    if (this.draft !== before) this.host.say(`${name} was taken off the map.`, { undo: () => this.restore(before) });
  }

  /** Several things left the map with Delete in the canvas. */
  takeOffMany(ids: readonly string[]): void {
    if (ids.length === 1) { this.takeOff(ids[0]); return; }
    const before = this.draft;
    let next = this.draft;
    for (const id of ids) next = removeAgent(next, id);
    this.selection = null;
    this.edit(next);
    if (this.draft !== before) this.host.say(`${ids.length} agents were taken off the map.`, { undo: () => this.restore(before) });
  }

  tidyNow(): void {
    this.placeByTidy();
    this.storePlaces(this.draft.agents.map(a => a.id));
    this.changed();
  }

  setKeepTidy(on: boolean): void {
    this.keepTidy = on;
    writeKeepTidy(on);
    if (on) this.tidyNow(); else this.changed();
  }

  arrangeAs(shape: ShapeId): void {
    if (this.applying || this.draft.agents.length < 2) return;
    const hub = this.selection && 'agent' in this.selection ? this.selection.agent : null;
    const before = this.draft;
    this.layoutRevision += 1;
    this.edit(arrange(this.draft, shape, hub, this.width), { tidy: false });
    this.host.say(`The team is arranged as ${shape}.`, { undo: () => this.restore(before) });
  }

  // --- apply -------------------------------------------------------------------------------------------------

  async apply(): Promise<void> {
    if (this.applying || !(this.pending() || this.lacking()) || this.errors().length || !this.fresh()) return;
    const sent = clone(this.draft);
    this.applying = true;
    this.failures.clear();
    this.status = { text: '', tone: 'info' };
    this.changed();
    try {
      const result = await this.ask(extra => teamApi.apply({ ...extra, formation: sent, expectedRevision: this.applied?.revision ?? 0 })) as
        { formation?: unknown; failed?: { agent?: unknown; reason?: unknown } | null } | null;
      const stored = parseFormation(result?.formation);
      if (!stored) throw 'The answer to Apply was not understood. The team may have been stored: Discard reads it again.';
      this.notBefore = this.readSeq + 1; // a read already under way began before this Apply: its answer is stale
      this.applied = stored;
      this.draft = clone(stored);
      this.selection = null;
      this.trialsFor = '';
      this.evaluation = null;
      this.evaluatedFor = '';
      if (this.snap) this.snap = { ...this.snap, evaluation: null };
      const failed = result?.failed;
      const who = typeof failed?.agent === 'string' ? stored.agents.find(a => a.id === failed.agent) : undefined;
      if (who) {
        const reason = typeof failed?.reason === 'string' && failed.reason ? failed.reason : 'It could not be opened.';
        this.failures.set(who.id, reason);
        this.status = { text: `${who.name} has no session: ${reason} What was opened is kept; Apply tries the rest again.`, tone: 'error' };
        this.host.say(`${who.name} has no session: ${reason}`, { error: true });
      } else {
        this.status = { text: `Applied: revision ${stored.revision}. Each session is told its place when it is in the room.`, tone: 'info' };
        this.host.say(`Team applied: revision ${stored.revision}.`);
      }
      this.applying = false;
      this.request();
      await this.refresh();
    } catch (error) {
      this.applying = false;
      this.setStatus(errorText(error), 'error');
      this.host.say(errorText(error), { error: true });
    }
    this.changed();
  }

  /** What was not applied is thrown away and the stored team is read again. */
  async discard(): Promise<void> {
    this.selection = null;
    this.trialsFor = '';
    this.trials = {};
    this.evaluator.cancel();
    this.status = { text: '', tone: 'info' };
    this.failures.clear();
    await this.refresh(true);
  }

  // --- templates ---------------------------------------------------------------------------------------------

  private sayTemplate(text: string, tone: Status['tone'] = 'info'): void {
    this.templateNote = { text, tone };
  }

  async loadTemplates(): Promise<void> {
    try {
      const list = parseTemplates(await teamApi.templates());
      if (this.disposed) return;
      if (!list) this.sayTemplate('The templates could not be read.', 'error');
      else { this.templates = list; if (this.templateNote.tone === 'error') this.sayTemplate(''); }
    } catch (error) { if (!this.disposed) this.sayTemplate(`The templates could not be read: ${errorText(error)}`, 'error'); }
    this.changed();
  }

  /** Replaces the draft with the template's agents, none with a session. Nothing is lost for good: the notice offers the old map back,
   *  and Discard reads the applied one. */
  useTemplate(t: Template): void {
    if (this.applying) return;
    const before = this.draft;
    this.layoutRevision += 1;
    this.selection = null;
    this.trialsFor = '';
    this.status = { text: '', tone: 'info' };
    this.edit(fromTemplate(t, this.width, newAgentId, this.models), { tidy: false });
    this.host.say(`The template “${t.name}” is on the map. No agent has a session yet.`, before.agents.length ? { undo: () => this.restore(before) } : undefined);
  }

  async saveTemplate(rawName: string): Promise<boolean> {
    const name = plainText(rawName).trim();
    if (!name || this.templatesBusy || this.saveBlock()) return false;
    this.templatesBusy = true;
    this.sayTemplate('');
    this.changed();
    let saved = false;
    try {
      const list = parseTemplates(await teamApi.templateSave({ name, formation: clone(this.draft) }));
      if (!list) throw 'The answer to Save was not understood. Open the templates again to read them.';
      this.templates = list;
      this.sayTemplate(`Saved “${name}”.`);
      saved = true;
    } catch (error) { this.sayTemplate(errorText(error), 'error'); }
    finally { this.templatesBusy = false; this.changed(); }
    return saved;
  }

  /** Asks first: a template deleted is gone (the teams made from it are not touched). */
  async deleteTemplate(t: Template): Promise<void> {
    if (!await this.host.ask({
      title: `Delete the template “${t.name}”?`, description: 'The template is removed for every thread. The teams made from it are not touched. This cannot be undone.',
      confirm: `Delete “${t.name}”`,
    })) return;
    this.sayTemplate('');
    try {
      const list = parseTemplates(await teamApi.templateDelete({ id: t.id }));
      if (!list) throw 'The answer to Delete was not understood. Open the templates again to read them.';
      this.templates = list;
      this.sayTemplate(`Deleted “${t.name}”.`);
    } catch (error) { this.sayTemplate(errorText(error), 'error'); }
    this.changed();
  }

  /** The sessions' own harness name, for the lists. */
  harnessLabel(harness: string | null): string { return harness ? this.host.harnessLabel(harness) : 'Session'; }
  focusSession(sessionId: string): void { this.host.focusSession(sessionId); }
  title(): string { return this.host.title(this.thread.id); }
  holds(m: ModelRef): boolean { return holdsModel(this.models, m); }
}

export const modelKey = (m: ModelRef): string => JSON.stringify([m.program, m.connection, m.model]);

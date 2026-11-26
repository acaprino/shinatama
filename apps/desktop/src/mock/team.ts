// The answers of the browser for the team map (see mock/index.ts): a thread with four sessions, two saved teams, and a small stand-in for
// the engine that infers the roles from the arrows, says what each agent is told and refuses an agent with neither a session nor a model.
import type { Answers } from './index.ts';

type Agent = { id: string; session: string | null; model: { program: string; connection: string | null; model: string | null } | null; name: string; notes: string; share: string; x: number; y: number };
type Link = { from: string; to: string; kind: 'work' | 'observes'; report: string; words: string };
type Formation = { revision: number; agents: Agent[]; links: Link[] };
type Template = { id: string; name: string; agents: Agent[]; links: Link[] };

const SESSIONS = [
  { sessionId: 't_planneraaaaa', harness: 'claude_code', title: 'Plan the billing rewrite', state: 'running', memberName: 'planner', connection: 'in_room', wakes: 1 },
  { sessionId: 't_coderaaaaaaa', harness: 'opencode', title: null, state: 'running', memberName: 'coder', connection: 'launched', wakes: 0 },
  { sessionId: 't_reviewaaaaaa', harness: 'codex', title: 'Review the diff', state: 'ready', memberName: null, connection: 'not_connected', wakes: null },
  { sessionId: 't_hermesaaaaaa', harness: 'pi', title: null, state: 'ready', memberName: null, connection: 'unsupported', wakes: null },
];
const MODELS = [
  { program: 'opencode', connection: 'acme', model: 'qwen3-coder-30b', label: 'qwen3-coder-30b · Acme gateway' },
  { program: 'pi', connection: 'acme', model: 'gpt-5.5-mini', label: 'gpt-5.5-mini · Acme gateway' },
];
const agent = (id: string, name: string, session: string | null, x: number, y: number): Agent => ({ id: `a_${id.padEnd(12, 'a')}`, session, model: null, name, notes: '', share: 'split', x, y });

const fresh = (): { formation: Formation | null; templates: Template[] } => ({
  formation: {
    revision: 1,
    agents: [agent('planner', 'planner', 't_planneraaaaa', 280, 40), agent('coder', 'coder', 't_coderaaaaaaa', 120, 230)],
    links: [{ from: `a_${'planner'.padEnd(12, 'a')}`, to: `a_${'coder'.padEnd(12, 'a')}`, kind: 'work', report: 'done', words: '' }],
  },
  templates: [
    { id: 'tpl_starfouraaaa', name: 'Star of four', agents: [agent('hub', 'hub', null, 280, 40), agent('one', 'one', null, 60, 230), agent('two', 'two', null, 280, 230), agent('three', 'three', null, 500, 230)],
      links: ['one', 'two', 'three'].map((to): Link => ({ from: `a_${'hub'.padEnd(12, 'a')}`, to: `a_${to.padEnd(12, 'a')}`, kind: 'work', report: 'done', words: '' })) },
  ],
});
let { formation, templates } = fresh();

/** Puts the stand-in back as it was (a test starts from it; the browser has it at each load). */
export function resetTeamMock(): void {
  ({ formation, templates } = fresh());
}

function evaluate(f: Formation) {
  const roles: Record<string, { role: string; detail: string; mukade: string; handOut: string | null }> = {};
  for (const a of f.agents) {
    const gives = f.links.filter(l => l.kind === 'work' && l.from === a.id), takes = f.links.some(l => l.kind === 'work' && l.to === a.id);
    const observes = f.links.some(l => l.kind === 'observes' && l.from === a.id);
    const role = observes ? 'observer' : gives.length && takes ? 'leadWorker' : gives.length ? 'lead' : takes ? 'worker' : 'peer';
    roles[a.id] = { role, detail: role === 'lead' ? 'splits the work' : role === 'worker' ? 'does the work it is given' : role === 'observer' ? 'follows another agent' : 'works with the others', mukade: role === 'lead' ? 'lead' : role === 'worker' ? 'worker' : role === 'observer' ? 'observer' : 'peer', handOut: null };
  }
  const issues = f.agents.filter(a => !a.session && !a.model).map(a => ({ level: 'error' as const, code: 'no_model', text: `${a.name} has neither a session nor a model.`, agents: [a.id], links: [] as number[] }));
  const name = (id: string) => f.agents.find(a => a.id === id)?.name ?? 'agent';
  const sentences: [string, string][] = f.links.map(l => l.kind === 'work'
    ? [`You give work to ${name(l.to)}.`, `${name(l.from)} gives you work: do it and report.`]
    : [`You observe ${name(l.to)}: follow what it does. Do not write to it.`, `${name(l.from)} observes you.`]);
  // A work arrow that is also an observing one the other way round is refused, as the engine refuses a loop.
  f.links.forEach((l, i) => {
    if (f.links.some(o => o.from === l.to && o.to === l.from && o.kind === 'work' && l.kind === 'work')) {
      issues.push({ level: 'error', code: 'loop', text: `${name(l.from)} and ${name(l.to)} give work to each other.`, agents: [l.from, l.to], links: [i] });
    }
  });
  return { roles, issues, sentences };
}

const snapshot = () => ({
  enabled: true, formation, told: formation ? Object.fromEntries(formation.agents.filter(a => a.session).map(a => [a.id, { revision: formation!.revision, member: a.name }])) : {},
  refused: {}, sessions: SESSIONS, models: MODELS, evaluation: formation ? evaluate(formation) : null,
});

export const answers: Answers = {
  team_get: () => snapshot(),
  team_evaluate: args => evaluate((args?.request as { formation: Formation }).formation),
  team_apply: args => {
    const sent = (args?.request as { formation: Formation }).formation;
    // An agent with only a model gets a session when applied.
    const agents = sent.agents.map((a, i) => a.session ? a : { ...a, session: `t_open${i}`.padEnd(14, 'a') });
    formation = { ...sent, agents, revision: (formation?.revision ?? 0) + 1 };
    return { formation, failed: null };
  },
  team_place: args => {
    const places = (args?.request as { places: Record<string, [number, number]> }).places;
    if (formation) formation = { ...formation, agents: formation.agents.map(a => places[a.id] ? { ...a, x: places[a.id][0], y: places[a.id][1] } : a) };
    return null;
  },
  team_templates: () => templates,
  team_template_save: args => {
    const { name, formation: f } = (args?.request as { name: string; formation: Formation });
    templates = [...templates, { id: `tpl_${name.replace(/\W/g, '').padEnd(12, 'a').slice(0, 12)}`, name, agents: f.agents, links: f.links }];
    return templates;
  },
  team_template_delete: args => { templates = templates.filter(t => t.id !== (args?.request as { id: string }).id); return templates; },
};

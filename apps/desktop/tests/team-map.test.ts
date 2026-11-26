// The team map of a thread, on the stand-in for the engine of src/mock/team.ts: what is read, what an edit asks, what Apply sends, how a
// change is taken back, and what the bar says. A table of answers takes the place of Rust (api/core.ts `overrideInvoke`).
import test, { afterEach } from 'node:test';
import assert from 'node:assert/strict';
import { overrideInvoke } from '../src/api/core.ts';
import { answers, resetTeamMock } from '../src/mock/team.ts';
import { TeamMap, type TeamHost } from '../src/controllers/team-map.ts';

afterEach(() => overrideInvoke(null));

function fixture(extra: Record<string, unknown> = {}) {
  resetTeamMock();
  const calls: { command: string; args: Record<string, unknown> | undefined }[] = [];
  overrideInvoke(async (command, args) => {
    calls.push({ command, args });
    const one = (extra[command] ?? (answers as Record<string, unknown>)[command]) as unknown;
    if (one === undefined) throw `no answer for ${command}`;
    return typeof one === 'function' ? (one as (a: Record<string, unknown> | undefined) => unknown)(args) : one;
  });
  const said: { text: string; error: boolean; undo?: () => void }[] = [];
  const asked: string[] = [];
  const host: TeamHost = {
    title: () => 'Billing rewrite', harnessLabel: h => h, focusSession: () => {},
    say: (text, say) => { said.push({ text, error: say?.error === true, ...(say?.undo ? { undo: say.undo } : {}) }); },
    ask: async question => { asked.push(question.title); return true; },
  };
  const map = new TeamMap({ id: 'w_aaaaaaaaaaaa' }, host);
  return { map, calls, said, asked };
}
const settle = (ms = 400) => new Promise(resolve => setTimeout(resolve, ms));

test('reading the team gives the draft, what was applied and the engine\'s answer, and a thread with no team says so', async () => {
  const { map } = fixture();
  await map.refresh();
  assert.equal(map.draft.agents.length, 2);
  assert.equal(map.applied?.revision, 1);
  assert.equal(map.pending(), false);
  assert.equal(map.fresh(), true);
  assert.deepEqual(map.draft.agents.map(a => map.roleOf(a)), ['Lead', 'Worker']);
  assert.equal(map.barState().tone, 'success');
  const off = fixture({ team_get: { enabled: false } });
  await off.map.refresh();
  assert.equal(off.map.enabled, false);
  assert.equal(off.map.draft.agents.length, 0);
});

test('an edit is a draft that is not applied: the engine is asked about it, Apply is blocked while it says an error, and Discard brings back what was applied', async () => {
  const { map, calls } = fixture();
  await map.refresh();
  map.addEmptyAgent();
  assert.equal(map.draft.agents.length, 3);
  assert.equal(map.pending(), true);
  assert.match(map.barState().text, /not applied: 1 session to open/);
  await settle();
  // The new agent has neither a session nor a model: the engine refuses it, and that is why Apply does not run.
  assert.equal(map.errors().length, 1);
  assert.match(map.applyBlock(), /Fix the problems/);
  assert.ok(calls.some(call => call.command === 'team_evaluate'));
  const agent = map.draft.agents.at(-1)!;
  map.setModel(agent.id, JSON.stringify(['opencode', 'acme', 'qwen3-coder-30b']));
  await settle();
  assert.equal(map.errors().length, 0);
  assert.equal(map.applyBlock(), '');
  await map.discard();
  assert.equal(map.draft.agents.length, 2);
  assert.equal(map.pending(), false);
});

test('Apply sends the draft with the revision it was read at, opens the session an agent lacks and reads the team again', async () => {
  const { map, calls, said } = fixture();
  await map.refresh();
  map.addEmptyAgent();
  const id = map.draft.agents.at(-1)!.id;
  map.setModel(id, JSON.stringify(['opencode', 'acme', 'qwen3-coder-30b']));
  map.connectAgents(map.draft.agents[0].id, id);
  await settle();
  await map.apply();
  const sent = calls.find(call => call.command === 'team_apply')!.args!.request as { expectedRevision: number; formation: { agents: unknown[] } };
  assert.equal(sent.expectedRevision, 1);
  assert.equal(sent.formation.agents.length, 3);
  assert.equal(map.applied?.revision, 2);
  assert.equal(map.draft.agents.every(a => a.session), true);
  assert.equal(map.pending(), false);
  assert.match(said.at(-1)!.text, /revision 2/);
});

test('an agent taken off the map and an arrow deleted come back with Undo', async () => {
  const { map, said } = fixture();
  await map.refresh();
  const [planner, coder] = map.draft.agents;
  map.deleteLink(planner.id, coder.id);
  assert.equal(map.draft.links.length, 0);
  assert.match(said.at(-1)!.text, /no longer gives work to/);
  said.at(-1)!.undo!();
  assert.equal(map.draft.links.length, 1);
  map.takeOff(coder.id);
  assert.equal(map.draft.agents.length, 1);
  assert.equal(map.draft.links.length, 0, 'an arrow goes with its agent');
  said.at(-1)!.undo!();
  assert.equal(map.draft.agents.length, 2);
  assert.equal(map.draft.links.length, 1);
});

test('the kind and the report of an arrow, its words and its direction change the draft; the reverse of an arrow that is there is refused', async () => {
  const { map } = fixture();
  await map.refresh();
  const [planner, coder] = map.draft.agents;
  map.setReport(planner.id, coder.id, 'steps');
  assert.equal(map.draft.links[0].report, 'steps');
  map.setWords(0, 'be careful @all');
  assert.equal(map.draft.links[0].words, 'be careful all', 'the engine refuses @, so it is not typed');
  map.setKind(planner.id, coder.id, 'observes');
  assert.equal(map.draft.links[0].kind, 'observes');
  assert.equal(map.draft.links[0].report, 'done');
  map.reverse(planner.id, coder.id);
  assert.deepEqual([map.draft.links[0].from, map.draft.links[0].to], [coder.id, planner.id]);
  assert.deepEqual(map.selection, { link: { from: coder.id, to: planner.id } });
});

test('a session of the thread joins the map once; one the engine gives no tools cannot fill an agent; a full map says so', async () => {
  const { map } = fixture();
  await map.refresh();
  const free = map.freeSessions();
  assert.equal(free.length, 2);
  assert.equal(map.takeable().length, 1, 'the unsupported one cannot take part');
  map.addSessionAt(free[0]);
  assert.equal(map.draft.agents.length, 3);
  assert.equal(map.freeSessions().length, 1);
  map.addEmptyAgent();
  map.fillWith('t_hermesaaaaaa', map.draft.agents.at(-1)!.id);
  assert.match(map.status.text, /no Mukade tools/);
  assert.equal(map.status.tone, 'error');
});

test('a card that is moved is stored without a revision when nothing else is pending, and travels with Apply otherwise', async () => {
  const { map, calls } = fixture();
  await map.refresh();
  map.moveAgent(map.draft.agents[0].id, { x: 10.4, y: 20.6 });
  assert.deepEqual([map.draft.agents[0].x, map.draft.agents[0].y], [10, 21]);
  assert.equal(calls.filter(call => call.command === 'team_place').length, 1);
  map.addEmptyAgent();
  map.moveAgent(map.draft.agents[0].id, { x: 50, y: 60 });
  assert.equal(calls.filter(call => call.command === 'team_place').length, 1, 'pending: the place waits for Apply');
});

test('Tidy up and Arrange as place the cards again and take the old places back with Undo', async () => {
  const { map, said } = fixture();
  await map.refresh();
  const before = map.layoutRevision;
  map.arrangeAs('chain');
  assert.ok(map.layoutRevision > before, 'the canvas is told to forget the places it kept');
  assert.match(said.at(-1)!.text, /arranged as chain/);
  const places = map.draft.agents.map(a => [a.x, a.y]);
  said.at(-1)!.undo!();
  assert.notDeepEqual(map.draft.agents.map(a => [a.x, a.y]), places);
});

test('templates: read, used on the map (with Undo), saved with a name, and deleted only after the person says yes', async () => {
  const { map, said, asked } = fixture();
  await map.refresh();
  await map.loadTemplates();
  assert.equal(map.templates?.length, 1);
  assert.equal(map.saveBlock(), '');
  assert.equal(await map.saveTemplate('My team'), true);
  assert.equal(map.templates?.length, 2);
  map.useTemplate(map.templates![0]);
  assert.equal(map.draft.agents.length, 4);
  assert.equal(map.draft.agents.every(a => a.session === null), true);
  assert.match(said.at(-1)!.text, /Star of four/);
  said.at(-1)!.undo!();
  assert.equal(map.draft.agents.length, 2);
  await map.deleteTemplate(map.templates![1]);
  assert.deepEqual(asked, ['Delete the template “My team”?']);
  assert.equal(map.templates?.length, 1);
});

test('the toldState of a card says whether its session was told, when it joins, or why it could not be told', async () => {
  const { map } = fixture({ team_get: () => ({
    enabled: true, formation: { revision: 4, agents: [
      { id: `a_${'one'.padEnd(12, 'a')}`, session: 't_planneraaaaa', model: null, name: 'one', notes: '', share: 'split', x: 0, y: 0 },
      { id: `a_${'two'.padEnd(12, 'a')}`, session: 't_coderaaaaaaa', model: null, name: 'two', notes: '', share: 'split', x: 300, y: 0 },
    ], links: [] },
    told: { [`a_${'one'.padEnd(12, 'a')}`]: { revision: 4, member: 'one' } }, refused: { [`a_${'two'.padEnd(12, 'a')}`]: 'Mukade is stopped' },
    sessions: answers.team_get ? (answers.team_get as () => { sessions: unknown[] })().sessions : [], models: [], evaluation: null,
  }) });
  await map.refresh();
  const [one, two] = map.draft.agents;
  assert.deepEqual(map.toldState(one), { state: 'told', text: 'told · revision 4' });
  assert.equal(map.toldState(two).state, 'failed');
  assert.match(map.toldState(two).text, /Mukade is stopped/);
});

test('the canvas chooses an arrow by its id, one choice at a time: an arrow gives up an agent, an agent gives up an arrow, and the nulls the canvas says along do not undo the choice', async () => {
  const { map } = fixture();
  await map.refresh();
  assert.ok(map.draft.links.length > 0 && map.draft.agents.length > 1);
  const link = map.draft.links[0];
  const id = TeamMap.arrowId(link.from, link.to);
  // Nothing is chosen: no arrow is named.
  assert.equal(map.selectedArrowId(), null);
  // An agent is chosen, then an arrow: the canvas says the arrow, then gives the agent up (null) and the arrow stays.
  map.chooseAgent(link.from);
  assert.equal(map.selectedAgent()?.id, link.from);
  map.chooseArrow(id);
  map.chooseAgent(null);
  assert.equal(map.selectedArrowId(), id);
  assert.equal(map.selectedAgent(), null);
  // An agent is chosen: the canvas says it, then gives the arrow up (null); the agent stays.
  map.chooseAgent(link.to);
  map.chooseArrow(null);
  assert.equal(map.selectedAgent()?.id, link.to);
  assert.equal(map.selectedArrowId(), null);
  // A press on the ground says both nulls: nothing is chosen.
  map.chooseArrow(id);
  map.chooseAgent(null);
  map.chooseArrow(null);
  assert.equal(map.selection, null);
  // An arrow that is not on the map is not chosen.
  map.chooseArrow('nobody>nowhere');
  assert.equal(map.selection, null);
});

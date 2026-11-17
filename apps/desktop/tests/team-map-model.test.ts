import test from 'node:test';
import assert from 'node:assert/strict';
import {
  CARD, addSession, arrange, borderPoint, changed, connect, newAgentId, parseEvaluation, parseTeam, pendingLinks, removeAgent, tidy,
  type Agent, type Evaluation, type Formation, type Link,
} from '../src/team-map-model.ts';

const agent = (id: string, extra: Partial<Agent> = {}): Agent => ({
  id: `a_${id.padEnd(12, 'a')}`, session: null, model: null, name: id, notes: '', share: 'split', x: 0, y: 0, ...extra,
});
const ids = (...names: string[]) => names.map((name) => `a_${name.padEnd(12, 'a')}`);
const link = (from: string, to: string, extra: Partial<Link> = {}): Link => ({
  from: `a_${from.padEnd(12, 'a')}`, to: `a_${to.padEnd(12, 'a')}`, kind: 'work', report: 'done', words: '', ...extra,
});
const team = (names: string[], links: Link[] = []): Formation => ({ revision: 3, agents: names.map((name) => agent(name)), links });
const id = (name: string) => `a_${name.padEnd(12, 'a')}`;
/** The roles the engine would answer, taken from the arrows alone. */
const rolesOf = (f: Formation): Evaluation['roles'] => Object.fromEntries(f.agents.map((a) => {
  const gives = f.links.some((l) => l.kind === 'work' && l.from === a.id), takes = f.links.some((l) => l.kind === 'work' && l.to === a.id);
  const observes = f.links.some((l) => l.kind === 'observes' && l.from === a.id);
  const role = observes ? 'observer' : gives && takes ? 'leadWorker' : gives ? 'lead' : takes ? 'worker' : 'peer';
  return [a.id, { role, detail: role, mukade: 'peer', handOut: null }];
}));
const at = (f: Formation, name: string) => {
  const found = f.agents.find((a) => a.id === id(name));
  assert.ok(found, `${name} is on the map`);
  return found;
};
const place = (points: Record<string, { x: number; y: number }>, name: string) => {
  const point = points[id(name)];
  assert.ok(point, `${name} is placed`);
  return point;
};
const closeCards = (points: Record<string, { x: number; y: number }>) => {
  const all = Object.entries(points), found: string[] = [];
  for (let i = 0; i < all.length; i += 1) for (let j = i + 1; j < all.length; j += 1) {
    if (Math.abs(all[i][1].x - all[j][1].x) < CARD.width && Math.abs(all[i][1].y - all[j][1].y) < CARD.height) found.push(`${all[i][0]} ${all[j][0]}`);
  }
  return found;
};

test('connecting replaces the arrow between two agents', () => {
  const [a, b, c] = ids('a', 'b', 'c');
  const start: Formation = { ...team(['a', 'b', 'c'], [link('b', 'a', { kind: 'observes', words: 'old' }), link('a', 'c')]) };
  const once = connect(start, a, b);
  assert.deepEqual(once.links, [link('a', 'c'), { from: a, to: b, kind: 'work', report: 'done', words: '' }]);
  assert.equal(connect(once, a, b).links.length, 2);
  const reversed = connect(once, b, a);
  assert.equal(reversed.links.length, 2);
  assert.ok(reversed.links.some((l) => l.from === b && l.to === a && l.kind === 'work'));
  assert.equal(reversed.links.some((l) => l.from === a && l.to === b), false);
  assert.equal(start.links.length, 2, 'the formation it was given is not touched');
  assert.equal(connect(start, a, a), start);
  assert.equal(connect(start, a, id('zzz')), start);
  assert.equal(connect(start, c, c).links.length, 2);
});

test('removing an agent removes its arrows', () => {
  const f = team(['a', 'b', 'c'], [link('a', 'b'), link('b', 'c'), link('c', 'a', { kind: 'observes' })]);
  const out = removeAgent(f, id('b'));
  assert.deepEqual(out.agents.map((a) => a.id), ids('a', 'c'));
  assert.deepEqual(out.links, [link('c', 'a', { kind: 'observes' })]);
  assert.equal(out.revision, 3);
  assert.equal(f.agents.length, 3);
});

test('a session is added once, with a name the engine accepts', () => {
  const f = addSession(team([]), 't_abcdefghij', '  @Planner\nwith a very long title that goes past the forty characters allowed  ', { x: 10.4, y: 20.6 });
  assert.equal(f.agents.length, 1);
  const [added] = f.agents;
  assert.match(added.id, /^a_[a-z2-7]{12}$/);
  assert.equal(added.session, 't_abcdefghij');
  assert.deepEqual([added.x, added.y], [10, 21]);
  assert.ok(added.name.length >= 1 && added.name.length <= 40 && !/[@\n]/.test(added.name) && added.name === added.name.trim());
  assert.equal(addSession(f, 't_abcdefghij', 'again', { x: 0, y: 0 }), f);
  assert.equal(addSession(team([]), 't_klmnopqrst', '@', { x: 0, y: 0 }).agents[0].name, 'Agent');
  const full = team(Array.from({ length: 24 }, (_, i) => `n${i}`));
  assert.equal(addSession(full, 't_klmnopqrst', 'x', { x: 0, y: 0 }), full);
});

test('new agent ids are the ones the engine accepts', () => {
  const seen = new Set(Array.from({ length: 50 }, newAgentId));
  assert.equal(seen.size, 50);
  for (const value of seen) assert.match(value, /^a_[a-z2-7]{12}$/);
});

test('moving a card is not a change', () => {
  const applied = team(['a', 'b'], [link('a', 'b')]);
  const moved: Formation = { ...applied, revision: 9, agents: applied.agents.map((a) => ({ ...a, x: a.x + 300, y: a.y + 40 })) };
  assert.equal(changed(applied, moved), false);
  assert.equal(changed(applied, { ...applied, agents: [...applied.agents].reverse(), links: [...applied.links] }), false);
  assert.equal(changed(applied, { ...applied, agents: applied.agents.map((a) => (a.id === id('b') ? { ...a, name: 'other' } : a)) }), true);
  assert.equal(changed(applied, { ...applied, agents: applied.agents.map((a) => (a.id === id('b') ? { ...a, notes: 'x' } : a)) }), true);
  assert.equal(changed(applied, { ...applied, agents: applied.agents.map((a) => (a.id === id('b') ? { ...a, share: 'same' } : a)) }), true);
  assert.equal(changed(applied, { ...applied, agents: applied.agents.map((a) => (a.id === id('b') ? { ...a, model: { program: 'pi', connection: null, model: null } } : a)) }), true);
  assert.equal(changed(applied, { ...applied, links: [link('a', 'b', { report: 'steps' })] }), true);
  assert.equal(changed(applied, { ...applied, links: [] }), true);
  assert.equal(changed(applied, removeAgent(applied, id('b'))), true);
  assert.equal(changed(null, team([])), false);
  assert.equal(changed(null, team(['a'])), true);
});

test('the arrows not applied yet are the ones drawn dashed', () => {
  const applied = team(['a', 'b', 'c'], [link('a', 'b'), link('b', 'c')]);
  assert.deepEqual([...pendingLinks(applied, applied)], []);
  const draft: Formation = { ...applied, links: [link('b', 'c', { words: 'now' }), link('a', 'b'), link('a', 'c')] };
  assert.deepEqual([...pendingLinks(applied, draft)].sort(), [0, 2]);
  assert.deepEqual([...pendingLinks(null, draft)].sort(), [0, 1, 2]);
});

test('a border point is where the line towards another card leaves the card', () => {
  const card = { x: 100, y: 100, width: CARD.width, height: CARD.height };
  const near = (got: { x: number; y: number }, x: number, y: number) => assert.ok(Math.abs(got.x - x) < 1e-9 && Math.abs(got.y - y) < 1e-9, JSON.stringify(got));
  near(borderPoint(card, { x: 195, y: 1000 }, 0), 195, 178);
  near(borderPoint(card, { x: 1000, y: 139 }, 6), 296, 139);
  near(borderPoint(card, { x: 195, y: 139 }, 0), 195, 139);
  // A target closer than the border: the point never passes the middle of the way.
  near(borderPoint(card, { x: 195, y: 140 }, 0), 195, 139.5);
});

test('tidy puts givers above takers and overlaps nothing', () => {
  const f = team(['a', 'b', 'c', 'd', 'e', 'x1', 'x2'], [
    link('a', 'd'), link('a', 'b'), link('b', 'x1'), link('d', 'x2'), link('c', 'b', { kind: 'observes' }),
  ]);
  const points = tidy(f, rolesOf(f), 960);
  assert.deepEqual(Object.keys(points).sort(), f.agents.map((a) => a.id).sort());
  const [a, b, d, x1, x2, e, c] = ['a', 'b', 'd', 'x1', 'x2', 'e', 'c'].map((name) => place(points, name));
  assert.ok(a.y < b.y && b.y === d.y, 'a is in the first row, b and d in the second');
  assert.ok(x1.y > b.y && x1.y === x2.y);
  assert.equal(x1.x, b.x, 'x1 is under b');
  assert.equal(x2.x, d.x, 'x2 is under d');
  assert.ok(e.y > x1.y, 'e, with no arrow, is in the last row');
  assert.equal(c.y, b.y, 'the observer is level with what it observes');
  for (const [name, point] of Object.entries({ a, b, d, x1, x2, e })) assert.ok(c.x >= point.x + CARD.width, `c is right of ${name}`);
  assert.deepEqual(closeCards(points), []);
  for (const point of Object.values(points)) assert.ok(Number.isInteger(point.x) && Number.isInteger(point.y) && point.x >= 8 && point.y >= 8 && point.x + CARD.width <= 960);
});

test('tidy orders a layer to cross few arrows', () => {
  const f = team(['a', 'b', 'p', 'q'], [link('a', 'q'), link('b', 'p')]);
  const crossed = tidy(f, rolesOf(f), 960);
  assert.ok(place(crossed, 'a').x < place(crossed, 'b').x);
  assert.ok(place(crossed, 'q').x < place(crossed, 'p').x, 'q is under a, p under b');
});

test('tidy survives a loop', () => {
  const f = team(['a', 'b', 'c', 'z'], [link('a', 'b'), link('b', 'c'), link('c', 'a'), link('c', 'z')]);
  const points = tidy(f, rolesOf(f), 960);
  assert.equal(Object.keys(points).length, 4);
  assert.deepEqual(closeCards(points), []);
  const own = team(['a'], [link('a', 'a')]);
  assert.equal(Object.keys(tidy(own, rolesOf(own), 960)).length, 1);
  assert.deepEqual(tidy(team([]), {}, 960), {});
});

test('a wide layer wraps', () => {
  const names = ['boss', ...Array.from({ length: 10 }, (_, i) => `w${i}`)];
  const f = team(names, names.slice(1).map((name) => link('boss', name)));
  const points = tidy(f, rolesOf(f), 960);
  const rows = [...new Set(names.slice(1).map((name) => place(points, name).y))].sort((x, y) => x - y);
  assert.equal(rows.length, 3, 'ten cards of 190 do not fit two rows of 960');
  assert.ok(rows[0] > place(points, 'boss').y);
  assert.deepEqual(closeCards(points), []);
  for (const point of Object.values(points)) assert.ok(point.x >= 8 && point.x + CARD.width <= 960);
  const narrow = tidy(f, rolesOf(f), 200);
  assert.deepEqual(closeCards(narrow), []);
});

test('several observers stack beside what they observe', () => {
  const f = team(['a', 'b', 'w1', 'w2'], [link('a', 'b'), link('w1', 'b', { kind: 'observes' }), link('w2', 'b', { kind: 'observes' })]);
  const points = tidy(f, rolesOf(f), 960);
  const [w1, w2, b] = ['w1', 'w2', 'b'].map((name) => place(points, name));
  assert.equal(w1.x, w2.x);
  assert.ok(w1.y === b.y && w2.y >= w1.y + CARD.height);
  assert.deepEqual(closeCards(points), []);
});

test('star keeps observers and points the hub at everyone', () => {
  const f = team(['hub', 'a', 'b', 'c', 'watcher'], [link('a', 'b', { words: 'old' }), link('watcher', 'c', { kind: 'observes', words: 'explain' })]);
  const out = arrange(f, 'star', id('hub'), 960);
  assert.deepEqual(out.links, [
    link('watcher', 'c', { kind: 'observes', words: 'explain' }),
    link('hub', 'a'), link('hub', 'b'), link('hub', 'c'),
  ]);
  assert.equal(out.links.some((l) => l.to === id('watcher') || l.from === id('a')), false, 'the observer takes no work');
  assert.equal(f.links.length, 2, 'the formation it was given is not touched');
  const points = Object.fromEntries(out.agents.map((x) => [x.id, { x: x.x, y: x.y }]));
  assert.deepEqual(closeCards(points), []);
  assert.ok(at(out, 'watcher').x > at(out, 'hub').x + CARD.width, 'the observer is at the right edge');
  assert.ok(out.agents.every((x) => Number.isInteger(x.x) && Number.isInteger(x.y)));
  // The hub is the first agent that gives work when none is named; a named observer cannot be the hub.
  const unnamed = arrange(team(['a', 'b', 'c'], [link('b', 'c')]), 'star', null, 960);
  assert.deepEqual(unnamed.links.map((l) => [l.from, l.to]), [[id('b'), id('a')], [id('b'), id('c')]]);
  const refused = arrange(f, 'star', id('watcher'), 960);
  assert.equal(refused.links.filter((l) => l.kind === 'work').every((l) => l.from !== id('watcher')), true);
  // An existing arrow that stays keeps its settings.
  assert.equal(arrange(f, 'star', id('a'), 960).links.find((l) => l.from === id('a') && l.to === id('b'))?.words, 'old');
});

test('star puts many spokes on corners or an ellipse', () => {
  for (const count of [2, 3, 4, 7]) {
    const f = team(['hub', ...Array.from({ length: count }, (_, i) => `s${i}`)]);
    const out = arrange(f, 'star', id('hub'), 960);
    assert.equal(out.links.length, count);
    const hub = at(out, 'hub');
    for (const spoke of out.agents.filter((x) => x.id !== hub.id)) assert.ok(spoke.x !== hub.x || spoke.y !== hub.y);
    assert.ok(out.agents.every((x) => x.x >= 8 && x.y >= 8 && x.x + CARD.width <= 960));
  }
});

test('chain gives work from each agent to the next', () => {
  const f = team(['a', 'b', 'c', 'w'], [link('a', 'c', { report: 'strict' }), link('w', 'a', { kind: 'observes' })]);
  const out = arrange(f, 'chain', id('b'), 960);
  assert.deepEqual(out.links.map((l) => [l.kind, l.from, l.to]), [
    ['observes', id('w'), id('a')], ['work', id('b'), id('a')], ['work', id('a'), id('c')],
  ]);
  assert.equal(out.links[2].report, 'strict', 'the arrow that stays keeps its settings');
  assert.ok(at(out, 'b').x < at(out, 'a').x && at(out, 'a').x < at(out, 'c').x, 'laid out in a line, from the hub');
  assert.equal(at(out, 'a').y, at(out, 'b').y);
  const crowded = arrange(team(Array.from({ length: 9 }, (_, i) => `n${i}`)), 'chain', null, 960);
  assert.equal(crowded.links.length, 8);
  assert.deepEqual(closeCards(Object.fromEntries(crowded.agents.map((x) => [x.id, { x: x.x, y: x.y }]))), []);
});

test('peers drops the work arrows and keeps what observes', () => {
  const f = team(['a', 'b', 'c', 'w'], [link('a', 'b'), link('b', 'c'), link('w', 'a', { kind: 'observes' })]);
  const out = arrange(f, 'peers', null, 960);
  assert.deepEqual(out.links, [link('w', 'a', { kind: 'observes' })]);
  assert.deepEqual(closeCards(Object.fromEntries(out.agents.map((x) => [x.id, { x: x.x, y: x.y }]))), []);
  assert.deepEqual(arrange(team([]), 'star', null, 960), team([]));
});

const wire = {
  enabled: true,
  formation: { revision: 2, agents: [
    { id: 'a_aaaaaaaaaaaa', session: 't_1', model: null, name: 'planner', notes: '', share: 'split', x: 10, y: 20 },
    { id: 'a_bbbbbbbbbbbb', session: null, model: { program: 'opencode', connection: 'janura', model: 'qwen' }, name: 'maker', notes: 'n', share: 'free', x: 300, y: 20 },
  ], links: [{ from: 'a_aaaaaaaaaaaa', to: 'a_bbbbbbbbbbbb', kind: 'work', report: 'steps', words: 'go' }] },
  told: { a_aaaaaaaaaaaa: { revision: 2, member: 'claude-1', digest: 'x' }, notice: { revision: 2, member: '', digest: '' } },
  sessions: [{ sessionId: 't_1', harness: 'claude_code', title: 'plan', state: 'running', memberName: 'claude-1', connection: 'in_room', wakes: 4 }],
  models: [{ program: 'claude_code', connection: null, model: null, label: 'Claude Code' }],
  evaluation: {
    roles: { a_aaaaaaaaaaaa: { role: 'lead', detail: 'Lead', mukade: 'peer', handOut: null }, a_bbbbbbbbbbbb: { role: 'worker', detail: 'Worker', mukade: 'peer', handOut: 'x' } },
    issues: [{ level: 'warning', code: 'w', text: 'careful', agents: ['a_aaaaaaaaaaaa'], links: [0] }],
    sentences: [['You give work.', 'You take work.']],
  },
};

test('the answer of team_get is read, with the wire format of the engine', () => {
  const parsed = parseTeam(wire);
  assert.ok(parsed);
  assert.equal(parsed.enabled, true);
  assert.equal(parsed.formation?.agents.length, 2);
  assert.deepEqual(parsed.formation?.agents[1].model, { program: 'opencode', connection: 'janura', model: 'qwen' });
  assert.equal(parsed.formation?.links[0].report, 'steps');
  assert.equal(parsed.sessions[0].wakes, 4);
  assert.equal(parsed.evaluation?.roles.a_bbbbbbbbbbbb.handOut, 'x');
  assert.equal(parsed.told.a_aaaaaaaaaaaa.member, 'claude-1');
  assert.equal(parsed.models[0].label, 'Claude Code');
  assert.deepEqual(parseTeam({ enabled: false }), { enabled: false, formation: null, told: {}, sessions: [], models: [], evaluation: null });
  assert.deepEqual(parseTeam({ ...wire, formation: null, evaluation: null })?.formation, null);
  assert.deepEqual(parseEvaluation(wire.evaluation)?.sentences, [['You give work.', 'You take work.']]);
  for (const role of ['lead', 'worker', 'leadWorker', 'observer', 'peer']) {
    assert.equal(parseEvaluation({ ...wire.evaluation, roles: { x: { role, detail: '', mukade: 'peer', handOut: null } } })?.roles.x.role, role);
  }
});

test('parse refuses what is not a team', () => {
  for (const bad of [null, undefined, 'team', 4, [], {}, { enabled: 'yes' }, { ...wire, sessions: 'x' }, { ...wire, formation: 'x' },
    { ...wire, formation: { ...wire.formation, agents: 'x' } },
    { ...wire, formation: { ...wire.formation, revision: -1 } },
    { ...wire, formation: { ...wire.formation, agents: [{ ...wire.formation.agents[0], id: 4 }] } },
    { ...wire, formation: { ...wire.formation, agents: [{ ...wire.formation.agents[0], share: 'all' }] } },
    { ...wire, formation: { ...wire.formation, agents: [{ ...wire.formation.agents[0], x: 'left' }] } },
    { ...wire, formation: { ...wire.formation, agents: [{ ...wire.formation.agents[0], model: { program: 4 } }] } },
    { ...wire, formation: { ...wire.formation, links: [{ ...wire.formation.links[0], kind: 'friend' }] } },
    { ...wire, formation: { ...wire.formation, links: [{ ...wire.formation.links[0], to: 'a_zzzzzzzzzzzz' }] } },
    { ...wire, formation: { ...wire.formation, links: [{ ...wire.formation.links[0], to: wire.formation.links[0].from }] } },
    { ...wire, formation: { ...wire.formation, agents: Array.from({ length: 25 }, (_, i) => ({ ...wire.formation.agents[0], id: `a_${String(i).padStart(12, 'a')}` })), links: [] } },
    { ...wire, evaluation: { ...wire.evaluation, issues: [{ level: 'fatal', code: 'c', text: 't', agents: [], links: [] }] } },
    { ...wire, evaluation: { ...wire.evaluation, sentences: [['only one']] } },
    { ...wire, evaluation: { ...wire.evaluation, roles: { x: { role: 'boss', detail: '', mukade: 'peer', handOut: null } } } },
    { ...wire, evaluation: 'x' }]) {
    assert.equal(parseTeam(bad), null, JSON.stringify(bad)?.slice(0, 80));
  }
  assert.equal(parseEvaluation(null), null);
  assert.equal(parseEvaluation({ roles: {}, issues: [] }), null);
});

// What the team map smoke drives, kept apart from how the window is started so the same steps run against any page that mounts the map.
// The backend commands of the map (`team_get`, `team_evaluate`, `team_apply`, `team_place`) are stubbed in the page: the checks are the
// view's, not the engine's (the stub only knows the few rules the steps below need: a loop, "only done / blocked" next to another
// relation, two givers to one agent). Three sessions of one thread are stubbed: two Claude Code, one OpenCode.
export const SESSIONS = [
  { sessionId: 's_aaaaaaaaaaaa', harness: 'claude_code', title: 'Planner', state: 'running', memberName: 'claude-1', connection: 'in_room', wakes: 0 },
  { sessionId: 's_bbbbbbbbbbbb', harness: 'claude_code', title: 'Builder', state: 'running', memberName: 'claude-2', connection: 'in_room', wakes: 1 },
  { sessionId: 's_cccccccccccc', harness: 'opencode', title: 'Local qwen', state: 'running', memberName: null, connection: 'launched', wakes: 8 },
];

/** Replaces the page's `invoke` for the team commands (and the room's snapshot); every other command goes on to the real one. */
export async function installStub(page) {
  await page.evaluate(sessions => {
    const stub = { calls: [], stored: null, told: {}, refused: {}, enabled: true, stale: false, holdNext: false, sessions, events: [] };
    const roleOf = (f, id) => {
      const gives = f.links.filter(l => l.kind === 'work' && l.from === id), takes = f.links.filter(l => l.kind === 'work' && l.to === id);
      const sees = f.links.some(l => l.kind === 'observes' && l.from === id);
      if (sees && !gives.length && !takes.length) return 'observer';
      return gives.length && takes.length ? 'leadWorker' : gives.length ? 'lead' : takes.length ? 'worker' : 'peer';
    };
    const name = (f, id) => f.agents.find(a => a.id === id)?.name ?? id;
    // What the Models panel holds, as team_get lists it: the four own sign-ins first, then each model once per program.
    stub.models = [['claude_code', 'Claude Code'], ['codex', 'Codex'], ['opencode', 'OpenCode'], ['pi', 'Pi']]
      .map(([program, label]) => ({ program, connection: null, model: null, label: `${label} \u00b7 its own sign-in and model` }))
      .concat([['opencode', 'OpenCode'], ['pi', 'Pi']].map(([program, label]) => ({ program, connection: 'home', model: 'qwen-9', label: `qwen-9 \u00b7 runs in ${label}` })));
    const held = m => stub.models.some(x => x.program === m.program && x.connection === m.connection && x.model === m.model);
    stub.opened = []; stub.failFor = null;
    stub.evaluate = f => {
      const issues = [], roles = {};
      for (const a of f.agents) {
        if (!a.session && !a.model) issues.push({ level: 'error', code: 'no_model', text: `${a.name} has no session and no model, so it cannot be opened.`, agents: [a.id], links: [] });
        else if (!a.session && !held(a.model)) issues.push({ level: 'error', code: 'model_gone', text: `${a.name} uses ${a.model.model ?? a.model.program}, which the Models panel no longer holds. Choose another model.`, agents: [a.id], links: [] });
        const role = roleOf(f, a.id);
        roles[a.id] = { role, detail: `${role} by its arrows`, mukade: role === 'observer' ? 'observer' : role === 'worker' ? 'worker' : 'peer', handOut: null };
      }
      f.links.forEach((l, i) => {
        // A loop of work: this arrow leads back to its own start.
        const seen = new Set(); let here = l.to; let loop = l.kind === 'work';
        while (loop && here !== l.from) { const next = f.links.find(x => x.kind === 'work' && x.from === here && !seen.has(x)); if (!next) { loop = false; break; } seen.add(next); here = next.to; }
        if (loop && l.kind === 'work') issues.push({ level: 'error', code: 'loop', text: `Loop: ${name(f, l.from)} → ${name(f, l.to)} leads back to itself. Each would wait for the next.`, agents: [l.from, l.to], links: [i] });
      });
      for (const a of f.agents) {
        const strict = f.links.map((l, i) => [l, i]).filter(([l]) => l.kind === 'work' && l.report === 'strict' && l.to === a.id);
        const extra = f.links.map((l, i) => [l, i]).filter(([l]) => (l.from === a.id || l.to === a.id) && !(l.kind === 'work' && l.report === 'strict' && l.to === a.id) && !(l.kind === 'observes' && l.to === a.id));
        if (strict.length && extra.length) issues.push({ level: 'error', code: 'strict-extra', text: `${a.name} only executes, so it can answer only done or blocked: it cannot also take part in another relation.`, agents: [a.id], links: [...strict, ...extra].map(([, i]) => i) });
        const givers = f.links.map((l, i) => [l, i]).filter(([l]) => l.kind === 'work' && l.to === a.id);
        if (givers.length > 1) issues.push({ level: 'warning', code: 'two-givers', text: `${a.name} takes work from ${givers.length} agents: their instructions may conflict.`, agents: [a.id], links: givers.map(([, i]) => i) });
      }
      const sentences = f.links.map(l => {
        const how = l.report === 'strict' ? 'answer only done or blocked' : l.report === 'steps' ? 'tell what you are doing at every step' : 'report back when it is done';
        const said = l.words ? ` The user adds: “${l.words}”.` : '';
        return l.kind === 'work'
          ? [`You give work to ${name(f, l.to)}; it will ${how}.${said}`, `You wait for work from ${name(f, l.from)}, do it, and ${how}.${said}`]
          : [`You observe ${name(f, l.to)}: follow what it does. Do not write to it.${said}`, `${name(f, l.from)} observes you and may read what you do.${said}`];
      });
      return { roles, issues, sentences };
    };
    // Everyone is told at once, except what `release()` has not let through yet: the window sees the rest at its next poll.
    stub.held = [];
    stub.release = () => { for (const [id, record] of stub.held) stub.told[id] = record; stub.held = []; };
    const handlers = {
      team_get: () => ({ enabled: stub.enabled, formation: stub.stored, told: stub.told, refused: stub.refused, sessions: stub.sessions, models: stub.models, evaluation: stub.stored ? stub.evaluate(stub.stored) : null }),
      team_evaluate: ({ formation }) => stub.evaluate(formation),
      team_apply: ({ formation, expectedRevision }) => {
        const revision = stub.stored?.revision ?? 0;
        if (stub.stale || revision !== expectedRevision) throw `The team was changed elsewhere (revision ${revision + 1}). Reload it.`;
        const problem = stub.evaluate(formation).issues.find(i => i.level === 'error'); if (problem) throw problem.text;
        // Opens a session for each agent without one, one after another, and stops at the first that fails. (The tab of each arrives
        // as the backend's `collaboration-session-opened` event; the window may not emit events, so that part is for the real run.)
        let failed = null;
        for (const a of formation.agents) {
          if (a.session) continue;
          if (stub.failFor === a.name) { failed = { agent: a.id, reason: `${a.model.program} did not start.` }; break; }
          const id = `t_${String(stub.opened.length + 1).padStart(12, 'o')}`;
          stub.opened.push(a.name); a.session = id;
          stub.sessions = [...stub.sessions, { sessionId: id, harness: a.model.program, title: a.name, state: 'running', memberName: null, connection: 'launched', wakes: 0 }];
        }
        stub.stored = { ...JSON.parse(JSON.stringify(formation)), revision: expectedRevision + 1 };
        stub.held = []; stub.told = {};
        stub.stored.agents.forEach((a, i) => { const record = [a.id, { revision: stub.stored.revision, member: `m_${i}` }]; if (i === 0) stub.told[a.id] = record[1]; else stub.held.push(record); });
        return { formation: stub.stored, failed };
      },
      team_place: ({ places }) => {
        if (!stub.stored) throw 'The thread has no team.';
        stub.stored = { ...stub.stored, agents: stub.stored.agents.map(a => places[a.id] ? { ...a, x: places[a.id][0], y: places[a.id][1] } : a) };
        return {};
      },
      thread_chat_snapshot: () => ({ enabled: true, members: [], sessions: [], events: stub.events }),
    };
    // `window.__TAURI_INTERNALS__.invoke` cannot be replaced (it is not writable); on Windows it is a POST to http://ipc.localhost/<command>
    // whose answer is read from the `Tauri-Response` header, so the stub answers at that layer and every other command goes on.
    const realFetch = window.fetch.bind(window);
    window.fetch = async (input, init) => {
      const url = String(input?.url ?? input);
      const command = /^https?:\/\/ipc\.localhost\/([a-z_]+)/.exec(url)?.[1];
      const handler = command && handlers[command];
      if (!handler) return realFetch(input, init);
      const request = init?.body ? JSON.parse(String(init.body)).request ?? {} : {};
      stub.calls.push({ command, request: JSON.parse(JSON.stringify(request)), at: performance.now() });
      const answer = (ok, value) => new Response(JSON.stringify(value), { status: 200, headers: { 'Tauri-Response': ok ? 'ok' : 'error', 'Content-Type': 'application/json' } });
      try {
        const value = handler(request);
        // A read the test holds answers late, with what the stub held when it was asked: the window must not take it for the truth.
        if (command === 'team_get' && stub.holdNext) { stub.holdNext = false; await new Promise(resolve => { stub.letGo = resolve; }); }
        return answer(true, value);
      } catch (error) { return answer(false, String(error)); }
    };
    window.__teamStub = stub;
  }, SESSIONS);
}

const numberOf = text => Number.parseFloat(text ?? '');

export async function driveTeamMap(page, expect, log = console.log) {
  const map = page.locator('.team-map[data-view-id]').first();
  const canvas = map.locator('.team-canvas');
  const calls = command => page.evaluate(name => window.__teamStub.calls.filter(call => call.command === name), command);
  const count = async command => (await calls(command)).length;
  const bar = name => map.locator('.team-bar').getByRole('button', { name, exact: true });
  const byName = name => map.locator('.team-card').filter({ has: page.locator('.team-card-name', { hasText: new RegExp(`^${name}$`) }) });
  const byId = id => map.locator(`.team-card[data-agent="${id}"]`);
  const idOf = async name => byName(name).getAttribute('data-agent');
  const chip = title => map.locator('.team-session', { hasText: title });
  const top = async id => numberOf(await byId(id).evaluate(node => node.style.top));
  const left = async id => numberOf(await byId(id).evaluate(node => node.style.left));
  const link = (from, to) => map.locator(`.team-link[data-from="${from}"][data-to="${to}"]`);
  const box = map.locator('.team-box-link');
  const agentBox = map.locator('.team-box-agent');
  // Arrows may cross or overlap once tidied, so a test opens one the way the keyboard does.
  const openLink = async (from, to) => { await link(from, to).focus(); await page.keyboard.press('Enter'); };
  // A box that is open may lie over the card an arrow is pulled to, as it may for a user: it is closed first.
  const pull = async (from, to) => {
    if (await box.isVisible()) await box.getByRole('button', { name: 'Close the arrow box' }).click();
    if (await agentBox.isVisible()) await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
    await byId(from).locator('.team-port').dragTo(byId(to));
  };

  // The empty map and its hint.
  await expect(map).toBeVisible();
  await expect(map.locator('.team-hint')).toBeVisible();
  await expect(map.locator('.team-session')).toHaveCount(3);
  await expect(bar('Apply')).toBeDisabled();
  await expect(map.locator('.team-state')).toHaveText('no team');
  log('ok the empty map and its hint');

  // A session dragged from the list lands where it is dropped; another one is added from the keyboard.
  await chip('Planner').dragTo(canvas, { targetPosition: { x: 300, y: 130 } });
  const planner = await idOf('Planner');
  expect(Math.abs(await left(planner) - 205)).toBeLessThanOrEqual(2);
  expect(Math.abs(await top(planner) - 91)).toBeLessThanOrEqual(2);
  await expect(map.locator('.team-hint')).toBeHidden();
  // The window may be small: what is dropped far to the right stays inside the visible map.
  const view = await map.locator('.team-scroll').boundingBox();
  const farRight = Math.min(640, Math.round(view.width) - 130);
  await chip('Builder').dragTo(canvas, { targetPosition: { x: farRight, y: 130 } });
  const builder = await idOf('Builder');
  await chip('Local qwen').focus(); await page.keyboard.press('Enter');
  const qwen = await idOf('Local qwen');
  await expect(map.locator('.team-card')).toHaveCount(3);
  await expect(map.locator('.team-session')).toHaveCount(0);
  await expect(map.locator('.team-side')).toContainText('Every session of the thread is on the map.');
  await expect(byId(qwen).locator('.team-wakes')).toHaveText('2 wakes left');
  await expect(bar('Apply')).toBeEnabled();
  // Moving a card with something pending stores nothing: the place goes with Apply.
  await byId(planner).locator('.team-card-name').dragTo(canvas, { targetPosition: { x: 300, y: 60 } });
  expect(await count('team_place')).toBe(0);
  log('ok sessions dropped, added from the keyboard, and a card moved without team_place while something is pending');

  // Pulling the → onto a card draws a work arrow and opens its box.
  await pull(planner, builder);
  await expect(map.locator('.team-link')).toHaveCount(1);
  await expect(box).toBeVisible();
  await expect(box.locator('h3')).toContainText('Planner → Builder');
  await expect(byId(planner).locator('.team-card-role')).toHaveText('Lead');
  await expect(byId(builder).locator('.team-card-role')).toHaveText('Worker');
  // The box changes kind, report and words, reverses and deletes.
  await box.locator('.team-kinds button[data-kind="observes"]').click();
  await expect(link(planner, builder).locator('.team-wire-label')).toHaveText('observes');
  await expect(byId(planner).locator('.team-card-role')).toHaveText('Observer');
  await box.locator('.team-kinds button[data-kind="work"]').click();
  await expect(link(planner, builder).locator('.team-wire-label')).toHaveText('gives work to');
  await box.locator('.team-seg button[data-report="strict"]').click();
  await expect(link(planner, builder).locator('.team-wire-label')).toHaveText('gives work · only done / blocked');
  await expect(box.locator('.team-says')).toContainText('answer only done or blocked');
  const words = box.locator('input');
  await words.fill('check twice');
  await expect(box.locator('.team-says')).toContainText('The user adds: “check twice”');
  await expect(link(planner, builder).locator('.team-wire-label')).toContainText('check twice');
  // However fast the user types, the engine is asked at most once per 150 ms.
  const before = await count('team_evaluate');
  await words.fill(''); await words.pressSequentially('abcdefghijklmnop', { delay: 12 });
  await expect(box.locator('.team-says')).toContainText('abcdefghijklmnop');
  const asked = (await calls('team_evaluate')).slice(before);
  expect(asked.length).toBeGreaterThan(0); expect(asked.length).toBeLessThan(16);
  const all = await calls('team_evaluate');
  for (let i = 1; i < all.length; i += 1) expect(all[i].at - all[i - 1].at).toBeGreaterThanOrEqual(140);
  await words.fill('');
  await box.getByRole('button', { name: '⇄ Reverse' }).click();
  await expect(box.locator('h3')).toContainText('Builder → Planner');
  await box.getByRole('button', { name: '⇄ Reverse' }).click();
  await expect(box.locator('h3')).toContainText('Planner → Builder');
  await box.getByRole('button', { name: 'Delete the arrow' }).click();
  await expect(map.locator('.team-link')).toHaveCount(0);
  await expect(box).toBeHidden();
  log('ok an arrow: created from the →, kind, report, words, throttled evaluation, reverse, delete');

  // A struck-out option shows the reason the engine gave; a problem keeps Apply disabled and says so beyond colour.
  await pull(planner, builder);
  await box.locator('.team-seg button[data-report="strict"]').click();
  await pull(builder, qwen);
  await expect(box.locator('h3')).toContainText('Builder → Local qwen');
  const observes = box.locator('.team-kinds button[data-kind="observes"]');
  await expect(observes).toBeDisabled();
  await expect(observes).toContainText('✕');
  await expect(observes).toContainText('only executes');
  await expect(map.locator('.team-problems')).toBeVisible();
  await expect(map.locator('.team-problems button[data-level="error"]').first()).toContainText('Error: Builder only executes');
  await expect(link(builder, qwen).locator('.team-wire-label')).toContainText('✕');
  await expect(byId(builder).locator('.team-card-bad')).toHaveText('✕');
  await expect(bar('Apply')).toBeDisabled();
  await expect(map.locator('.team-state')).toContainText('problem');
  await openLink(planner, builder);
  await box.locator('.team-seg button[data-report="done"]').click();
  await expect(map.locator('.team-problems button[data-level="error"]')).toHaveCount(0);
  await expect(bar('Apply')).toBeEnabled();
  log('ok a struck-out option with the engine\'s reason, the problems strip and Apply disabled on an error');

  // The card's box edits name and notes and shows the role; hand-out appears with two or more work arrows.
  await byId(planner).locator('.team-card-name').click();
  await expect(agentBox).toBeVisible();
  await expect(agentBox.locator('.team-seg')).toBeHidden();
  await expect(agentBox.locator('.role-line')).toContainText('Lead');
  await pull(planner, qwen);
  await expect(map.locator('.team-problems button[data-level="warning"]')).toContainText('Warning: Local qwen takes work from 2 agents');
  await expect(bar('Apply')).toBeEnabled();
  await byId(planner).locator('.team-card-name').click();
  await expect(agentBox.locator('.team-seg')).toBeVisible();
  await agentBox.locator('.team-seg button[data-share="same"]').click();
  await expect(byId(planner).locator('.team-card-role')).toHaveText('Lead · same work to all');
  await agentBox.locator('input').first().fill('Lead dev');
  await expect(byId(planner).locator('.team-card-name')).toHaveText('Lead dev');
  await agentBox.locator('textarea').fill('Keep the plan short.');
  await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
  await byId(planner).locator('.team-card-name').click();
  await expect(agentBox.locator('textarea')).toHaveValue('Keep the plan short.');
  log('ok the card box: name, notes, role with its reason, hand-out with two work arrows');

  // Tidy up places the cards by their arrows; keep tidy does it at every change.
  await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
  await bar('⤢ Tidy up').click();
  await expect.poll(async () => (await top(planner)) < (await top(builder)) && (await top(builder)) < (await top(qwen))).toBe(true);
  await map.locator('.team-auto input').check();
  await openLink(builder, qwen);
  await box.getByRole('button', { name: 'Delete the arrow' }).click();
  await expect.poll(async () => (await top(builder)) === (await top(qwen))).toBe(true);
  await map.locator('.team-auto input').uncheck();
  // Star, Chain and Peers draw the arrows from the card that is open.
  await byId(builder).locator('.team-card-name').click();
  await bar('Star').click();
  await expect(map.locator('.team-link')).toHaveCount(2);
  for (const l of await map.locator('.team-link').all()) expect(await l.getAttribute('data-from')).toBe(builder);
  await bar('Chain').click();
  await expect(map.locator('.team-link')).toHaveCount(2);
  await bar('Peers').click();
  await expect(map.locator('.team-link')).toHaveCount(0);
  log('ok tidy up, keep tidy, star, chain, peers');

  // Apply sends the draft against the revision it started from; the cards say who was told, from team_get polled while visible.
  await pull(planner, builder);
  await pull(planner, qwen);
  await expect(bar('Apply')).toBeEnabled();
  await bar('Apply').click();
  await expect(map.locator('.team-state')).toContainText('revision 1');
  const applied = (await calls('team_apply'));
  expect(applied).toHaveLength(1);
  expect(applied[0].request.expectedRevision).toBe(0);
  expect(applied[0].request.formation.agents).toHaveLength(3);
  expect(applied[0].request.formation.links).toHaveLength(2);
  await expect(byId(planner).locator('.team-told')).toHaveText('told · revision 1');
  await expect(byId(builder).locator('.team-told')).toHaveText('not told yet');
  await expect(byId(qwen).locator('.team-told')).toHaveText('told when it joins');
  await expect(bar('Apply')).toBeDisabled();
  const polls = await count('team_get');
  await page.evaluate(() => window.__teamStub.release());
  await expect(byId(builder).locator('.team-told')).toHaveText('told · revision 1', { timeout: 13000 });
  await expect(map.locator('.team-state')).toHaveText('revision 1 · everyone was told');
  expect(await count('team_get')).toBeGreaterThan(polls);
  log('ok Apply, and the told states from team_get polled every 10 s');

  // A message Mukade refused, or a service that is not running, is said on the card and in the bar: not "not told yet", not an old "told".
  const lookAgain = () => page.evaluate(() => document.dispatchEvent(new Event('visibilitychange')));
  await page.evaluate(id => { window.__teamStub.refused = { [id]: 'secret_refused: the notes look like a secret' }; }, builder);
  await lookAgain();
  await expect(byId(builder).locator('.team-told')).toHaveText('could not be told: secret_refused: the notes look like a secret');
  await expect(byId(builder)).toHaveAttribute('data-bad', '');
  await expect(byId(planner).locator('.team-told')).toHaveText('told · revision 1');
  await expect(map.locator('.team-state')).toHaveText('revision 1 · 1 could not be told: see the cards');
  const wasTold = await page.evaluate(() => window.__teamStub.told);
  await page.evaluate(id => { window.__teamStub.refused = { service: 'Mukade chat is not running.' }; window.__teamStub.told = { [id]: window.__teamStub.told[id] }; }, planner);
  await lookAgain();
  await expect(byId(builder).locator('.team-told')).toHaveText('could not be told: Mukade chat is not running.');
  await expect(byId(qwen).locator('.team-told')).toHaveText('could not be told: Mukade chat is not running.');
  await expect(byId(planner).locator('.team-told')).toHaveText('told · revision 1');
  await expect(map.locator('.team-state')).toHaveText('revision 1 · could not be told: Mukade chat is not running.');
  await expect(map.locator('.team-state')).toHaveAttribute('data-tone', 'error');
  await page.evaluate(told => { window.__teamStub.refused = {}; window.__teamStub.told = told; }, wasTold);
  await lookAgain();
  await expect(map.locator('.team-state')).toHaveText('revision 1 · everyone was told');
  await expect(byId(builder).locator('.team-told')).toHaveText('told · revision 1');
  log('ok a refused message and a stopped service are shown on the cards and in the bar');

  // With nothing pending a moved card is stored with team_place.
  await byId(builder).locator('.team-card-name').dragTo(canvas, { targetPosition: { x: farRight, y: Math.min(330, Math.round(view.height) - 60) } });
  await expect.poll(() => count('team_place')).toBe(1);
  const placed = (await calls('team_place'))[0].request.places;
  expect(Object.keys(placed)).toEqual([builder]);
  await expect(bar('Apply')).toBeDisabled();
  expect((await page.evaluate(() => window.__teamStub.stored.revision))).toBe(1);
  log('ok team_place when nothing else is pending');

  // Discard throws the draft away; a stale revision is said, not hidden.
  await openLink(planner, builder);
  await box.getByRole('button', { name: 'Delete the arrow' }).click();
  await expect(map.locator('.team-link')).toHaveCount(1);
  await expect(bar('Apply')).toBeEnabled();
  await bar('Discard').click();
  await expect(map.locator('.team-link')).toHaveCount(2);
  await expect(bar('Apply')).toBeDisabled();
  await openLink(planner, builder);
  await box.locator('input').fill('stale');
  await page.evaluate(() => { window.__teamStub.stale = true; });
  await bar('Apply').click();
  await expect(map.locator('.team-status[data-tone="error"]')).toContainText('changed elsewhere');
  await page.evaluate(() => { window.__teamStub.stale = false; });
  await bar('Discard').click();
  await expect(map.locator('.team-status')).toBeHidden();
  await expect(map.locator('.team-wire-label').first()).not.toContainText('stale');
  log('ok discard and a refused Apply');

  // A card dragged onto the strip at the bottom leaves the map, with its arrows; its session returns to the list.
  await byId(qwen).locator('.team-card-name').dragTo(map.locator('.team-tray'));
  await expect(map.locator('.team-card')).toHaveCount(2);
  await expect(map.locator('.team-link')).toHaveCount(1);
  await expect(chip('Local qwen')).toBeVisible();
  await expect(bar('Apply')).toBeEnabled();
  await bar('Discard').click();
  await expect(map.locator('.team-card')).toHaveCount(3);
  log('ok a card dragged to the strip leaves the map');

  // Every action from the keyboard: Enter opens the box, Connect to… draws an arrow, an arrow takes focus and Enter.
  await byId(builder).focus(); await page.keyboard.press('Enter');
  await expect(agentBox).toBeVisible();
  await expect(agentBox.locator('input').first()).toBeFocused();
  await agentBox.getByLabel('Connect to…').selectOption(planner);
  await expect(box).toBeVisible();
  await expect(box.locator('h3')).toContainText('Builder → Lead dev');
  await page.keyboard.press('Escape');
  await expect(box).toBeHidden();
  await expect(link(builder, planner)).toBeFocused();
  await page.keyboard.press('Enter');
  await expect(box).toBeVisible();
  await page.keyboard.press('Escape');
  await expect(byId(planner)).toHaveAttribute('role', 'button');
  log('ok keyboard: Enter on a card, Connect to…, Escape, Enter on an arrow');

  // An older read that is still on its way when Apply answers must not put the team back as it was: the window takes the answer of a read that began after Apply.
  await bar('Discard').click();
  await openLink(planner, builder);
  await box.locator('input').fill('after the stale read');
  await page.evaluate(() => { window.__teamStub.holdNext = true; });
  const readsBefore = await count('team_get');
  await lookAgain();
  await expect.poll(() => count('team_get')).toBeGreaterThan(readsBefore);
  await bar('Apply').click();
  await expect(map.locator('.team-state')).toContainText('revision 2');
  await page.evaluate(() => window.__teamStub.letGo());
  await expect.poll(() => count('team_get')).toBeGreaterThan(readsBefore + 1);
  await page.waitForTimeout(500);
  await expect(map.locator('.team-state')).toContainText('revision 2');
  await expect(map.locator('.team-status[data-tone="error"]')).toBeHidden();
  await expect(map.locator('.team-wire-label', { hasText: 'after the stale read' })).toBeVisible();
  await expect(bar('Apply')).toBeDisabled();
  log('ok a read that began before Apply does not overwrite its result');
}

/** The room's notices: what the member `shinatama` writes is drawn apart from what people write. */
export async function driveNotices(page, expect, log = console.log) {
  await page.evaluate(() => {
    window.__teamStub.events = [
      { eventId: '1', timestamp: '2026-11-17T10:00:00Z', kind: 'message', actor: 'shinatama', actorHost: true, recipients: ['claude-1'], text: 'TEAM revision 1, set by the user in Shinatama.', signal: null, reason: null },
      { eventId: '2', timestamp: '2026-11-17T10:00:01Z', kind: 'message', actor: 'shinatama', actorHost: true, recipients: [], text: 'TEAM revision 1. New: Lead dev gives work to Builder', signal: null, reason: null },
      { eventId: '3', timestamp: '2026-11-17T10:00:02Z', kind: 'message', actor: 'claude-2', recipients: ['claude-1'], text: 'Done.', signal: null, reason: null },
      { eventId: '4', timestamp: '2026-11-17T10:00:03Z', kind: 'message', actor: 'shinatama', actorHost: false, recipients: [], text: 'I joined as shinatama by hand.', signal: null, reason: null },
    ];
  });
  await page.locator('.thread-sidebar-chat').first().click();
  const notices = page.locator('.thread-chat-event[data-kind="team"]');
  await expect(notices).toHaveCount(2);
  await expect(notices.nth(0).locator('.thread-chat-tag')).toHaveText('to claude-1');
  await expect(notices.nth(1).locator('.thread-chat-tag')).toHaveText('notice');
  await expect(page.locator('.thread-chat-event[data-kind="message"]')).toHaveCount(2);
  log('ok the host member shinatama is drawn as notices, an agent that took the name is not');
}

// ------------------------------------------------------------------------------------------------------------- templates
// The template commands (`team_templates`, `team_template_save`, `team_template_delete`) are NOT stubbed: they run against the real backend
// and its SQLite store, so what is saved here is what a restart of the window reads back.
const tools = (page, expect) => {
  // Several threads may hold a map in the pane: the one on show is the one driven.
  const map = page.locator('.team-map[data-view-id]:visible').first();
  const bar = name => map.locator('.team-bar').getByRole('button', { name, exact: true });
  const byName = name => map.locator('.team-card').filter({ has: page.locator('.team-card-name', { hasText: new RegExp(`^${name}$`) }) });
  const idOf = async name => byName(name).getAttribute('data-agent');
  const byId = id => map.locator(`.team-card[data-agent="${id}"]`);
  const link = (from, to) => map.locator(`.team-link[data-from="${from}"][data-to="${to}"]`);
  const box = map.locator('.team-box-link');
  const agentBox = map.locator('.team-box-agent');
  const menu = map.locator('.team-templates');
  const chip = title => map.locator('.team-session', { hasText: title });
  const calls = command => page.evaluate(name => window.__teamStub.calls.filter(call => call.command === name), command);
  const openLink = async (from, to) => { await link(from, to).focus(); await page.keyboard.press('Enter'); };
  const pull = async (from, to) => {
    if (await box.isVisible()) await box.getByRole('button', { name: 'Close the arrow box' }).click();
    if (await agentBox.isVisible()) await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
    await byId(from).locator('.team-port').dragTo(byId(to));
  };
  const row = name => menu.locator('.team-template').filter({ has: page.locator('.team-template-text b', { hasText: new RegExp(`^${name}$`) }) });
  const save = menu.getByRole('button', { name: 'Save this team', exact: true });
  return { map, bar, byName, idOf, byId, link, box, agentBox, menu, chip, calls, openLink, pull, row, save };
};

/** On the map of a thread that has a team (the three agents applied): the menu, an error that blocks Save, Save, a refused name, Delete. */
export async function driveTemplateSave(page, expect, log = console.log) {
  const t = tools(page, expect); const { map, bar, menu, save, row } = t;
  if (await bar('Discard').isEnabled()) await bar('Discard').click();
  await expect(map.locator('.team-card')).toHaveCount(3);
  const planner = await t.idOf('Lead dev'), builder = await t.idOf('Builder'), qwen = await t.idOf('Local qwen');

  // The menu opens empty; Save waits for a name.
  await bar('Templates ▾').click();
  await expect(menu).toBeVisible();
  await expect(bar('Templates ▾')).toHaveAttribute('aria-expanded', 'true');
  await expect(menu.locator('.team-template-empty')).toContainText('No template yet');
  await expect(save).toBeDisabled();
  await menu.getByLabel('Name of the template').fill('Review crew');
  await expect(save).toBeEnabled();
  await page.keyboard.press('Escape');
  await expect(menu).toBeHidden();
  await expect(bar('Templates ▾')).toBeFocused();
  log('ok the Templates menu opens empty, Save waits for a name, Escape closes it');

  // An error of the checks blocks Save: the engine decides, the window only says so.
  await t.openLink(planner, builder);
  await t.box.locator('.team-seg button[data-report="strict"]').click();
  await t.pull(builder, qwen);
  await expect(map.locator('.team-problems button[data-level="error"]').first()).toContainText('Error: Builder only executes');
  await bar('Templates ▾').click();
  await expect(save).toBeDisabled();
  await expect(save).toHaveAttribute('title', 'Fix the problems above first.');
  await page.keyboard.press('Escape');
  await bar('Discard').click();
  await expect(map.locator('.team-problems')).toBeHidden();
  log('ok Save is disabled while the team has an error');

  // Save keeps the team with a drawing of its shape; the same name, in other letters, is refused by the store.
  await bar('Templates ▾').click();
  await menu.getByLabel('Name of the template').fill('Review crew');
  await save.click();
  await expect(menu.locator('.team-template')).toHaveCount(1);
  await expect(row('Review crew').locator('.team-template-text span')).toHaveText('3 agents');
  await expect(row('Review crew').locator('.team-mini-card')).toHaveCount(3);
  await expect(row('Review crew').locator('.team-mini-line')).toHaveCount(2);
  await expect(menu.locator('.team-template-said')).toHaveText('Saved “Review crew”.');
  await menu.getByLabel('Name of the template').fill('REVIEW crew');
  await save.click();
  await expect(menu.locator('.team-template-said[data-tone="error"]')).toContainText('A template named Review crew already exists.');
  await expect(menu.locator('.team-template')).toHaveCount(1);
  log('ok Save keeps the team with its miniature and agent count; the same name twice is refused');

  // Delete asks twice; the other template stays.
  await menu.getByLabel('Name of the template').fill('Second');
  await save.click();
  await expect(menu.locator('.team-template')).toHaveCount(2);
  const second = row('Second').getByRole('button', { name: 'Delete Second' });
  await second.click();
  await expect(second).toHaveText('Delete it?');
  await expect(menu.locator('.team-template')).toHaveCount(2);
  await second.click();
  await expect(menu.locator('.team-template')).toHaveCount(1);
  await expect(row('Review crew')).toBeVisible();
  await page.keyboard.press('Escape');
  log('ok Delete asks twice and keeps the others');
}

/** After a restart of the window, on a map: the saved template is listed with what it was saved with. */
export async function driveTemplateListed(page, expect, log = console.log) {
  const t = tools(page, expect); const { bar, menu, row } = t;
  await bar('Templates ▾').click();
  await expect(menu).toBeVisible();
  await expect(menu.locator('.team-template')).toHaveCount(1);
  await expect(row('Review crew').locator('.team-template-text span')).toHaveText('3 agents');
  await expect(row('Review crew').locator('.team-mini-card')).toHaveCount(3);
  await page.keyboard.press('Escape');
  log('ok the template is listed after a restart of the window');
}

/** In another thread: Use, agents with no session and no model, the model menu, Apply opening the sessions (one fails, then the retry). */
export async function driveTemplateUse(page, expect, log = console.log) {
  const t = tools(page, expect); const { map, bar, menu, row, chip, byName, agentBox } = t;
  const modelMenu = agentBox.locator('select[aria-label="Model"]');
  await expect(map.locator('.team-card')).toHaveCount(0);
  await bar('Templates ▾').click();
  await row('Review crew').getByRole('button', { name: 'Use Review crew' }).click();
  await expect(menu).toBeHidden();
  await expect(map.locator('.team-card')).toHaveCount(3);
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(3);
  for (const meta of await map.locator('.team-card-meta').all()) await expect(meta).toHaveText('no session yet');
  await expect(map.locator('.team-link')).toHaveCount(2);
  await expect(map.locator('.team-hint')).toBeHidden();
  // An agent with neither a session nor a model is a problem the engine reports, and Apply says so.
  await expect(bar('Open 3 sessions and apply')).toBeDisabled();
  await expect(bar('Open 3 sessions and apply')).toHaveAttribute('title', 'Fix the problems above first');
  await expect(map.locator('.team-problems button[data-level="error"]')).toHaveCount(3);
  await expect(map.locator('.team-problems')).toContainText('Lead dev has no session and no model');
  await expect(map.locator('.team-session')).toHaveCount(3);
  log('ok Use puts the template on the map: agents with no session and no model are a problem, hatched');

  // A session dropped on an agent takes its place, name and arrows.
  const lead = await t.idOf('Lead dev');
  await chip('Planner').dragTo(byName('Lead dev'));
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(2);
  await expect(t.byId(lead)).not.toHaveAttribute('data-empty', '');
  await expect(t.byId(lead).locator('.team-card-name')).toHaveText('Lead dev');
  await expect(t.byId(lead).locator('.team-card-meta')).toContainText('Planner');
  // The template kept the agent's hand-out: it works as a lead that gives the same work to all.
  await expect(t.byId(lead).locator('.team-card-role')).toHaveText('Lead · same work to all');
  await expect(map.locator('.team-link')).toHaveCount(2);
  await expect(map.locator('.team-session')).toHaveCount(2);
  await expect(map.locator('.team-problems button[data-level="error"]')).toHaveCount(2);
  // The keyboard's way: the agent's box offers the sessions of the thread.
  await byName('Builder').locator('.team-card-name').click();
  await expect(agentBox).toBeVisible();
  await expect(agentBox.locator('h3')).toContainText('no session yet');
  await agentBox.getByLabel('Use a session…').selectOption({ label: 'Builder' });
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(1);
  await expect(agentBox.locator('h3')).toContainText('running session');
  // A running agent's model is fixed: no menu.
  await expect(modelMenu).toBeHidden();
  await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
  log('ok a session dropped on an agent, or chosen in its box, fills it and keeps its name and arrows');

  // The agent without a session chooses its model from the menu: the four own sign-ins first, then each model once per program.
  await byName('Local qwen').locator('.team-card-name').click();
  await expect(modelMenu).toBeVisible();
  const labels = await modelMenu.locator('option').allTextContents();
  expect(labels.slice(0, 5)).toEqual(['Choose a model…', 'Claude Code \u00b7 its own sign-in and model', 'Codex \u00b7 its own sign-in and model', 'OpenCode \u00b7 its own sign-in and model', 'Pi \u00b7 its own sign-in and model']);
  expect(labels.slice(5)).toEqual(['qwen-9 \u00b7 runs in OpenCode', 'qwen-9 \u00b7 runs in Pi']);
  await modelMenu.selectOption({ label: 'Codex \u00b7 its own sign-in and model' });
  await expect(byName('Local qwen').locator('.team-card-meta')).toHaveText('its own model \u00b7 via Codex');
  await expect(map.locator('.team-problems')).toBeHidden();
  await expect(bar('Open 1 session and apply')).toBeEnabled();
  await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
  log('ok the model menu, the card showing the model and its program, Apply reading "Open 1 session and apply"');

  // + New agent has no model: a problem, until one is chosen.
  await map.getByRole('button', { name: '+ New agent' }).click();
  await expect(map.locator('.team-card')).toHaveCount(4);
  await expect(byName('Agent').locator('.team-card-meta')).toHaveText('no session yet');
  await expect(bar('Open 2 sessions and apply')).toBeDisabled();
  await byName('Agent').locator('.team-card-name').click();
  await modelMenu.selectOption({ label: 'qwen-9 \u00b7 runs in OpenCode' });
  await expect(byName('Agent').locator('.team-card-meta')).toHaveText('qwen-9 \u00b7 via OpenCode');
  await agentBox.getByRole('button', { name: 'Close the agent box' }).click();
  await expect(bar('Open 2 sessions and apply')).toBeEnabled();

  // Apply opens the sessions one after another; the second cannot start: the first is kept, the card says why, nothing is lost.
  await page.evaluate(() => { window.__teamStub.failFor = 'Agent'; });
  await bar('Open 2 sessions and apply').click();
  await expect(byName('Agent').locator('.team-told')).toHaveText('opencode did not start.');
  await expect(map.locator('.team-status')).toContainText('Agent has no session: opencode did not start.');
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(1);
  await expect(byName('Local qwen')).not.toHaveAttribute('data-empty', '');
  await expect(bar('Open 1 session and apply')).toBeEnabled();
  expect(await page.evaluate(() => window.__teamStub.opened)).toEqual(['Local qwen']);
  expect(await page.evaluate(() => window.__teamStub.stored.revision)).toBe(1);
  log('ok Apply opened the first session, kept it, and named the agent whose session could not be opened');

  // The retry opens only the agent that has none.
  await page.evaluate(() => { window.__teamStub.failFor = null; });
  await bar('Open 1 session and apply').click();
  await expect(map.locator('.team-state')).toContainText('revision 2');
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(0);
  await expect(bar('Apply')).toBeDisabled();
  expect(await page.evaluate(() => window.__teamStub.opened)).toEqual(['Local qwen', 'Agent']);
  const applied = await t.calls('team_apply');
  expect(applied).toHaveLength(2);
  expect(applied[1].request.formation.agents.filter(a => !a.session).map(a => a.name)).toEqual(['Agent']);
  log('ok the retry opens only what is left');

  // Use over a team on the map asks first; Discard brings the applied team back.
  await bar('Templates ▾').click();
  const use = row('Review crew').getByRole('button', { name: 'Use Review crew' });
  await use.click();
  await expect(use).toHaveText('Replace the map?');
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(0);
  await use.click();
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(3);
  await bar('Discard').click();
  await expect(map.locator('.team-card[data-empty]')).toHaveCount(0);
  await expect(map.locator('.team-card')).toHaveCount(4);
  // And the template is deleted at last.
  await bar('Templates ▾').click();
  const del = row('Review crew').getByRole('button', { name: 'Delete Review crew' });
  await del.click(); await del.click();
  await expect(menu.locator('.team-template-empty')).toContainText('No template yet');
  await page.keyboard.press('Escape');
  log('ok Use over a team asks first, Discard brings it back, and the template is deleted');
}

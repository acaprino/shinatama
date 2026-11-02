/*
 * Interactive SAMPLE DATA for the accepted 2026-11-01 FRDs.
 * No real agent, filesystem operation, credential store or network request.
 * Companion: FRD-0012; routing: FRD-0015; optional TE: FRD-0016.
 */
(() => {
  'use strict';
  const $ = (selector, scope = document) => scope.querySelector(selector);
  const $$ = (selector, scope = document) => [...scope.querySelectorAll(selector)];
  const escape = value => String(value).replace(/[&<>"']/g, char => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[char]));
  const button = (label, attributes = '') => '<button class="fx-button" ' + (attributes.includes('type=') ? '' : 'type="button" ') + attributes + '>' + label + '</button>';
  const tag = (label, style = '') => '<span class="fx-tag ' + style + '">' + label + '</span>';
  const routes = { overview: 'Overview', projects: 'Projects', sessions: 'Sessions', tasks: 'Daodan tasks', roundtable: 'Roundtable', browser: 'Browser', agents: 'Agents & sets', launch: 'New instance', memory: 'Learning memory', settings: 'Settings' };
  const harnesses = ['Claude Code', 'Codex', 'OpenCode', 'Pi', 'Hermes Agent', 'DeepSeek Harness'];
  const options = (items, selected) => items.map(item => '<option' + (item === selected ? ' selected' : '') + '>' + escape(item) + '</option>').join('');
  const field = (label, control) => '<label class="fx-field">' + label + control + '</label>';
  const select = (name, items, selected, attributes = '') => '<select name="' + name + '" ' + attributes + '>' + options(items, selected) + '</select>';
  const state = {
    route: 'overview', settingsTab: 'companion', paused: false, quiz: false, web: 'on demand', memoryOn: true,
    cards: 'automatic', pending: 'smart', worker: 'Claude Code', companion: 'OpenCode', companionModel: 'Instance default',
    dailyCap: 2, cost: .12, cardSequence: 0, tokenEconomizer: false, roundRunning: false, roundStep: 0,
    memory: [
      { title: 'PTY', theme: 'Terminals', keywords: 'resize, shell', level: 2, date: '2026-11-01' },
      { title: 'SQLite transaction', theme: 'Storage', keywords: 'commit, atomicity', level: 2, date: '2026-11-01' },
      { title: 'Prompt cache', theme: 'Models', keywords: 'prefix, tokens', level: 1, date: '2026-11-02' }
    ],
    projects: ['shinatama', 'mukade', 'billion-context', 'daodan'], pins: new Set(['shinatama']),
    placementHarness: harnesses[0],
    agentPlacement: Object.fromEntries(harnesses.map(name => [name, { program: 'System installation', home: 'Private box' }]))
  };
  const settingsValues = {};
  let toastTimer;
  function notify(message) {
    if (!$('#toast')) return;
    $('#toast-text').textContent = message;
    $('#toast').classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => $('#toast').classList.remove('show'), 3200);
  }
  const modal = document.createElement('dialog');
  modal.className = 'fx-workbench';
  modal.setAttribute('aria-labelledby', 'fx-workbench-title');
  modal.innerHTML = '<header class="fx-workbench-head"><div><strong id="fx-workbench-title">Workspace</strong><small>Interactive mockup · sample data · 2 November 2026</small></div>' +
    button('Close', 'data-fx-close aria-label="Close workspace panel"') + '</header><div class="fx-workbench-body"><nav class="fx-workbench-nav" aria-label="Workspace features"></nav><main class="fx-route" id="fx-route"></main></div><footer class="fx-workbench-foot">Complete design preview, including later delivery slices. Changes last until reload. No real processes or requests.</footer>';
  document.body.append(modal);
  const routeView = $('#fx-route');
  let returnFocus = null;
  function openRoute(route) {
    if (route === 'learning') { showCompanion(); return; }
    if (!routes[route]) return;
    state.route = route; renderRoute();
    if (!modal.open) { returnFocus = document.activeElement; modal.showModal(); }
  }
  function closeRoute() { modal.close(); }
  modal.addEventListener('close', () => { if (returnFocus?.isConnected) returnFocus.focus(); rearmExposure(); });
  modal.addEventListener('click', event => { if (event.target === modal) closeRoute(); });

  let companionHost = $('.right-panel');
  const fullWorkspace = Boolean(companionHost);
  if (fullWorkspace) {
    const original = document.createElement('div');
    original.className = 'fx-thread-original';
    while (companionHost.firstChild) original.append(companionHost.firstChild);
    companionHost.append(original);
    companionHost.insertAdjacentHTML('afterbegin', '<div class="fx-inspector-switch">' + button('Companion', 'data-inspector="companion" aria-pressed="true"') + button('Thread', 'data-inspector="thread" aria-pressed="false"') + '</div>');
    original.hidden = true;
    companionHost.setAttribute('aria-label', 'Companion and thread inspector');
  } else {
    companionHost = document.createElement('aside'); companionHost.className = 'fx-drawer';
    companionHost.setAttribute('aria-label', 'Learning companion'); companionHost.hidden = true; document.body.append(companionHost);
  }
  const companion = document.createElement('section');
  companion.className = 'fx-companion';
  companion.setAttribute('aria-label', 'Read-only learning companion');
  companion.innerHTML = '<header class="fx-companion-head"><div class="fx-row spread"><strong>Learning companion</strong>' + button(fullWorkspace ? 'Collapse' : 'Close', 'data-companion-close') +
    '</div><p><span data-companion-provider>OpenCode</span> · <span data-companion-model>Instance default</span> ↳ <span data-worker-name>Claude Code</span> / T·8F3A</p>' +
    '<div class="fx-row">' + tag('read-only: harness policy only', 'warn') + tag('live feed · demo', 'good') + '</div>' +
    '<div class="fx-row" style="margin-top:8px">' + button('Settings', 'data-feature-open="settings"') + button('Memory', 'data-feature-open="memory"') + button('Pause feed', 'data-pause-feed aria-pressed="false"') +
    '</div><p><span data-companion-cost>$0.120 / $2.00 today</span> · worker: $0.48</p></header>' +
    '<div class="fx-companion-scroll"><div class="fx-request"><strong>Worker waiting for your answer</strong><br>Sample request: run a local verification command.<br>' + button('Expand request', 'data-preset="expand"') +
    '<div class="fx-small fx-muted">Answer in the worker terminal.</div></div><div data-card-list></div></div>' +
    '<footer class="fx-companion-foot"><div class="fx-row">' + button('Explain latest', 'data-preset="explain"') + button('Explain selection', 'data-explain-selection') +
    '</div><p style="margin:6px 0 0">Preset actions only · <span data-web-status>web: on demand (demo)</span></p><div class="fx-status" role="status" aria-live="polite" data-queue-status></div></footer>';
  companionHost.append(companion);
  if ($('.left-panel')) {
    const nav = document.createElement('nav'); nav.className = 'fx-nav'; nav.setAttribute('aria-label', 'Workspace features');
    nav.innerHTML = button('Learning companion', 'data-feature-open="learning"') + button('Overview & activity', 'data-feature-open="overview"') + button('Sessions & search', 'data-feature-open="sessions"') + button('Roundtable', 'data-feature-open="roundtable"') + button('Agents, tools & settings', 'data-feature-open="agents"');
    $('.left-panel').insertBefore(nav, $('.project-list'));
    if ($('.project-list')) $('.project-list').style.overflowY = 'auto';
  }
  function showCompanion() {
    if (modal.open) closeRoute();
    if (fullWorkspace) {
      $('#workspace').classList.remove('right-collapsed'); $('.fx-thread-original').hidden = true; companion.hidden = false;
      $$('[data-inspector]').forEach(item => item.setAttribute('aria-pressed', String(item.dataset.inspector === 'companion')));
    } else { companionHost.hidden = false; document.body.classList.add('fx-lite-companion-open'); }
    rearmExposure();
  }
  function closeCompanion() {
    if (fullWorkspace) $('#workspace').classList.add('right-collapsed');
    else { companionHost.hidden = true; document.body.classList.remove('fx-lite-companion-open'); }
  }
  function presets(concept) {
    return '<div class="fx-presets">' + [['basics', 'Basics'], ['why', 'Why here'], ['alternatives', 'Alternatives'], ['example', 'Example'], ['simpler', 'Simpler'], ['deeper', 'Deeper'], ['expand', 'Expand'], ['sources', 'Sources']]
      .map(([key, label]) => button(label, 'data-preset="' + key + '" data-concept="' + escape(concept) + '"' + (key === 'sources' && (state.web === 'off' || state.companion === 'Pi') ? ' hidden' : ''))).join('') + '</div>';
  }
  function learningBody() {
    return '<h3>A hop prepares a fresh conversation</h3><p>The worker is separating seed preparation from acknowledgement. The source stays writable until the destination confirms.</p>' +
      '<table><thead><tr><th>Sample file</th><th>Change</th></tr></thead><tbody><tr><td>src/hop.rs</td><td>Keep the source writable</td></tr><tr><td>tests/hop.rs</td><td>Check the failure path</td></tr></tbody></table>' +
      '<h4>Why this matters</h4><p>If the destination fails, your original conversation is still available. This is the companion’s inference from the sample changes.</p>' +
      '<h4>Two concepts</h4><p><strong>Seed:</strong> a compact handover of the goal, decisions and references. It starts a new native session.</p><p><strong>Acknowledgement:</strong> confirmation that the destination received the seed. Preparing it alone is not enough.</p>' +
      '<div class="fx-concepts">' + button('Seed · basics', 'data-preset="basics" data-concept="Seed"') + button('Acknowledgement', 'data-preset="why" data-concept="Acknowledgement"') + button('Transaction', 'data-preset="deeper" data-concept="SQLite transaction"') + '</div>';
  }
  function remember(title, level, lower = false) {
    if (!state.memoryOn) return;
    const entry = state.memory.find(item => item.title === title);
    if (entry) { entry.level = lower ? level : Math.max(level, entry.level); entry.date = '2026-11-02'; }
    else state.memory.push({ title, theme: 'General concepts', keywords: title.toLowerCase(), level, date: '2026-11-02' });
  }
  const exposureTimers = new Map();
  const pendingExposure = new Set();
  const exposureObserver = new IntersectionObserver(entries => entries.forEach(entry => {
    clearTimeout(exposureTimers.get(entry.target));
    if (!entry.isIntersecting || document.hidden) return;
    exposureTimers.set(entry.target, setTimeout(() => {
      if (document.hidden || companion.hidden || companionHost.hidden || modal.open || (fullWorkspace && $('#workspace').classList.contains('right-collapsed'))) return;
      entry.target._taught.forEach(item => remember(item.title, item.level, item.lower));
      $('[data-memory-receipt]', entry.target).textContent = state.memoryOn ? 'Recorded as explained · not a measure of understanding' : 'Memory is off';
      exposureObserver.unobserve(entry.target);
      pendingExposure.delete(entry.target);
      exposureTimers.delete(entry.target);
    }, 2000));
  }), { root: $('.fx-companion-scroll'), threshold: .1 });
  function trackExposure(card, taught) { if (taught.length) { card._taught = taught; pendingExposure.add(card); exposureObserver.observe(card); } }
  function rearmExposure() {
    pendingExposure.forEach(card => {
      clearTimeout(exposureTimers.get(card));
      exposureObserver.unobserve(card);
      if (!document.hidden && !modal.open) exposureObserver.observe(card);
    });
  }
  document.addEventListener('visibilitychange', rearmExposure);
  function appendCard(kind, body, taught = [], quiz = false, concept = 'Seed') {
    const card = document.createElement('article'); card.className = 'fx-card'; card.dataset.cardId = String(++state.cardSequence);
    card.innerHTML = '<header><span>' + kind + '</span><small>Sample · ' + (kind === 'Aided Decision' ? '$0.004' : '$0.003') + '</small></header>' +
      (quiz ? '<div class="fx-quiz"><strong>Predict first</strong><p>If preparing the destination fails, which session can you continue?</p>' + button('A · The original session', 'data-quiz-answer="A"') + button('B · Only the destination', 'data-quiz-answer="B"') + button('Not sure / skip', 'data-quiz-answer="skip"') + '</div>' : '') +
      '<div data-card-body' + (quiz ? ' hidden' : '') + '>' + body + presets(concept) + '<footer>Illustrative project design · no product code built.<br><span data-memory-receipt>' +
      (taught.length ? 'Memory updates after 2 seconds in view' : 'No learning-memory entry from this card') + '</span></footer></div>';
    $('[data-card-list]').append(card);
    if (quiz) card._pendingTaught = taught; else trackExposure(card, taught);
    return card;
  }
  appendCard('Learning', learningBody(), [{ title: 'Seed', level: 1 }, { title: 'Acknowledgement', level: 1 }]);
  function decisionBody() {
    return '<h3>What running the verification means</h3>' + tag('changes only the worktree', 'warn') +
      '<p>The worker proposes <code>cargo test</code>. Cargo is Rust’s build and test tool; it builds tests and checks their assertions.</p>' +
      '<h4>If you say yes</h4><p>The command runs and may write build files in <code>target/</code>. The result returns to the worker.</p>' +
      '<h4>If you say no</h4><p>The command does not run. The worker gets no new test evidence from this request.</p>' +
      '<details><summary>Example and why the worker asks</summary><p>A hop test checks that failed preparation leaves the source available. The harness asks permission to run a shell command.</p></details>' +
      '<h4>Check first</h4><p>Read the command and current folder in the worker. Tests run project code, including build scripts; the command alone does not prove their effects.</p>' +
      '<p class="fx-small fx-muted">General knowledge, not checked. The sample risk label assumes only local effects.</p><div class="fx-notice">Give your answer in the worker terminal. This card has no approval or reply action.</div>';
  }
  const presetQueue = []; let presetBusy = false;
  function queuePreset(action, concept = 'Seed', selection = '') {
    presetQueue.push({ action, concept, selection });
    $('[data-queue-status]').textContent = presetBusy ? 'Queued · ' + presetQueue.length + ' preset action(s)' : 'Preparing sample card…';
    runNextPreset();
  }
  function runNextPreset() {
    if (presetBusy || !presetQueue.length) return;
    presetBusy = true;
    const { action, concept, selection } = presetQueue.shift();
    setTimeout(() => {
      let body, taught = [], kind = 'Learning';
      const level = state.memoryOn ? state.memory.find(item => item.title === concept)?.level || 0 : 0;
      if (action === 'expand') { body = decisionBody(); kind = 'Aided Decision'; }
      else if (action === 'sources') {
        body = '<h3>Sources for the explanation</h3><p>A seed is a handover document, not a native-session migration.</p><div class="fx-sources"><p><strong>From the repository</strong><br>docs/frd/FRD-0005-threads-and-hop.md · seed-only v1</p>' +
          '<p><strong>From the web (search result only) · demo</strong><br><a href="https://doc.rust-lang.org/cargo/commands/cargo-test.html" target="_blank" rel="noopener noreferrer">Cargo test — doc.rust-lang.org</a></p></div>' +
          '<details><summary>Searches · simulated harness events</summary><p>“Cargo test official documentation” · 1 result · 0 page reads. No request was made by this mockup.</p></details>';
      } else if (action === 'explain') {
        body = learningBody(); taught = [{ title: 'Seed', level: 1 }, { title: 'Acknowledgement', level: 1 }];
      } else if (action === 'selection') {
        body = '<h3>Explain this selection</h3><blockquote>' + escape(selection) + '</blockquote><p>This excerpt is quoted data; the worker’s terminal is unchanged.</p><p>In this sample, acknowledgement is the boundary after which the source can be archived.</p>';
        taught = [{ title: 'Acknowledgement', level: 1 }];
      } else {
        const content = {
          basics: 'A seed is a short handover of the goal, decisions and references. It gives a new conversation enough context to continue.',
          why: 'Here, the seed is prepared before archiving the source. If the destination fails, the original agent remains available.',
          alternatives: 'Stay with the same agent to retain native state; close-source hop to continue one thread; fork-source hop to keep both threads live.',
          example: 'Sample seed: “Finish hop recovery; check src/hop.rs; let the destination ask its own permissions.” The new agent starts from this document.',
          simpler: 'A seed is a handover note. A new agent reads the note and continues the task.',
          deeper: 'Preparation and acknowledgement are separate states. Discarding failed preparation leaves the source available; confirmed receipt permits close-source completion.'
        };
        const labels = { basics: 'the basics', why: 'why here', alternatives: 'alternatives', example: 'worked example', simpler: 'from zero', deeper: 'one level deeper' };
        body = '<h3>' + escape(concept) + ' · ' + labels[action] + '</h3><p>' + (concept === 'Seed' ? content[action] : escape(concept) + ' helps reason about the handover boundary. This illustrative card demonstrates the requested depth, not an agent response.') + '</p>';
        if (action !== 'example') taught = [{ title: concept, level: ({ basics: 1, why: 2, alternatives: 3, simpler: Math.max(1, level - 1), deeper: Math.min(4, level + 1) })[action], lower: action === 'basics' || action === 'simpler' }];
      }
      appendCard(kind, body, taught, state.quiz && kind === 'Learning', action === 'selection' ? 'Acknowledgement' : concept);
      $('.fx-companion-scroll').scrollTop = $('.fx-companion-scroll').scrollHeight;
      state.cost += kind === 'Learning' ? .003 : .004; syncCompanion(); presetBusy = false;
      $('[data-queue-status]').textContent = presetQueue.length ? 'Queued · ' + presetQueue.length + ' preset action(s)' : 'Explanation ready · sample';
      runNextPreset();
    }, 350);
  }
  function syncCompanion() {
    $('[data-companion-provider]').textContent = state.companion; $('[data-worker-name]').textContent = state.worker;
    $('[data-companion-model]').textContent = state.companionModel;
    $('[data-companion-cost]').textContent = '$' + state.cost.toFixed(3) + ' / $' + Number(state.dailyCap).toFixed(2) + ' today';
    $('[data-pause-feed]').textContent = state.paused ? 'Resume feed' : 'Pause feed';
    $('[data-pause-feed]').setAttribute('aria-pressed', String(state.paused));
    $$('.fx-companion-head .fx-tag')[1].textContent = state.paused ? 'paused' : state.worker === 'Hermes Agent' ? 'partial feed' : state.worker === 'DeepSeek Harness' ? 'ACP feed · unverified' : 'live feed · demo';
    $('[data-web-status]').textContent = state.companion === 'Pi' ? 'web: unavailable on Pi' : 'web: ' + state.web + ' (demo)';
    $$('[data-preset="sources"]', companion).forEach(item => { item.hidden = state.web === 'off' || state.companion === 'Pi'; });
  }
  function heading(title, lead) { return '<h2>' + title + '</h2><p class="fx-lead">' + lead + '</p>'; }
  function overviewView() {
    return heading('Your workspace, at a glance', 'Two workers, one companion. Activity, costs and capabilities are sample data.') +
      '<div class="fx-row" style="margin-bottom:16px">' + tag('1 working', 'good') + tag('1 waiting for you', 'warn') + tag('1 explanation ready') + '</div>' +
      '<div class="fx-table-wrap"><table class="fx-table"><thead><tr><th>Instance / activity</th><th>Project / branch</th><th>State</th><th>Usage today</th><th>Action</th></tr></thead><tbody>' +
      '<tr><td>Claude Code<small>Prepare hop boundary · default model</small></td><td>shinatama<small>main / shared worktree</small></td><td>' + tag('working', 'good') + '</td><td>$0.48<small>76k input · context 38%</small></td><td>' + button('Focus', 'data-focus-worker="claude"') + '</td></tr>' +
      '<tr class="fx-subrow"><td>↳ Learning companion<small>OpenCode · read-only preview</small></td><td>T·8F3A<small>Attached to thread</small></td><td>' + tag('explanation ready') + '</td><td>$' + state.cost.toFixed(3) + '<small>Not a collaborator</small></td><td>' + button('Open', 'data-feature-open="learning"') + '</td></tr>' +
      '<tr><td>Codex<small>Verify hop recovery · default model</small></td><td>shinatama<small>main / shared worktree</small></td><td>' + tag('permission', 'warn') + '</td><td>$0.23<small>34k input · context 17%</small></td><td>' + button('Focus', 'data-focus-worker="codex"') + '</td></tr></tbody></table></div>' +
      '<div class="fx-notice" style="margin:16px 0">Possible collision: both workers share a worktree and mention src/hop.rs. Focus a worker to coordinate. A linked worktree is an option when launching a new instance.</div>' +
      '<div class="fx-split"><section class="fx-section"><h3>Activity & message flow</h3><p class="fx-small">Now · Codex requests permission<br>20s ago · Companion explanation ready<br>1m ago · Claude changed sample files<br>2m ago · Claude → Codex: verification request</p>' +
      '<p class="fx-small fx-muted">Native event states: working, idle, waiting_permission, waiting_question, unknown. Summaries can be paused independently.</p>' + button('Pause summaries', 'data-pause-summaries aria-pressed="false"') + '</section>' +
      '<section class="fx-section"><h3>Desktop widget preview</h3><div class="fx-widget"><span class="fx-widget-mark">S</span><div><strong>Codex needs your answer</strong><br><span class="fx-small fx-muted">shinatama · verification</span></div></div><div class="fx-row" style="margin-top:12px">' +
      button('Focus worker', 'data-focus-worker="codex"') + button('Widget settings', 'data-feature-open="settings" data-settings-tab="appearance"') + '</div></section></div>';
  }
  function projectsView() {
    return heading('Projects', 'Pinned and recent folders across configured roots. Ordered by last use in the mockup.') +
      '<div class="fx-split"><section>' + field('Filter projects', '<input data-filter="projects" placeholder="Type a project name…" aria-label="Filter projects">') +
      '<div class="fx-results">' + state.projects.map((name, index) => '<article class="fx-result" data-search-text="' + escape(name.toLowerCase()) + '"><div class="fx-row spread"><h3>' + escape(name) + '</h3>' +
      button(state.pins.has(name) ? 'Unpin' : 'Pin', 'data-pin="' + escape(name) + '" aria-pressed="' + state.pins.has(name) + '"') + '</div><p>D:/Projects/' + escape(name) + ' · ' + (index ? 'clean' : 'main · 3 modified · 2 workers') +
      '</p><div class="fx-row">' + button('Threads', 'data-feature-open="sessions"') + button('Tasks', 'data-feature-open="tasks"') + button('New instance', 'data-feature-open="launch"') + '</div></article>').join('') +
      '<p class="fx-empty" data-project-empty hidden>No matching projects.</p></div></section><div class="fx-stack"><section class="fx-section"><h3>Create a project</h3><form class="fx-form" data-form="project">' +
      field('Project name', '<input name="name" required maxlength="80" placeholder="new-project">') +
      '<label class="fx-check"><input type="checkbox" name="git" checked>Initialize a Git repository</label><label class="fx-check"><input type="checkbox" name="daodan">Prepare Daodan’s init command</label>' +
      '<div>' + button('Create sample project', 'type="submit"') + '</div><div class="fx-status" role="status" data-form-status></div></form></section>' +
      '<section class="fx-section"><h3>Project roots</h3><p class="fx-small">D:/Projects · default<br>E:/Work · unavailable drive</p>' + field('Open another folder (F3 preview)', '<input id="fx-folder" placeholder="D:/Other/project">') +
      '<div class="fx-row" style="margin-top:8px">' + button('Open sample folder', 'data-folder-open') + '</div><p class="fx-small fx-muted">The desktop app uses the native folder picker. No folder is accessed here.</p>' +
      '<details><summary>Recent external folders · last 10</summary><p class="fx-small">D:/Experiments/demo<br>D:/Desktop/scratch</p></details></section></div></div>';
  }
  const sessions = [
    { title: 'Resumable hop boundary', project: 'shinatama', agent: 'Claude Code', date: '2026-11-02', kind: 'Worker', detail: 'Native session available · T·8F3A', available: true },
    { title: 'PTY resize under nested splits', project: 'shinatama', agent: 'Codex', date: '2026-11-02', kind: 'Worker', detail: 'Native session available · T·71B2', available: true },
    { title: 'Hop acknowledgement explained', project: 'shinatama', agent: 'OpenCode', date: '2026-11-02', kind: 'Companion', detail: 'Companion record · linked to T·8F3A · not a hop segment', available: false },
    { title: 'Native handover reader', project: 'mukade', agent: 'Pi', date: '2026-10-30', kind: 'Worker', detail: 'Native session aged out · text index retained', available: false }
  ];
  function sessionsView() {
    return heading('Sessions & search', 'Read-only history across supported native stores and Shinatama’s companion records.') +
      '<div class="fx-row" style="margin-bottom:14px">' + button('Import .stx · preview', 'data-session-transfer="Import"') +
      button('Export .stx · preview', 'data-session-transfer="Export"') + tag('Portable transcript · no credentials') + '</div>' +
      '<div class="fx-status" data-transfer-status role="status"></div><div class="fx-stack">' + field('Search text', '<input data-session-filter placeholder="Search titles, messages or files…" aria-label="Search sessions">') +
      '<div class="fx-row">' + field('Harness', select('agent', ['All harnesses', ...harnesses], '', 'data-session-agent')) +
      field('Project', select('project', ['All projects', 'shinatama', 'mukade'], '', 'data-session-project')) +
      field('Type', select('kind', ['All types', 'Worker', 'Companion'], '', 'data-session-kind')) + field('Since', '<input type="date" data-session-date>') + '</div></div>' +
      '<div class="fx-results">' + sessions.map((session, index) => '<article class="fx-result" data-session-index="' + index + '"><h3>' + session.title + '</h3><p>' + session.project + ' · ' + session.agent + ' · ' + session.date + '<br>' + session.detail +
      '</p><div class="fx-row">' + tag(session.kind) + button('Preview', 'data-session-preview="' + index + '"') + (session.kind === 'Companion' ? '' : button(session.available ? 'Resume same harness' : 'New session from seed', 'data-session-resume="' + index + '"') + button('Seed another harness', 'data-feature-open="launch"')) +
      '</div></article>').join('') + '<p class="fx-empty" data-sessions-empty hidden>No sessions match these filters.</p></div>' +
      '<div class="fx-notice" style="margin-top:16px">Hermes and DeepSeek have no searchable native reader at the current Mukade pin, so cannot be hop sources. Retained text can seed a new conversation, but cannot resume an expired native session.</div>' +
      '<section class="fx-section" data-session-preview-panel style="margin-top:14px" hidden></section>';
  }
  function tasksView() {
    return heading('Daodan tasks', 'Read-only native task state. Commands are prepared for the agent, never sent automatically.') +
      '<div class="fx-table-wrap"><table class="fx-table"><thead><tr><th>Objective</th><th>Phase / waves</th><th>Questions</th><th>Holder / worktree</th><th>Action</th></tr></thead><tbody>' +
      '<tr><td>HOP-142<small>Resumable transaction boundary</small></td><td>Implementation<small>2 / 4 sealed</small></td><td>1 pending</td><td>Claude Code<small>main · T·8F3A</small></td><td>' + button('Focus thread', 'data-focus-worker="claude"') + '</td></tr>' +
      '<tr><td>PTY-061<small>Nested split resize</small></td><td>Verification<small>3 / 3 sealed</small></td><td>0</td><td>Codex<small>worktrees/pty-resize · T·71B2</small></td><td>' + button('Focus thread', 'data-focus-worker="codex"') + '</td></tr>' +
      '<tr><td>SC-014<small>Sidecar recovery</small></td><td>Planning<small>0 / 2 sealed</small></td><td>0</td><td>Unassigned<small>worktrees/sidecar</small></td><td>' + button('Start a thread', 'data-feature-open="launch"') + '</td></tr></tbody></table></div>' +
      '<section class="fx-section" style="margin-top:18px"><h3>Daodan onboarding · new project preview</h3><p class="fx-small fx-muted">One offer per project. A newly created project gets its init offer only once.</p><div class="fx-row" data-onboarding>' +
      button('Onboard existing project', 'data-onboard="onboard"') + button('Initialize project', 'data-onboard="init"') + button('Not now', 'data-onboard="dismiss"') + '</div><p class="fx-status" role="status" data-onboarding-status></p></section>';
  }
  function roundtableView() {
    return heading('Roundtable', 'Moderated proposals, critique and synthesis. Answer-only by default.') +
      '<form class="fx-form" data-form="roundtable">' + field('Topic', '<textarea name="topic" required>How should a failed hop preserve the source conversation?</textarea>') +
      '<div class="fx-split"><section class="fx-section fx-form"><h3>Participants</h3>' + field('First participant', select('first', harnesses)) +
      field('First model', select('firstModel', ['Instance default', 'Local model · sample', 'Other configured model · sample'])) +
      field('Second participant', select('second', harnesses, 'Codex')) + field('Second model', select('secondModel', ['Instance default', 'Local model · sample', 'Other configured model · sample'])) +
      '<p class="fx-small fx-muted">Duplicate harnesses and local models are allowed. Each participant keeps its chosen model.</p>' +
      field('Tools for first participant', select('tools', ['Answer-only', 'Read-only tools'])) +
      field('Tools for second participant', select('secondTools', ['Answer-only', 'Read-only tools'])) + '</section><section class="fx-section fx-form"><h3>Limits</h3>' +
      field('Cost cap ($)', '<input name="cap" type="number" min=".01" step=".01" value="2" required>') + field('Token cap', '<input name="tokens" type="number" min="1000" value="40000" required>') +
      '<p class="fx-small fx-muted">No model calls. “Next round” advances the sample transcript.</p></section></div><div class="fx-row">' +
      button('Start sample discussion', 'type="submit"') + button('Next round', 'data-round-next disabled') + button('Stop', 'data-round-stop disabled') + '</div></form>' +
      '<div class="fx-results" data-round-transcript></div><div class="fx-status" data-round-status role="status"></div>' +
      '<section class="fx-section" style="margin-top:18px"><h3>Interject</h3>' + field('Add a constraint', '<input data-round-interjection placeholder="For example: preserve the original worktree">') +
      '<div style="margin-top:8px">' + button('Add intervention', 'data-round-interject') + '</div></section>' +
      '<section class="fx-section" style="margin-top:14px"><h3>Messages between workers</h3><p class="fx-small">Claude → Codex: “Check the acknowledgement boundary.”</p><div class="fx-row">' + tag('delivered · native queue', 'good') + tag('not acknowledged yet') +
      '</div><p class="fx-small fx-muted">Delivered does not mean read. Peer messages never approve a worker’s permission request.</p></section>';
  }
  function browserView() {
    return heading('Browser sessions', 'Moli reads fast; Chrome / Patchright opens only through an explicit UI action.') +
      '<div class="fx-split"><div class="fx-stack"><section class="fx-section"><h3>Read / preview</h3>' + field('Public URL', '<input id="fx-browser-url" type="url" value="https://doc.rust-lang.org">') +
      '<div style="margin-top:10px">' + button('Read sample page', 'data-browser-read') + '</div><p class="fx-small fx-muted">Moli · static preview. No URL is fetched.</p></section>' +
      '<section class="fx-section"><h3>Interactive browser</h3><p class="fx-small">A failed read never opens a visible browser automatically.</p>' + button('Open UI · demo', 'data-browser-open') +
      '<div class="fx-status" data-browser-status role="status"></div></section></div><section class="fx-section"><h3>Static preview</h3><div class="fx-browser-preview">' + tag('sample document') +
      '<h4>Rust documentation</h4><p class="fx-small fx-muted">Language guide, standard library and Cargo documentation.</p></div><dl class="fx-small"><dt>Engine / operation</dt><dd data-browser-engine>Moli / preview</dd><dt>Scope / controller</dt><dd>this instance / Claude Code</dd><dt>Reason</dt><dd data-browser-reason>Read official documentation</dd></dl></section></div>';
  }
  function agentsView() {
    const placement = state.agentPlacement[state.placementHarness];
    return heading('Agents, accounts & sets', 'Program and home are configured per agent. Each instance inherits them and chooses its own tool set.') +
      '<div class="fx-table-wrap"><table class="fx-table"><thead><tr><th>Harness</th><th>Account · sample</th><th>Surface / capability</th><th>Action</th></tr></thead><tbody>' +
      harnesses.map((name, index) => '<tr><td>' + name + '</td><td>' + tag(index === 4 ? 'signed out' : index === 5 ? 'unknown' : 'signed in', index < 4 ? 'good' : 'warn') + '</td><td>' +
      (index === 5 ? 'Native chat / ACP · no session reader' : index === 4 ? 'Terminal · partial companion feed' : 'Terminal · reader capability gated') + '</td><td>' +
      button(index < 4 ? 'New instance' : 'Native login', index < 4 ? 'data-feature-open="launch"' : 'data-native-login="' + escape(name) + '"') + '</td></tr>').join('') +
      '</tbody></table></div><div class="fx-split" style="margin-top:18px"><section class="fx-section fx-form"><h3>Per-instance set</h3><p class="fx-small">Daodan is preinstalled. The set chooses installed plugins, skills, hooks and MCP servers.</p>' +
      field('Set', select('set', ['Development', 'Read-only companion', 'Minimal'])) +
      '<label class="fx-check"><input type="checkbox" checked>Daodan skills and task commands</label><label class="fx-check"><input type="checkbox" checked>Shinatama messaging · workers only</label><label class="fx-check"><input type="checkbox" checked>BillionContext recovery tools · always preloaded</label>' +
      '<p class="fx-small fx-muted">Companions have a separate read-only set, without worker messaging or approval channels.</p></section>' +
      '<form class="fx-section fx-form" data-form="agent-placement"><h3>Program & home · per agent</h3>' +
      field('Agent to configure', select('harness', harnesses, state.placementHarness, 'data-agent-placement-harness')) +
      field('Program', select('program', ['System installation', 'Shinatama-owned copy'], placement.program, 'data-agent-placement')) +
      field('Home', select('home', ['Private box', 'System home'], placement.home, 'data-agent-placement')) +
      '<p class="fx-small fx-muted">Applies to subsequent launches of this agent. Existing instances and sessions stay where they are; no files move and no process starts. Owned program + system home is prohibited.</p>' +
      '<div>' + button('Save agent configuration · demo', 'type="submit"') + '</div><div class="fx-status" data-form-status role="status"></div><div class="fx-row">' +
      button('Review a new launch', 'data-feature-open="launch"') + button('Token savings', 'data-feature-open="settings" data-settings-tab="tokens"') +
      '</div><p class="fx-small fx-muted">Account login belongs to the harness. Shinatama does not collect its credentials.</p></form></div>';
  }
  function launchView() {
    const placement = state.agentPlacement[harnesses[0]];
    return heading('Start a new instance', 'Inherit the agent’s program and home; choose tools for this instance. Routing selects once; the conversation keeps its model.') +
      '<form class="fx-form" data-form="launch"><div class="fx-split"><div class="fx-stack">' +
      field('Harness', select('harness', harnesses, '', 'data-launch-harness')) + field('Project', select('project', state.projects)) +
      field('Optional task for model routing', '<textarea name="task" maxlength="1000" placeholder="Describe the work before starting…"></textarea>') +
      field('Model choice', select('routing', ['Automatic · decision model', 'Instance default'])) +
      '<p class="fx-small fx-muted">Low confidence or router failure keeps the instance default. No switching between turns.</p></div><section class="fx-section fx-form"><h3>Placement & tools</h3>' +
      field('Program · inherited from agent', '<input name="program" value="' + escape(placement.program) + '" readonly>') +
      field('Home · inherited from agent', '<input name="home" value="' + escape(placement.home) + '" readonly>') +
      '<div>' + button('Configure program & home per agent', 'data-feature-open="agents"') + '</div>' +
      field('Set', select('set', ['Development · Daodan preinstalled', 'Minimal', 'Read-only'])) +
      '<label class="fx-check"><input name="worktree" type="checkbox">Create a linked worktree</label><label class="fx-check"><input name="te" data-launch-te type="checkbox"' + (state.tokenEconomizer ? ' checked' : '') +
      '>TokenEconomizer · optional, off by default</label><p class="fx-small fx-muted" data-launch-note>Version gates must pass. Exclusions apply at the next launch only. Recovery tools stay preloaded.</p></section></div>' +
      '<div class="fx-notice">Real launches check native login, adapter capabilities and sidecar health. This is a simulated review.</div><div>' + button('Review launch', 'type="submit"') +
      '</div><div class="fx-status" data-launch-result role="status"></div></form>';
  }
  function memoryView() {
    return heading('What has been explained', 'Your personal ledger, shared across projects. It records exposure, not understanding. No explanation text or code is stored.') +
      '<div class="fx-row spread"><label class="fx-check"><input type="checkbox" data-memory-enabled' + (state.memoryOn ? ' checked' : '') + '>Use learning memory</label>' + button('Forget all…', 'data-forget-request') +
      '</div><div class="fx-notice" data-forget-confirm hidden style="margin-top:12px">Remove all sample concepts? Existing cards remain.<div class="fx-row" style="margin-top:8px">' + button('Forget all concepts', 'data-forget-confirmed') + button('Cancel', 'data-forget-cancel') +
      '</div></div><div class="fx-table-wrap" style="margin-top:16px"><table class="fx-table"><thead><tr><th>Concept / theme</th><th>Keywords</th><th>Level</th><th>Last explained</th><th>Action</th></tr></thead><tbody>' +
      state.memory.map((entry, index) => '<tr><td>' + escape(entry.title) + '<small>' + escape(entry.theme) + '</small></td><td>' + escape(entry.keywords) + '</td><td>' + entry.level + ' / 4</td><td>' + entry.date +
      '</td><td><div class="fx-row">' + button('Lower', 'data-memory-lower="' + index + '"' + (entry.level === 1 ? ' disabled' : '')) + button('Delete', 'data-memory-delete="' + index + '"') + '</div></td></tr>').join('') +
      '</tbody></table></div>' + (!state.memory.length ? '<p class="fx-empty">No concepts recorded. Open Learning to view a card.</p>' : '') +
      '<section class="fx-section" style="margin-top:18px"><h3>Four levels</h3><p class="fx-small">1 · What it is and which problem it solves<br>2 · How it is used and why here<br>3 · Alternatives and trade-offs<br>4 · How it works inside</p>' +
      '<p class="fx-small fx-muted">Relevant titles, themes, keywords and levels go only to the companion provider. Lowering and forgetting affect the next card. Basics and Simpler can lower the recorded level.</p></section>';
  }
  function settingsView() {
    const tabs = { companion: 'Learning', routing: 'Model routing', tokens: 'Token savings', appearance: 'Appearance & privacy' };
    let content;
    if (state.settingsTab === 'companion') {
      content = '<form class="fx-form" data-form="companion"><div class="fx-split"><div class="fx-stack">' +
      field('Companion harness', select('companion', harnesses, state.companion)) +
      field('Companion model', select('companionModel', ['Instance default', 'Local model · sample', 'Other configured model · sample'], state.companionModel)) +
      field('Worker feed to demonstrate', select('worker', harnesses, state.worker)) +
      field('Turn cards', select('cards', ['automatic', 'on demand'], state.cards)) + field('Pending requests', select('pending', ['smart', 'all', 'none'], state.pending)) +
      field('Web grounding', select('web', ['off', 'on demand', 'auto'], state.web)) + '</div><div class="fx-stack">' +
      field('Daily companion cap ($)', '<input name="cap" type="number" min=".01" step=".01" required value="' + state.dailyCap + '">') +
      '<label class="fx-check"><input name="quiz" type="checkbox"' + (state.quiz ? ' checked' : '') + '>Quiz first · next Learning card</label>' +
      '<label class="fx-check"><input name="memory" type="checkbox"' + (state.memoryOn ? ' checked' : '') + '>Use personal learning memory</label>' +
      field('Your level · settings only', '<textarea name="level" maxlength="500" placeholder="Explain Rust concepts from the basics"></textarea>') +
      '<p class="fx-small fx-muted">Level text: max 500 characters. Web hosting and confinement are unverified; these are previews. Pi has no built-in web.</p></div></div><div>' +
      button('Save sample settings', 'type="submit"') + '</div><div class="fx-status" data-form-status role="status"></div></form>';
    } else if (state.settingsTab === 'routing') {
      content = '<form class="fx-form" data-form="routing"><div class="fx-split"><div class="fx-stack">' +
      field('Mode', select('mode', ['choice', 'signals', 'off'])) + field('Decision executor', select('executor', ['julia1 · local CPU · no key', 'Hosted decision model · opt-in'])) +
      field('Endpoint', '<input name="endpoint" placeholder="Local sidecar / decision endpoint">') +
      field('Wire', select('wire', ['System One /v1/systemone', 'OpenRouter /v1/decisions'])) + field('Decision model', '<input name="model" value="julia1">') +
      '</div><div class="fx-stack">' + field('Minimum confidence · sample default', '<input name="confidence" type="number" min="0" max="1" step=".01" value=".8" required>') +
      field('Maximum reject probability · sample default', '<input name="reject" type="number" min="0" max="1" step=".01" value=".2" required>') +
      field('Timeout (ms) · sample default', '<input name="timeout" type="number" min="1" value="5000" required>') +
      field('OS vault reference · no key entry', '<input name="vault" placeholder="router/decision-key">') +
      '<label class="fx-check"><input name="consent" type="checkbox">Allow hosted selection to receive clipped, redacted digests and candidate cards</label></div></div>' +
      '<section class="fx-section"><h3>Same selector, two jobs</h3><p class="fx-small">Model routing: at session start, eligible sub-agent spawn and bookkeeping. Default model on low confidence or failure.</p>' +
      '<p class="fx-small">Tool chooser: per prompt, preload deferred tools without changing the cached prefix. Claude Code early-access seam; other harnesses await a supported seam.</p><p class="fx-small fx-muted">No net savings are claimed.</p></section><div>' +
      button('Save routing preview', 'type="submit"') + '</div><div class="fx-status" data-form-status role="status"></div></form>';
    } else if (state.settingsTab === 'tokens') {
      content = '<div class="fx-stack"><section class="fx-section"><h3>BillionContext sidecar</h3><div class="fx-row">' + tag('online · sample', 'good') + tag('single supervised proxy') +
      '</div><p class="fx-small">Shared context compaction and recovery. Recovery tools always stay preloaded; coverage is verified per harness.</p></section>' +
      '<section class="fx-section"><h3>Decision compaction · julia1</h3><p class="fx-small">Local CPU executor. Compaction decisions and digests stay local; hosted model selection has a separate opt-in.</p></section>' +
      '<section class="fx-section"><h3>TokenEconomizer</h3><label class="fx-check"><input type="checkbox" data-te-enabled' + (state.tokenEconomizer ? ' checked' : '') +
      '>Enable for the Development set · off by default</label><p class="fx-small">Repository evidence trims unused tools between sessions. Changes apply at the next launch after version gates pass. Hermes and DeepSeek are excluded.</p>' +
      '<table class="fx-table"><thead><tr><th>Tool · sample evidence</th><th>Next launch</th><th>Action</th></tr></thead><tbody><tr><td>unused_demo_tool<small>0 sample calls</small></td><td data-tool-exclusion>' +
      (state.tokenEconomizer ? 'Excluded in preview' : 'Unchanged · module off') + '</td><td>' + button('Restore tool', 'data-restore-tool') + '</td></tr><tr><td>mcp__bili__*<small>Recovery tools</small></td><td>Always preloaded</td><td>Protected</td></tr></tbody></table>' +
      '<p class="fx-small fx-muted">No savings percentage: pruning and net savings still need end-to-end evidence.</p></section></div>';
    } else {
      content = '<div class="fx-split"><form class="fx-form" data-form="appearance">' + field('Interface language', select('language', ['English', 'Italiano'])) +
      field('Terminal profile', select('profile', ['Cascadia Mono · 12 pt', 'High contrast · 14 pt', 'Compact · 11 pt'])) +
      field('Desktop widget', select('widget', ['Enabled · sample', 'Muted', 'Disabled'])) +
      '<label class="fx-check"><input type="checkbox" checked>Respect reduced motion</label><label class="fx-check"><input type="checkbox">Exclude thread from summaries and companions</label><div>' +
      button('Save appearance preview', 'type="submit"') + '</div><div class="fx-status" data-form-status role="status"></div></form>' +
      '<section class="fx-section"><h3>Local state & privacy</h3><p class="fx-small">Roots, pins, history and learning memory stay outside repositories. Companion records are separate from hop lineage.</p><p class="fx-small">The companion can read allowed project files. Relevant memory goes only to its provider. Reported web searches are shown.</p>' +
      '<h3>Distribution & updates</h3><p class="fx-small">Native Rust desktop design. The app, owned harness programs and plugins have separate update lifecycles.</p>' + button('Preview update check', 'data-update-preview') + '</section></div>';
    }
    return heading('Settings', 'Complete-design configuration preview. Values last only for this page session.') + '<div class="fx-tabs">' +
      Object.entries(tabs).map(([key, label]) => button(label, 'data-settings-tab="' + key + '" aria-pressed="' + (state.settingsTab === key) + '"')).join('') + '</div>' + content;
  }
  const views = { overview: overviewView, projects: projectsView, sessions: sessionsView, tasks: tasksView, roundtable: roundtableView, browser: browserView, agents: agentsView, launch: launchView, memory: memoryView, settings: settingsView };
  function renderRoute() {
    const navigation = $('.fx-workbench-nav');
    if (!navigation.children.length) navigation.innerHTML = Object.entries(routes).map(([key, label]) => '<button type="button" data-feature-open="' + key + '">' + label + '</button>').join('');
    $$('button', navigation).forEach(item => {
      if (item.dataset.featureOpen === state.route) item.setAttribute('aria-current', 'page');
      else item.removeAttribute('aria-current');
    });
    const contentHadFocus = routeView.contains(document.activeElement);
    routeView.innerHTML = views[state.route](); routeView.scrollTop = 0;
    if (contentHadFocus) { $('h2', routeView).tabIndex = -1; $('h2', routeView).focus(); }
    const form = $('form[data-form="' + state.settingsTab + '"]', routeView);
    const saved = settingsValues[state.settingsTab];
    if (saved && form) Object.entries(saved).forEach(([name, value]) => {
      const input = form.elements.namedItem(name);
      if (input) { if (input.type === 'checkbox') input.checked = value; else input.value = value; }
    });
    if (state.route === 'launch') validatePlacement($('[data-form="launch"]', routeView));
  }
  function focusWorker(name) {
    closeRoute();
    const pane = $('[data-pane="' + name + '"]') || $('[data-tile="' + (name === 'claude' ? 'A' : 'B') + '"]');
    if (pane) {
      $$('.pane, .tile').forEach(item => item.classList.remove('is-focused'));
      pane.classList.add('is-focused');
      const tab = $('[role="tab"]', pane);
      if (tab) { tab.click(); tab.focus(); }
    }
    notify('Focused ' + (name === 'claude' ? 'Claude Code' : 'Codex') + ' · sample worker');
  }
  function filterSessions() {
    const query = $('[data-session-filter]').value.toLowerCase(), agent = $('[data-session-agent]').value;
    const project = $('[data-session-project]').value, kind = $('[data-session-kind]').value, date = $('[data-session-date]').value;
    let count = 0;
    $$('[data-session-index]').forEach(row => {
      const session = sessions[Number(row.dataset.sessionIndex)];
      row.hidden = !((session.title + ' ' + session.project + ' ' + session.detail).toLowerCase().includes(query) &&
        (agent === 'All harnesses' || session.agent === agent) && (project === 'All projects' || session.project === project) &&
        (kind === 'All types' || session.kind === kind) && (!date || session.date >= date));
      if (!row.hidden) count++;
    });
    $('[data-sessions-empty]').hidden = count > 0;
  }
  function validatePlacement(form) {
    const placement = state.agentPlacement[form.elements.harness.value];
    form.elements.program.value = placement.program;
    form.elements.home.value = placement.home;
    const invalid = form.elements.program.value === 'Shinatama-owned copy' && form.elements.home.value === 'System home';
    const te = $('[data-launch-te]', form), unavailable = ['Hermes Agent', 'DeepSeek Harness'].includes(form.elements.harness.value);
    te.disabled = unavailable; if (unavailable) te.checked = false;
    $('[data-launch-note]').textContent = invalid ? 'Prohibited combination: use a private box with a Shinatama-owned program.' : unavailable ?
      'TokenEconomizer is unavailable on this harness. Native-session search and hop sources are unavailable at the current reader pin.' :
      'Version gates must pass. Exclusions apply at the next launch only; recovery tools stay preloaded.';
    $('button[type="submit"]', form).disabled = invalid;
    return !invalid;
  }
  function validateAgentPlacement(form) {
    const invalid = form.elements.program.value === 'Shinatama-owned copy' && form.elements.home.value === 'System home';
    $('[data-form-status]', form).textContent = invalid ? 'Prohibited combination: use a private box with a Shinatama-owned program.' : '';
    $('button[type="submit"]', form).disabled = invalid;
    return !invalid;
  }
  function advanceRound() {
    if (!state.roundRunning) return;
    const rounds = [
      ['Proposals', 'A proposes preserving the source until destination acknowledgement. B proposes testing failed preparation first.'],
      ['Critique', 'Both examine the gap between preparing a seed and confirming receipt. Neither participant changes files.'],
      ['Synthesis', 'Sample plan: prepare the seed, confirm receipt, then archive only in close-source mode. Keep the source on failure.']
    ];
    const [title, text] = rounds[state.roundStep];
    const usedTokens = (state.roundStep + 1) * 800, usedCost = (state.roundStep + 1) * .04;
    if (usedTokens > state.roundTokenCap || usedCost > state.roundCostCap) {
      state.roundRunning = false; $('[data-round-next]').disabled = true; $('[data-round-stop]').disabled = true;
      $('[data-round-status]').textContent = 'Sample budget cap reached. Discussion stopped before the next round.'; return;
    }
    $('[data-round-transcript]').insertAdjacentHTML('beforeend', '<article class="fx-result"><h3>' + title + '</h3><p>' + text + '</p>' +
      (state.roundStep === 2 ? button('Continue from this plan · seed', 'data-feature-open="launch"') : '') + '</article>');
    state.roundStep++;
    $('[data-round-status]').textContent = 'Round ' + state.roundStep + ' / 3 · sample usage $' + usedCost.toFixed(2) + ' · ' + usedTokens + ' tokens';
    if (state.roundStep === 3) { state.roundRunning = false; $('[data-round-next]').disabled = true; $('[data-round-stop]').disabled = true; }
  }
  document.addEventListener('click', event => {
    const target = event.target.closest('button, [data-feature-open]'); if (!target) return;
    if (target.matches('[data-feature-open]')) {
      if (target.dataset.settingsTab) state.settingsTab = target.dataset.settingsTab;
      openRoute(target.dataset.featureOpen);
    } else if (target.matches('[data-fx-close]')) closeRoute();
    else if (target.matches('[data-companion-close]')) closeCompanion();
    else if (target.matches('[data-inspector]')) {
      const thread = target.dataset.inspector === 'thread';
      $('.fx-thread-original').hidden = !thread; companion.hidden = thread;
      $$('[data-inspector]').forEach(item => item.setAttribute('aria-pressed', String(item === target)));
    } else if (target.matches('[data-pause-feed]')) { state.paused = !state.paused; syncCompanion(); }
    else if (target.matches('[data-preset]')) {
      if (target.dataset.preset === 'sources' && (state.web === 'off' || state.companion === 'Pi')) return;
      showCompanion(); queuePreset(target.dataset.preset, target.dataset.concept || 'Seed');
    } else if (target.matches('[data-explain-selection]')) {
      const selection = window.getSelection()?.toString().trim().slice(0, 4096);
      if (selection) queuePreset('selection', 'Seed', selection); else notify('Select text in a worker terminal, then choose Explain selection.');
    } else if (target.matches('[data-quiz-answer]')) {
      const card = target.closest('.fx-card');
      target.closest('.fx-quiz').innerHTML = '<p>You picked ' + escape(target.dataset.quizAnswer === 'skip' ? '“not sure”' : target.dataset.quizAnswer) + '; the companion expected A. This is the companion’s expectation.</p>';
      $('[data-card-body]', card).hidden = false; trackExposure(card, card._pendingTaught || []);
    } else if (target.matches('[data-settings-tab]')) { state.settingsTab = target.dataset.settingsTab; renderRoute(); }
    else if (target.matches('[data-focus-worker]')) focusWorker(target.dataset.focusWorker);
    else if (target.matches('[data-memory-lower]')) { state.memory[Number(target.dataset.memoryLower)].level--; renderRoute(); }
    else if (target.matches('[data-memory-delete]')) { state.memory.splice(Number(target.dataset.memoryDelete), 1); renderRoute(); }
    else if (target.matches('[data-forget-request]')) $('[data-forget-confirm]').hidden = false;
    else if (target.matches('[data-forget-cancel]')) $('[data-forget-confirm]').hidden = true;
    else if (target.matches('[data-forget-confirmed]')) { state.memory = []; renderRoute(); }
    else if (target.matches('[data-pin]')) {
      const name = target.dataset.pin; if (state.pins.has(name)) state.pins.delete(name); else state.pins.add(name);
      target.textContent = state.pins.has(name) ? 'Unpin' : 'Pin'; target.setAttribute('aria-pressed', String(state.pins.has(name)));
    } else if (target.matches('[data-folder-open]')) {
      const path = $('#fx-folder').value.trim(); if (!path) { $('#fx-folder').focus(); return; }
      notify('Sample folder selected: ' + path + ' · no filesystem access');
    } else if (target.matches('[data-session-preview]')) {
      const session = sessions[Number(target.dataset.sessionPreview)], preview = $('[data-session-preview-panel]');
      preview.hidden = false;
      preview.innerHTML = '<h3>' + session.title + '</h3><p class="fx-small">' + (session.kind === 'Companion' ?
        'Learning record: seed and acknowledgement explained. Read-only; cannot resume or hop this companion record.' :
        'Sample transcript: preserve the source; prepare a seed and confirm acknowledgement. No reasoning or credentials included.') + '</p>';
      preview.scrollIntoView({ block: 'nearest' });
    } else if (target.matches('[data-session-transfer]')) {
      $('[data-transfer-status]').textContent = target.dataset.sessionTransfer + ' preview: canonical transcript, attachments as references, no provider credentials or native model state. No file read or written.';
    } else if (target.matches('[data-session-resume]')) {
      const session = sessions[Number(target.dataset.sessionResume)];
      if (session.available) { closeRoute(); notify('Native resume preview · ' + session.agent + ' · same session'); }
      else { openRoute('launch'); notify('Native session aged out · new conversation from retained text seed'); }
    } else if (target.matches('[data-onboard]')) {
      $('[data-onboarding]').hidden = true;
      $('[data-onboarding-status]').textContent = target.dataset.onboard === 'dismiss' ? 'Dismissed for this sample project.' : 'Native ' + target.dataset.onboard + ' command prepared in preview, not sent.';
    } else if (target.matches('[data-native-login]')) notify('Native login preview for ' + target.dataset.nativeLogin + ' · no credentials collected');
    else if (target.matches('[data-browser-read]')) {
      $('[data-browser-engine]').textContent = 'Moli / read · sample'; $('[data-browser-reason]').textContent = 'Explicit read request · ' + $('#fx-browser-url').value;
      $('[data-browser-status]').textContent = 'Static sample loaded. No URL fetched.';
    } else if (target.matches('[data-browser-open]')) {
      $('[data-browser-engine]').textContent = 'Chrome / Patchright · open-ui · sample'; $('[data-browser-reason]').textContent = 'Explicit user action';
      $('[data-browser-status]').textContent = 'Open-UI preview recorded. No browser launched.';
    } else if (target.matches('[data-restore-tool]')) { $('[data-tool-exclusion]').textContent = 'Restored for next launch'; target.disabled = true; }
    else if (target.matches('[data-update-preview]')) notify('Update check preview · app, harness and plugin updates are separate');
    else if (target.matches('[data-pause-summaries]')) {
      const paused = target.getAttribute('aria-pressed') !== 'true'; target.setAttribute('aria-pressed', String(paused)); target.textContent = paused ? 'Resume summaries' : 'Pause summaries';
    } else if (target.matches('[data-round-next]')) advanceRound();
    else if (target.matches('[data-round-stop]')) {
      state.roundRunning = false; target.disabled = true; $('[data-round-next]').disabled = true; $('[data-round-status]').textContent = 'Sample discussion stopped. Replies remain visible.';
    } else if (target.matches('[data-round-interject]')) {
      const input = $('[data-round-interjection]'); if (!input.value.trim()) { input.focus(); return; }
      $('[data-round-transcript]').insertAdjacentHTML('beforeend', '<article class="fx-result"><h3>Your intervention</h3><p>' + escape(input.value) + '</p></article>'); input.value = '';
    }
  });
  document.addEventListener('input', event => {
    if (event.target.matches('[data-filter="projects"]')) {
      let count = 0;
      $$('[data-search-text]').forEach(item => { item.hidden = !item.dataset.searchText.includes(event.target.value.toLowerCase()); if (!item.hidden) count++; });
      $('[data-project-empty]').hidden = count > 0;
    }
    if (event.target.matches('[data-session-filter]')) filterSessions();
  });
  document.addEventListener('change', event => {
    if (event.target.matches('[data-session-agent], [data-session-project], [data-session-kind], [data-session-date]')) filterSessions();
    if (event.target.matches('[data-memory-enabled]')) { state.memoryOn = event.target.checked; }
    if (event.target.matches('[data-te-enabled]')) {
      state.tokenEconomizer = event.target.checked; $('[data-tool-exclusion]').textContent = state.tokenEconomizer ? 'Excluded in preview · next launch' : 'Unchanged · module off';
    }
    if (event.target.matches('[data-launch-harness]')) validatePlacement(event.target.form);
    if (event.target.matches('[data-agent-placement-harness]')) {
      state.placementHarness = event.target.value;
      const placement = state.agentPlacement[state.placementHarness], form = event.target.form;
      form.elements.program.value = placement.program; form.elements.home.value = placement.home;
      validateAgentPlacement(form);
    }
    if (event.target.matches('[data-agent-placement]')) validateAgentPlacement(event.target.form);
  });
  document.addEventListener('submit', event => {
    const form = event.target.closest('[data-form]'); if (!form) return;
    event.preventDefault(); const data = new FormData(form), status = $('[data-form-status]', form);
    if (form.dataset.form === 'project') {
      const name = String(data.get('name')).trim();
      if (!name || /[\\/:*?"<>|]/.test(name) || /[. ]$/.test(name) || name === '.' || name === '..' || /^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\.|$)/i.test(name) || state.projects.some(item => item.toLowerCase() === name.toLowerCase())) {
        status.textContent = 'Use a unique valid folder name, without reserved names or trailing dots.'; return;
      }
      state.projects.unshift(name); renderRoute();
      $('[data-form-status]').textContent = 'Sample project added: ' + name + '. No folder created.' + (data.has('daodan') ? ' Init command prepared, not sent.' : '');
    } else if (form.dataset.form === 'companion') {
      state.companion = data.get('companion'); state.companionModel = data.get('companionModel'); state.worker = data.get('worker'); state.cards = data.get('cards'); state.pending = data.get('pending');
      state.web = data.get('web'); state.dailyCap = Number(data.get('cap')); state.quiz = data.has('quiz'); state.memoryOn = data.has('memory');
      settingsValues.companion = Object.fromEntries(data); settingsValues.companion.quiz = state.quiz; settingsValues.companion.memory = state.memoryOn; syncCompanion();
      status.textContent = 'Sample settings saved. Automatic feed is simulated; requests and presets remain available.' + (state.cost >= state.dailyCap ? ' Daily cap reached: automatic cards paused; presets show their extra cost.' : '');
    } else if (form.dataset.form === 'routing') {
      if (data.get('executor').startsWith('Hosted') && !data.has('consent')) { status.textContent = 'Enable the digest-sharing opt-in or choose local julia1.'; return; }
      settingsValues.routing = Object.fromEntries(data); settingsValues.routing.consent = data.has('consent');
      status.textContent = 'Routing preview saved. No key entered, stored or sent.';
    } else if (form.dataset.form === 'appearance') {
      settingsValues.appearance = Object.fromEntries(data); status.textContent = 'Appearance and privacy preview saved. Desktop settings are illustrative.';
    } else if (form.dataset.form === 'agent-placement') {
      if (!validateAgentPlacement(form)) return;
      const harness = data.get('harness');
      state.agentPlacement[harness] = { program: data.get('program'), home: data.get('home') };
      status.textContent = 'Saved for ' + harness + ' · subsequent launches inherit this program and home. No files moved or processes started. Existing sessions are unchanged.';
    } else if (form.dataset.form === 'launch') {
      if (!validatePlacement(form)) return;
      const harness = data.get('harness');
      $('[data-launch-result]').textContent = 'Preview ready: ' + harness + ' · ' + (harness === 'DeepSeek Harness' ? 'native ACP chat' : 'terminal') + ' · ' + data.get('home') + ' · ' +
        (data.has('worktree') ? 'new linked worktree' : 'project worktree') + '. ' + (data.get('routing').startsWith('Automatic') ? 'Sample router outcome: keep instance default.' : 'Instance default selected.') + ' No process launched.';
    } else if (form.dataset.form === 'roundtable') {
      state.roundRunning = true; state.roundStep = 0; state.roundCostCap = Number(data.get('cap')); state.roundTokenCap = Number(data.get('tokens'));
      $('[data-round-transcript]').innerHTML = '<div class="fx-notice">' + escape(data.get('topic')) + '<br>Participants: ' + escape(data.get('first')) + ' / ' + escape(data.get('firstModel')) +
        ' (' + escape(data.get('tools')) + ') + ' + escape(data.get('second')) + ' / ' + escape(data.get('secondModel')) + ' (' + escape(data.get('secondTools')) + ')</div>';
      $('[data-round-next]').disabled = false; $('[data-round-stop]').disabled = false; advanceRound();
    }
  });
  // Connect the original chrome without replacing terminal output or existing layout handlers.
  $('.search-trigger')?.setAttribute('data-feature-open', 'sessions');
  $('.project-filter')?.setAttribute('data-feature-open', 'projects');
  $('.project-switch')?.setAttribute('data-feature-open', 'projects');
  $('.task-link')?.setAttribute('data-feature-open', 'tasks');
  if ($('.commandbar')) {
    const launchButton = document.createElement('button');
    launchButton.type = 'button'; launchButton.className = 'fx-button';
    launchButton.dataset.featureOpen = 'launch'; launchButton.textContent = '+ Instance';
    $('.commandbar').insertBefore(launchButton, $('#open-hop-top'));
  }
  // Reviewable seed text and destination selection augment the existing close/fork dialog.
  if ($('#hop-backdrop')) {
    const hopBody = $('#hop-backdrop .dialog-body');
    hopBody.insertAdjacentHTML('afterbegin', '<div class="fx-form" style="margin-bottom:14px">' +
      field('Destination harness · sample capability', select('destination', harnesses, 'Codex', 'data-hop-destination')) + '</div>');
    $('.carry-grid', hopBody).insertAdjacentHTML('afterend', '<details class="fx-small" style="margin:14px 0"><summary>Seed document preview · sample</summary><p><strong>Goal:</strong> finish a resumable hop boundary.<br><strong>Decisions:</strong> seed-only handover; keep the source writable until acknowledgement.<br><strong>References:</strong> src/hop.rs, tests/hop.rs, HOP-142.<br><strong>Next:</strong> verify destination acknowledgement before archiving in close mode.</p><p class="fx-muted">Excluded: full native history, permissions, model state, pending tool result and credentials.</p></details>');
    $('[data-hop-destination]').addEventListener('change', event => {
      const destination = event.target.value;
      $('#hop-title').textContent = 'Hop thread to ' + destination;
      const copy = $$('.route-copy', hopBody)[1];
      $('strong', copy).textContent = destination;
      $('small', copy).textContent = 'new native session · seed-only preview';
      $$('.route-card .agent-mark', hopBody)[1].textContent = destination === 'Codex' ? 'X' : destination[0];
      $('.hop-note:not(.success) span', hopBody).textContent = 'V1 hops are seed-only. A fresh ' + destination + ' session receives a handover document, not the full native history or state.';
      $('.hop-note.success span', hopBody).textContent = 'Sample destination check: login and adapter capabilities must pass before launch. Credentials are excluded from the seed.';
    });
    $('#start-hop').addEventListener('click', event => {
      event.stopImmediatePropagation();
      const destination = $('[data-hop-destination]').value;
      const fork = $('.mode-card.selected').dataset.mode === 'fork';
      $('#cancel-hop').click();
      notify('Sample ' + (fork ? 'fork' : 'hop') + ' prepared for ' + destination + ' · seed-only · no agent launched');
    }, true);
  }
  $$('.section-label button[aria-label="New project"]').forEach(item => item.dataset.featureOpen = 'projects');
  $$('button[aria-label^="New tab in"]').forEach(item => item.dataset.featureOpen = 'launch');
  document.addEventListener('keydown', event => {
    if (!(event.ctrlKey || event.metaKey)) return;
    const key = event.key.toLowerCase();
    if (modal.open) {
      if (['h', 'm', 'b', 'j', 'f', 'k', 't', 'e', 'o'].includes(key)) {
        event.stopImmediatePropagation();
        if (key === 'f') { event.preventDefault(); openRoute('sessions'); $('[data-session-filter]').focus(); }
      }
      return;
    }
    const map = { e: 'learning', f: 'sessions', k: 'tasks', o: 'overview', t: 'launch' };
    if (map[key]) { event.preventDefault(); event.stopImmediatePropagation(); openRoute(map[key]); }
  }, true);
  renderRoute();
})();

import { uiPart, uiButton, uiInput, uiSelect, uiOption, uiTraits, uiTone } from '@acaprino/imago/ui';
import { openModal } from './modal.ts';
import { parseMuroStatus, pluginsButtons, pluginsLine, type MuroStatus } from './muro-status.ts';
import { createModelSettingsPanel } from './model-settings-panel.ts';
import type { Save as ModelSave } from './model-settings-model.ts';
import {
  buildMemory, buildSave, DEFAULT_MEMORY, formFromView, memorySummary, parseMemory, parseView, summary, MAX_MARGIN_MIB, MAX_VARIABLES,
  type Form, type MemorySafety, type Row, type SaveRequest,
} from './settings-model.ts';

/** Where each part of Settings sits, in order: the id after #settings-section-, the index label, the heading. */
export const SETTINGS_SECTIONS = [
  ['appearance', 'Appearance', 'Appearance'], ['agents', 'Agents', 'Agents'], ['models', 'Models', 'Models & API keys'], ['memory', 'Memory', 'Memory'], ['alerts', 'Alerts', 'Alerts'],
  ['github', 'GitHub', 'GitHub'], ['variables', 'Variables', 'Environment variables'], ['sessions', 'Sessions', 'Sessions'],
  ['plugins', 'Plugins', 'Plugins'],
] as const;
export type SettingsSection = typeof SETTINGS_SECTIONS[number][0];

type Options = {
  desktop: boolean;
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  notify(message: unknown, tone?: 'info' | 'error'): void;
  restoreFocus?: () => void;
  /** The theme and the terminal controls (presentation.ts). They apply and save as they change. */
  appearance: { theme: HTMLElement; terminal: HTMLElement; reset(): void; reload(): void };
  /** The desktop alert controls (attention-control.ts). They apply and save as they change. */
  alerts: { fields: HTMLElement; reset(): void };
  /** The interface zoom (desktop-shell.ts): the level Ctrl+= and Ctrl+- move. */
  zoom: { levels: readonly number[]; current(): number; available(): boolean; set(level: number): Promise<boolean>; subscribe(listener: () => void): void };
};

const message = (error: unknown) => typeof error === 'string' ? error : (error as { message?: string } | null)?.message ?? 'The settings could not be saved.';
const percent = (level: number) => `${Math.round(level * 100)}%`;

/** Settings: one drawer for every preference of the app. Each section keeps saving the way its owner
 * always has: Appearance, Alerts and Sessions apply as they change; Agents, Models, Memory, GitHub and the
 * variables wait for Save. Saved secrets never come back to this page; they can be replaced or removed. */
export function createSettingsPanel(options: Options) {
  const dialog = uiTraits(uiPart('dialog'), 'sectionedDrawer'); dialog.id = 'settings-dialog'; dialog.setAttribute('aria-labelledby', 'settings-title');
  const top = uiPart('stack');
  const heading = uiPart('row', undefined, 'dialog-heading'); const title = uiPart('title', 'Settings'); title.id = 'settings-title';
  const closeButton = uiButton('×'); closeButton.setAttribute('aria-label', 'Close settings');
  heading.append(title, closeButton);
  const index = uiTraits(uiPart('nav'), 'row'); index.setAttribute('aria-label', 'Settings sections');
  top.append(heading, index);
  const intro = uiPart('hint', 'Appearance, Alerts and Sessions apply as you change them. Agents, Models, Memory, GitHub and Environment variables apply when you press Save.');
  intro.id = 'settings-intro';

  const parts = new Map<SettingsSection, { section: HTMLElement; heading: HTMLHeadingElement; link: HTMLButtonElement }>();
  for (const [id, label, text] of SETTINGS_SECTIONS) {
    const section = uiTraits(uiPart('region'), 'stack', 'section'); section.id = `settings-section-${id}`;
    const sectionHeading = uiPart('heading', text); sectionHeading.id = `settings-section-${id}-title`; sectionHeading.tabIndex = -1;
    section.setAttribute('aria-labelledby', sectionHeading.id); section.append(sectionHeading);
    const link = uiButton(label, { cls: 'settings-index-link', sm: true }); link.setAttribute('aria-controls', section.id);
    link.onclick = () => go(id);
    index.append(link); parts.set(id, { section, heading: sectionHeading, link });
  }
  const part = (id: SettingsSection) => parts.get(id)!.section;

  // Appearance: the theme, the interface zoom, then the terminals.
  const zoomLabel = uiPart('label', 'Interface zoom'); zoomLabel.htmlFor = 'settings-zoom';
  const zoom = uiSelect(); zoom.id = 'settings-zoom'; zoom.setAttribute('aria-describedby', 'settings-zoom-hint');
  for (const level of options.zoom.levels) { const option = uiOption(String(level), percent(level)); zoom.append(option); }
  const zoomHint = uiPart('hint', 'Ctrl+= and Ctrl+- change it too; Ctrl+0 returns to 100%.'); zoomHint.id = 'settings-zoom-hint';
  part('appearance').append(options.appearance.theme, zoomLabel, zoom, zoomHint, options.appearance.terminal);

  // Agents
  const fullLabel = uiPart('label'); const full = uiInput('checkbox'); full.id = 'settings-full-access';
  fullLabel.append(full, document.createTextNode(' Full access: start agents without their own protections'));
  const fullHint = uiPart('hint', 'Claude starts in bypass-permissions mode, Codex without its sandbox and approvals, OpenCode allows every tool. '
    + 'Pi and PowerShell never asked. An agent can then change or delete anything your Windows account can, inside or outside the project. '
    + 'The private home keeps their settings and sign-ins apart; it does not confine their files. Applies to sessions started after you save.');
  part('agents').append(fullLabel, fullHint);

  const models = createModelSettingsPanel(options.invoke, text => say(text));
  part('models').append(models.element);

  // Memory
  const memoryLabel = uiPart('label'); const memoryCheck = uiInput('checkbox'); memoryCheck.id = 'settings-memory-safety';
  memoryLabel.append(memoryCheck, document.createTextNode(' Check free memory before a session, shell or harness check starts'));
  const marginLabel = uiPart('label', 'Memory kept free (MiB)'); marginLabel.htmlFor = 'settings-memory-margin';
  const margin = uiInput(); margin.id = 'settings-memory-margin'; margin.type = 'number'; margin.min = '0'; margin.max = String(MAX_MARGIN_MIB); margin.step = '64'; margin.inputMode = 'numeric';
  const memoryHint = uiPart('hint', 'On: a start waits a few seconds for a fresh measurement and is refused when free RAM or allocatable memory '
    + 'would drop below what it needs to start plus this margin. Off: nothing waits for or is refused on memory; Windows may then page or close apps '
    + 'when memory runs out. Running sessions are never stopped by this setting. Applies to the next start after you save.');
  part('memory').append(memoryLabel, marginLabel, margin, memoryHint);

  // Alerts
  part('alerts').append(options.alerts.fields);

  // GitHub
  const userLabel = uiPart('label', 'Username'); userLabel.htmlFor = 'settings-github-user';
  const user = uiInput(); user.id = 'settings-github-user'; user.autocomplete = 'off'; user.spellcheck = false; user.maxLength = 39;
  const tokenLabel = uiPart('label', 'Personal access token'); tokenLabel.htmlFor = 'settings-github-token';
  const token = uiInput(); token.id = 'settings-github-token'; token.type = 'password'; token.autocomplete = 'new-password'; token.spellcheck = false; token.maxLength = 255;
  const tokenRow = uiPart('row'); const removeToken = uiButton('Remove token'); removeToken.id = 'settings-github-remove';
  tokenRow.append(token, removeToken);
  const githubHint = uiPart('hint', 'GitHub no longer accepts an account password, so use a personal access token (a fine-grained one limited to the repositories you need is safest). '
    + 'Agents receive it as GH_TOKEN and as the Git credential for github.com, so git push and gh never ask. Stored encrypted for this Windows user; any agent you start can read it.');
  part('github').append(userLabel, user, tokenLabel, tokenRow, githubHint);

  // Environment variables
  const list = uiPart('stack'); list.setAttribute('role', 'list'); list.setAttribute('aria-label', 'Environment variables');
  const add = uiButton('Add variable'); add.id = 'settings-add-variable';
  const varsHint = uiPart('hint', 'Added to every new agent session. A secret value is stored encrypted and is never shown again; type a new one to replace it.');
  part('variables').append(list, add, varsHint);

  // Sessions
  const titlesLabel = uiPart('label'); const titles = uiInput('checkbox'); titles.id = 'settings-titles';
  titlesLabel.append(titles, document.createTextNode(' Activity titles'));
  const titlesHint = uiPart('hint', 'Sessions shows what each running session is doing, verbatim from its last tool call or message. Nothing leaves this computer.');
  const indexingLabel = uiPart('label'); const indexing = uiInput('checkbox'); indexing.id = 'settings-indexing';
  indexingLabel.append(indexing, document.createTextNode(' Index session text for search'));
  const indexingHint = uiPart('hint', 'Lets Search index read your session archives. Off stops indexing; text already indexed stays searchable until you forget it there.');
  const sessionsStatus = uiPart('hint'); sessionsStatus.id = 'settings-sessions-status'; sessionsStatus.setAttribute('role', 'status');
  part('sessions').append(titlesLabel, titlesHint, indexingLabel, indexingHint, sessionsStatus);

  // Plugins: Shinatama's own Muro keeps the plugins of its agents in sync (muro.rs).
  const pluginsStatus = uiPart('hint'); pluginsStatus.id = 'settings-plugins-status'; pluginsStatus.setAttribute('role', 'status');
  const pluginsRow = uiPart('row');
  const syncNow = uiButton('Sync now'); syncNow.id = 'settings-plugins-sync';
  const openMuro = uiButton('Open Muro'); openMuro.id = 'settings-plugins-open';
  pluginsRow.append(syncNow, openMuro);
  const pluginsHint = uiPart('hint', 'Muro installs and updates the plugins you choose for the agents Shinatama launches, in their homes inside Shinatama only: '
    + "your own Muro and your agents' usual homes are left alone. Open Muro to choose them; a session already open gets a new plugin at its next start.");
  pluginsHint.id = 'settings-plugins-hint';
  syncNow.setAttribute('aria-describedby', `${pluginsStatus.id} ${pluginsHint.id}`); openMuro.setAttribute('aria-describedby', `${pluginsStatus.id} ${pluginsHint.id}`);
  part('plugins').append(pluginsStatus, pluginsRow, pluginsHint);

  const actions = uiPart('actions');
  const status = uiPart('status'); status.setAttribute('role', 'status'); status.id = 'settings-status';
  const reload = uiButton('Reload'); reload.title = 'Read every saved setting again';
  const save = uiButton('Save', { primary: true }); save.id = 'settings-save'; save.setAttribute('aria-describedby', intro.id);
  const close = uiButton('Close');
  actions.append(status, reload, save, close);
  // The title and index above, the actions below, and only the sections between them scroll.
  const body = uiPart('scroll'); body.append(intro, ...[...parts.values()].map(entry => entry.section));
  dialog.append(top, body, actions);
  (document.querySelector('#app') ?? document.body).append(dialog);

  // The index follows the scroll; a section picked from it stays marked until the user scrolls.
  let picked: SettingsSection | null = null; let spyFrame = 0;
  function mark(id: SettingsSection) { for (const [key, entry] of parts) entry.link.setAttribute('aria-current', String(key === id)); }
  function go(id: SettingsSection) {
    const entry = parts.get(id)!; picked = id; mark(id);
    const smooth = !matchMedia('(prefers-reduced-motion: reduce)').matches;
    body.scrollTo({ top: Math.max(0, entry.section.offsetTop - 6), behavior: smooth ? 'smooth' : 'auto' });
    entry.heading.focus({ preventScroll: true });
  }
  function spy() {
    spyFrame = 0; if (picked) return;
    const line = body.scrollTop + 24; let current: SettingsSection = SETTINGS_SECTIONS[0][0];
    for (const [id] of SETTINGS_SECTIONS) if (part(id).offsetTop <= line) current = id;
    if (body.scrollTop + body.clientHeight >= body.scrollHeight - 4) current = SETTINGS_SECTIONS.at(-1)![0];
    mark(current);
  }
  body.addEventListener('scroll', () => { if (!spyFrame) spyFrame = requestAnimationFrame(spy); }, { passive: true });
  for (const type of ['wheel', 'pointerdown', 'keydown'] as const) dialog.addEventListener(type, () => { picked = null; }, { passive: true });
  mark('appearance');

  function paintZoom() {
    const level = options.zoom.current();
    zoom.value = String(options.zoom.levels.find(step => Math.abs(step - level) < 0.001) ?? 1);
    zoom.disabled = !options.zoom.available();
  }
  options.zoom.subscribe(paintZoom);
  zoom.onchange = () => {
    const wanted = Number(zoom.value); zoom.disabled = true;
    void options.zoom.set(wanted).then(changed => { if (!changed) options.notify('This window cannot change its zoom.', 'info'); paintZoom(); });
  };

  let form: Form = { fullAccess: true, githubUsername: '', tokenInput: '', removeToken: false, hasToken: false, rows: [] };
  let memory: MemorySafety = { ...DEFAULT_MEMORY };
  let loaded = false; let busy = false;
  const statusLine = () => `${summary(form)} · ${memorySummary(memory)}`;
  function paintMemory() { memoryCheck.checked = memory.enabled; margin.value = String(memory.marginMib); margin.disabled = !memory.enabled; }

  const say = (text: string, error = false) => { status.textContent = text; status.dataset.tone = error ? 'error' : 'info'; uiTone(status, error ? 'err' : null); };
  function paintToken() {
    token.placeholder = form.hasToken && !form.removeToken ? 'Saved. Type to replace' : 'ghp_… or github_pat_…';
    removeToken.hidden = !form.hasToken || form.removeToken || Boolean(token.value);
    if (form.hasToken && form.removeToken) token.placeholder = 'Will be removed when you save';
  }
  function paintRows() {
    list.replaceChildren(...form.rows.map((row, index) => rowView(row, index)));
    add.disabled = form.rows.length >= MAX_VARIABLES;
  }
  function rowView(row: Row, index: number) {
    const line = uiPart('row', undefined, 'settings-var'); line.setAttribute('role', 'listitem');
    const name = uiInput(); name.placeholder = 'NAME'; name.value = row.name; name.setAttribute('aria-label', 'Variable name'); name.autocomplete = 'off'; name.spellcheck = false; name.maxLength = 128;
    name.oninput = () => { row.name = name.value; };
    const value = uiInput(); value.value = row.value; value.setAttribute('aria-label', `Value of ${row.name || 'the variable'}`); value.autocomplete = 'off'; value.spellcheck = false;
    const paintValue = () => { value.type = row.secret ? 'password' : 'text'; value.placeholder = row.hasValue && !row.touched && row.secret ? 'Saved. Type to replace' : 'value'; };
    value.oninput = () => { row.value = value.value; row.touched = true; };
    const secretLabel = uiPart('label', undefined, 'settings-secret'); const secret = uiInput('checkbox'); secret.checked = row.secret;
    secret.onchange = () => { row.secret = secret.checked; paintValue(); };
    secretLabel.append(secret, document.createTextNode(' Secret'));
    const remove = uiButton('×'); remove.setAttribute('aria-label', `Remove ${row.name || 'variable'}`);
    remove.onclick = () => { form.rows.splice(index, 1); paintRows(); };
    paintValue();
    line.append(name, value, secretLabel, remove);
    return line;
  }
  function paint() {
    full.checked = form.fullAccess; user.value = form.githubUsername; token.value = form.tokenInput;
    paintToken(); paintRows(); paintMemory();
  }
  function read() { form.fullAccess = full.checked; form.githubUsername = user.value; form.tokenInput = token.value; }

  async function load() {
    if (!options.desktop) { say('Open the desktop app to change settings.', true); save.disabled = true; return; }
    if (busy) return;
    busy = true; save.disabled = true; models.setDisabled(true); say('Loading…');
    let memoryProblem = '';
    try {
      const saved = parseMemory(await options.invoke('memory_settings_load'));
      if (!saved) throw new Error('The memory settings are not in a form this version understands.');
      memory = saved;
    } catch (error) { memory = { ...DEFAULT_MEMORY }; memoryProblem = ` ${message(error)} Memory safety stays on until you save.`; }
    paintMemory();
    let modelsReady = false;
    try {
      await models.load();
      modelsReady = true;
      const view = parseView(await options.invoke('launch_settings_load'));
      if (!view) throw new Error('The saved settings are not in a form this version understands.');
      form = formFromView(view); loaded = true; save.disabled = false; paint();
      say(`New sessions start with: ${statusLine()}.${memoryProblem}`, Boolean(memoryProblem));
    } catch (error) {
      loaded = false; save.disabled = !modelsReady;
      say(`${message(error)} ${modelsReady ? 'Agents start with their own protections until you save.' : 'Reload before saving.'}`, true);
    } finally { busy = false; models.setDisabled(false); }
  }
  async function submit() {
    if (busy || !options.desktop) return;
    read();
    const built = buildSave(form);
    if ('problem' in built) { say(built.problem, true); return; }
    const memoryBuilt = buildMemory(memoryCheck.checked, margin.value);
    if ('problem' in memoryBuilt) { say(memoryBuilt.problem, true); margin.focus(); return; }
    let modelRequest: ModelSave;
    try { modelRequest = models.prepare(); }
    catch (error) { say(message(error), true); go('models'); return; }
    busy = true; save.disabled = true; models.setDisabled(true); say('Saving…');
    let modelsSaved = false;
    try {
      await models.save(modelRequest);
      modelsSaved = true;
      const view = parseView(await options.invoke<unknown>('launch_settings_save', { request: built.request satisfies SaveRequest }));
      if (!view) throw new Error('The saved settings could not be confirmed.');
      form = formFromView(view); loaded = true;
      const savedMemory = parseMemory(await options.invoke<unknown>('memory_settings_save', { request: memoryBuilt.request }));
      if (!savedMemory) throw new Error('The memory settings could not be confirmed.');
      memory = savedMemory; paint();
      say(`Saved. Shared models apply to new sessions. New sessions start with: ${statusLine()}.`);
    } catch (error) {
      const problem = `${modelsSaved ? 'Models were saved; the remaining settings were not all saved. ' : ''}${message(error)}`;
      say(problem, true); options.notify(problem, 'error');
    }
    finally { busy = false; save.disabled = false; models.setDisabled(false); }
  }

  // Sessions: each switch applies at once through the command its view always used.
  let sessionsBusy = 0;
  const saySessions = (text: string, error = false) => { sessionsStatus.textContent = text; sessionsStatus.dataset.tone = error ? 'error' : 'info'; uiTone(sessionsStatus, error ? 'err' : null); };
  async function loadSessions() {
    if (!options.desktop) { titles.disabled = indexing.disabled = true; saySessions('Open the desktop app to change these.'); return; }
    const mine = ++sessionsBusy; titles.disabled = indexing.disabled = true;
    const [titlesRead, indexRead] = await Promise.allSettled([
      options.invoke<unknown>('titles_enabled'), options.invoke<{ index?: { enabled?: unknown } }>('session_index_status'),
    ]);
    if (mine !== sessionsBusy) return;
    const titlesOn = titlesRead.status === 'fulfilled' && typeof titlesRead.value === 'boolean' ? titlesRead.value : null;
    const indexOn = indexRead.status === 'fulfilled' && typeof indexRead.value?.index?.enabled === 'boolean' ? indexRead.value.index.enabled : null;
    if (titlesOn !== null) titles.checked = titlesOn;
    if (indexOn !== null) indexing.checked = indexOn;
    titles.disabled = titlesOn === null; indexing.disabled = indexOn === null;
    saySessions(titlesOn === null || indexOn === null ? 'Part of this section could not be read. Reload to try again.' : '', titlesOn === null || indexOn === null);
  }
  function sessionSwitch(input: HTMLInputElement, command: string, on: string, off: string) {
    input.onchange = () => {
      const wanted = input.checked; const mine = ++sessionsBusy; input.disabled = true;
      options.invoke(command, { enabled: wanted }).then(() => { if (mine === sessionsBusy) saySessions(wanted ? on : off); },
        (error: unknown) => { input.checked = !wanted; saySessions(message(error), true); })
        .finally(() => { input.disabled = false; });
    };
  }
  // Plugins: the status line and the two buttons, disabled with the reason when they cannot run.
  let pluginsBusy = 0; let pluginsTimer: ReturnType<typeof setTimeout> | undefined;
  const clock = (seconds: number) => {
    const at = new Date(seconds * 1000);
    const today = at.toDateString() === new Date().toDateString();
    return at.toLocaleString([], today ? { hour: '2-digit', minute: '2-digit' } : { day: 'numeric', month: 'short', hour: '2-digit', minute: '2-digit' });
  };
  const sayPlugins = (text: string, error = false) => { pluginsStatus.textContent = text; pluginsStatus.dataset.tone = error ? 'error' : 'info'; uiTone(pluginsStatus, error ? 'err' : null); };
  function paintPlugins(status: MuroStatus | null, problem?: string) {
    const why = pluginsButtons(status, options.desktop);
    syncNow.disabled = why.sync !== null; syncNow.title = why.sync ?? 'Ask Muro to check and apply the plugins now';
    openMuro.disabled = why.open !== null; openMuro.title = why.open ?? "Show Muro's window on the list of Shinatama's agents";
    if (problem) sayPlugins(problem, true);
    else if (!options.desktop) sayPlugins('Open the desktop app to use Muro.');
    else if (!status) sayPlugins("Muro's state could not be read. Reload to try again.", true);
    else { const line = pluginsLine(status, clock); sayPlugins(line.text, line.tone === 'error'); }
    // A Muro that is still starting is asked again while Settings is open.
    clearTimeout(pluginsTimer);
    if (status?.state === 'starting' && dialog.open) pluginsTimer = setTimeout(() => { void loadPlugins(); }, 2000);
  }
  async function loadPlugins() {
    if (!options.desktop) { paintPlugins(null); return; }
    const mine = ++pluginsBusy;
    try { const status = parseMuroStatus(await options.invoke<unknown>('muro_status')); if (mine === pluginsBusy) paintPlugins(status); }
    catch (error) { if (mine === pluginsBusy) paintPlugins(null, message(error)); }
  }
  let syncing = false;
  syncNow.onclick = () => {
    const mine = ++pluginsBusy; syncing = true; syncNow.disabled = openMuro.disabled = true; sayPlugins('Syncing…');
    options.invoke<unknown>('muro_sync_now').finally(() => { syncing = false; }).then(
      value => { if (mine === pluginsBusy) paintPlugins(parseMuroStatus(value)); },
      (error: unknown) => { if (mine === pluginsBusy) void loadPlugins().then(() => sayPlugins(message(error), true)); });
  };
  // The buttons come back from the status, never re-enabled blind: a Sync now may still run.
  openMuro.onclick = () => {
    openMuro.disabled = true;
    options.invoke('muro_open').then(() => undefined, (error: unknown) => sayPlugins(message(error), true))
      .finally(() => { if (!syncing) void loadPlugins(); });
  };

  sessionSwitch(titles, 'titles_set_enabled', 'Activity titles on.', 'Activity titles off.');
  sessionSwitch(indexing, 'session_index_enable', 'Indexing allowed.', 'Indexing off.');

  full.onchange = () => { form.fullAccess = full.checked; say(`Not saved yet: ${summary({ ...form, fullAccess: full.checked })}.`); };
  memoryCheck.onchange = () => {
    margin.disabled = !memoryCheck.checked;
    say(`Not saved yet: ${memorySummary({ enabled: memoryCheck.checked, marginMib: Number(margin.value) || 0 })}.`);
  };
  token.oninput = () => { form.tokenInput = token.value; paintToken(); };
  removeToken.onclick = () => { form.removeToken = true; token.value = ''; form.tokenInput = ''; paintToken(); say('The token will be removed when you save.'); };
  add.onclick = () => {
    if (form.rows.length >= MAX_VARIABLES) return;
    form.rows.push({ name: '', secret: false, value: '', hasValue: false, touched: true }); paintRows();
    list.querySelector<HTMLInputElement>('.settings-var:last-child input')?.focus();
  };
  save.onclick = () => { void submit(); };
  reload.onclick = () => { options.appearance.reload(); void loadSessions(); void loadPlugins(); if (!busy) void load(); };
  closeButton.onclick = close.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { clearTimeout(pluginsTimer); token.value = ''; form.tokenInput = ''; models.clearSecrets(); if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  dialog.addEventListener('submit', event => event.preventDefault());

  return {
    /** Opens Settings, at one section when given. */
    show(section?: SettingsSection) {
      const opening = !dialog.open;
      if (opening) { openModal(dialog); body.scrollTop = 0; picked = null; mark('appearance'); }
      options.appearance.reset(); options.alerts.reset(); paintZoom();
      if (!loaded || !busy) void load();
      void loadSessions();
      void loadPlugins();
      if (section) go(section);
      else if (opening) index.querySelector<HTMLButtonElement>('button')?.focus({ preventScroll: true });
    },
  };
}

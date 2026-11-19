import { APIS, HARNESSES, HARNESS_NAMES, MAX_CONNECTIONS, MAX_MODELS, PRESETS, buildModelSave, connectionHost, emptyForm, firstModelId, fromView,
  newModel, parseModelView, readChoice, suggestedName, usedBy, writeChoice, type Edit, type Harness, type Save } from './model-settings-model.ts';
import './model-settings-panel.css';

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text?: string) => {
  const node = document.createElement(tag); node.className = className; if (text !== undefined) node.textContent = text; return node;
};
const apiNames = ['OpenAI Chat Completions', 'OpenAI Responses', 'Anthropic Messages', 'Google Generative AI'];
/** Agents Shinatama can launch with a shared connection today; Hermes only stores its choice. */
const LAUNCHABLE: Harness[] = ['opencode', 'pi'];
const CUSTOM_PRESET = 'Custom endpoint';
function button(text: string, click: () => void, className = '') { const node = el('button', className, text); node.type = 'button'; node.onclick = click; return node; }
function field(text: string, input: HTMLElement) {
  const label = el('label', 'model-field'); label.append(el('span', '', text), input); return label;
}
function option(value: string, text: string) { const node = el('option', '', text); node.value = value; return node; }
const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;
const list = (names: string[]) => names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;
let ids = 0;

/** Edits shared connections; the containing Settings drawer owns loading and the Save action.
 *  The model each agent starts with leads the section, since it is what decides a new session;
 *  connections follow, each saying which agents use it so an unused one is never silent. */
export function createModelSettingsPanel(invoke: Invoke, changed: (message: string) => void) {
  const element = el('fieldset', 'model-settings'); element.disabled = true;
  const intro = el('p', 'hint', 'Connect a provider once, then choose the model each agent starts with. Keys are encrypted for this Windows user and never shown again. Changes apply to new sessions after Save.');

  const useTitle = el('h3', 'model-block-title', 'Model for new sessions'); useTitle.id = 'models-use-title';
  const useHint = el('p', 'hint', 'Each agent uses its own sign-in and model until you add a connection below and choose one of its models here.');
  const assignments = el('div', 'model-assignments'); assignments.setAttribute('role', 'group'); assignments.setAttribute('aria-labelledby', useTitle.id);
  const useBlock = el('div', 'model-block'); useBlock.append(useTitle, useHint, assignments);

  const connectionsTitle = el('h3', 'model-block-title', 'Connections');
  const connectionsHint = el('p', 'hint', 'A connection is a provider endpoint, its API key and the models you use from it.');
  const connections = el('div', 'model-connections');
  const empty = el('p', 'model-empty', 'No connections yet. Add your provider or a local server below, then choose its model above.');
  const addRow = el('div', 'model-add');
  const preset = el('select'); preset.id = 'models-preset'; preset.setAttribute('aria-label', 'New connection provider');
  const presetLabel = el('label', 'model-add-label', 'New connection'); presetLabel.htmlFor = preset.id;
  PRESETS.forEach((p, i) => preset.append(option(String(i), p.name)));
  preset.value = String(PRESETS.findIndex(p => p.name === CUSTOM_PRESET));
  const add = button('Add connection', () => {
    if (form.connections.length >= MAX_CONNECTIONS) return;
    const selected = PRESETS[Number(preset.value)] ?? PRESETS[0];
    const custom = selected.name === CUSTOM_PRESET;
    const edit: Edit = { connection: { id: crypto.randomUUID(), ...selected, name: custom ? '' : selected.name, models: [newModel()] }, hasApiKey: false, keyInput: '', removeKey: false };
    if (custom) autoNamed.add(edit);
    form.connections.push(edit); expanded.add(edit.connection.id); render();
    const last = connections.lastElementChild;
    last?.querySelector<HTMLInputElement>(custom ? '.model-url' : '.model-id')?.focus();
    dirty();
  }); add.id = 'models-add-connection'; addRow.append(presetLabel, preset, add);
  const connectionsBlock = el('div', 'model-block'); connectionsBlock.append(connectionsTitle, connectionsHint, connections, empty, addRow);
  element.append(intro, useBlock, connectionsBlock);

  let form = emptyForm(); let loaded = false;
  /** Cards open for editing survive re-renders; new connections open, saved ones start closed. */
  const expanded = new Set<string>();
  /** New custom connections take their name from the endpoint until the user types one. */
  const autoNamed = new WeakSet<Edit>();
  let paints: (() => void)[] = [];
  const dirty = () => changed('Model changes are not saved yet. Press Save to apply them to new sessions.');
  const refresh = () => { renderAssignments(); paints.forEach(paint => paint()); };
  function input(value: string, change: (value: string) => void, maxLength = 200) {
    const node = el('input'); node.value = value; node.maxLength = maxLength; node.autocomplete = 'off'; node.spellcheck = false;
    node.oninput = () => { change(node.value); dirty(); }; return node;
  }
  const nameOf = (edit: Edit) => edit.connection.name.trim() || suggestedName(edit.connection.baseUrl) || 'New connection';

  function renderAssignments() {
    assignments.replaceChildren(...HARNESSES.map(harness => {
      const name = HARNESS_NAMES[harness];
      const row = el('div', 'model-assignment'); row.dataset.harness = harness;
      const select = el('select'); select.id = `models-use-${harness}`; select.setAttribute('aria-label', `${name} model`);
      const label = el('label', 'model-assignment-name', name); label.htmlFor = select.id;
      select.append(option('', `${name}'s own sign-in and model`));
      const selected = form.assignments[harness];
      const exists = selected !== null && form.connections.some(c => c.connection.id === selected.connectionId && c.connection.models.some(m => m.id.trim() === selected.modelId));
      if (selected && !exists) select.append(option(writeChoice(selected), 'Removed model · choose again'));
      // Each option names its connection too, so the closed menu still says where the model comes from.
      for (const edit of form.connections) {
        const models = edit.connection.models.map(m => m.id.trim()).filter(Boolean);
        if (!models.length) { const none = option('', `${nameOf(edit)}: add a model ID first`); none.disabled = true; select.append(none); }
        models.forEach(id => select.append(option(writeChoice({ connectionId: edit.connection.id, modelId: id }), `${id} · ${nameOf(edit)}`)));
      }
      select.value = selected ? writeChoice(selected) : '';
      select.onchange = () => {
        form.assignments[harness] = readChoice(select.value); refresh(); dirty();
        document.getElementById(select.id)?.focus();
      };
      const help = el('p', 'model-assignment-help'); help.id = `${select.id}-help`;
      if (selected && !exists) { help.textContent = 'That model was removed. Choose another one, or the agent’s own sign-in and model.'; help.dataset.tone = 'error'; select.setAttribute('aria-invalid', 'true'); }
      else if (harness === 'hermes') help.textContent = 'Saved only: Shinatama cannot launch Hermes yet.';
      help.hidden = !help.textContent;
      if (!help.hidden) select.setAttribute('aria-describedby', help.id);
      row.append(label, select, help); return row;
    }));
    useHint.hidden = form.connections.length > 0;
  }

  function connectionCard(edit: Edit, index: number) {
    const c = edit.connection; const bodyId = `model-connection-${++ids}`;
    const card = el('div', 'model-connection');
    const head = el('div', 'model-connection-head');
    const titles = el('div', 'model-connection-titles');
    const title = el('strong', 'model-connection-name'); const meta = el('span', 'model-connection-meta');
    titles.append(title, meta);
    const status = el('span', 'model-status');
    const toggle = button('', () => {
      if (expanded.has(c.id)) expanded.delete(c.id); else expanded.add(c.id);
      paintOpen(); if (expanded.has(c.id)) body.querySelector<HTMLInputElement>('input')?.focus();
    }, 'model-toggle');
    toggle.setAttribute('aria-controls', bodyId);
    head.append(titles, status, toggle);

    // An unused connection says so in words and offers the assignment it is missing.
    const unused = el('div', 'model-unused');
    const unusedText = el('span', 'model-unused-text');
    const quick = LAUNCHABLE.map(harness => {
      const node = button(`Use for ${HARNESS_NAMES[harness]}`, () => {
        const modelId = firstModelId(c); if (!modelId) return;
        form.assignments[harness] = { connectionId: c.id, modelId }; refresh();
        changed(`${HARNESS_NAMES[harness]} will start with ${modelId} from ${nameOf(edit)}. Press Save to apply it to new sessions.`);
        document.getElementById(`models-use-${harness}`)?.focus();
      }); node.dataset.harness = harness; return node;
    });
    unused.append(unusedText, ...quick);

    const body = el('div', 'model-connection-body'); body.id = bodyId;
    const grid = el('div', 'model-fields');
    const name = input(c.name, value => { c.name = value; autoNamed.delete(edit); refresh(); }, 100);
    name.placeholder = suggestedName(c.baseUrl) ?? 'e.g. My provider';
    const api = el('select'); api.setAttribute('aria-label', 'API protocol');
    APIS.forEach((value, i) => api.append(option(value, apiNames[i]))); api.value = c.api;
    api.onchange = () => { c.api = APIS.find(value => value === api.value) ?? APIS[0]; dirty(); };
    const url = input(c.baseUrl, value => {
      c.baseUrl = value;
      const suggestion = suggestedName(value);
      if (autoNamed.has(edit)) { c.name = suggestion ?? ''; name.value = c.name; }
      name.placeholder = suggestion ?? 'e.g. My provider'; refresh();
    }, 2048);
    url.type = 'url'; url.placeholder = 'https://provider.example/v1'; url.classList.add('model-url');
    const urlField = field('Base URL', url); urlField.classList.add('model-wide');
    grid.append(urlField, field('Connection name', name), field('API protocol', api));
    const keyRow = el('div', 'settings-inline');
    const key = input(edit.keyInput, value => { edit.keyInput = value; paint(); }, 4096); key.type = 'password'; key.autocomplete = 'new-password'; key.className = 'model-api-key';
    const paintKey = () => { key.placeholder = edit.removeKey ? 'Will be removed on Save' : edit.hasApiKey ? 'Saved · type to replace' : 'Optional for local servers'; };
    const removeKey = button('Remove key', () => { edit.removeKey = true; edit.keyInput = ''; key.value = ''; paintKey(); removeKey.hidden = true; paint(); dirty(); });
    removeKey.hidden = !edit.hasApiKey || edit.removeKey; paintKey(); keyRow.append(key, removeKey);
    key.setAttribute('aria-label', 'API key');
    const keyField = el('div', 'model-field model-wide'); keyField.append(el('span', '', 'API key'), keyRow); grid.append(keyField);

    const modelsTitle = el('h4', 'model-subtitle', 'Models');
    const modelsHint = el('p', 'hint', 'Use the exact model IDs your provider expects. The token limits start at common values: adjust them to your model.');
    const models = el('div', 'model-list');
    function renderModels() {
      models.replaceChildren(...c.models.map((m, i) => {
        const row = el('div', 'model-entry');
        const id = input(m.id, value => {
          const previous = m.id.trim(); m.id = value;
          for (const h of HARNESSES) { const s = form.assignments[h]; if (s?.connectionId === c.id && s.modelId === previous) s.modelId = value.trim(); }
          refresh();
        }); id.className = 'model-id'; id.placeholder = 'e.g. qwen3-coder';
        const number = (value: number, change: (value: number) => void) => {
          const node = input(String(value), text => change(Number(text)), 8); node.type = 'number'; node.min = '1'; node.max = '10000000'; node.step = '1'; return node;
        };
        const context = number(m.contextWindow, value => { m.contextWindow = value; });
        const output = number(m.maxTokens, value => { m.maxTokens = value; });
        const check = (text: string, checked: boolean, change: (value: boolean) => void) => {
          const node = el('input'); node.type = 'checkbox'; node.checked = checked; node.onchange = () => { change(node.checked); dirty(); };
          const label = el('label', 'settings-check'); label.append(node, document.createTextNode(text)); return label;
        };
        const flags = el('div', 'model-flags'); flags.append(check('Reasoning', m.reasoning, value => { m.reasoning = value; }), check('Image input', m.vision, value => { m.vision = value; }));
        const remove = button('Remove', () => { c.models.splice(i, 1); renderModels(); refresh(); dirty(); }, 'model-quiet');
        remove.setAttribute('aria-label', `Remove model ${m.id || i + 1}`); remove.disabled = c.models.length === 1;
        row.append(field('Model ID', id), field('Context tokens', context), field('Output tokens', output), flags, remove); return row;
      }));
      addModel.disabled = c.models.length >= MAX_MODELS;
    }
    const addModel = button('Add model', () => { if (c.models.length >= MAX_MODELS) return; c.models.push(newModel()); renderModels(); refresh(); models.lastElementChild?.querySelector<HTMLInputElement>('input')?.focus(); dirty(); });
    const remove = button('Remove connection', () => {
      form.connections.splice(index, 1); expanded.delete(c.id); render();
      // Keep assignments until explicitly changed, so deleting a connection cannot silently change the next model.
      dirty();
    }, 'model-remove-connection');
    const footer = el('div', 'model-connection-foot'); footer.append(addModel, remove);
    renderModels(); body.append(grid, modelsTitle, modelsHint, models, footer);
    card.append(head, unused, body);

    function paintOpen() {
      const open = expanded.has(c.id);
      body.hidden = !open; card.dataset.open = String(open);
      toggle.textContent = open ? 'Close' : 'Edit'; toggle.setAttribute('aria-expanded', String(open));
      toggle.setAttribute('aria-label', `${open ? 'Close' : 'Edit'} ${nameOf(edit)}`);
    }
    function paint() {
      title.textContent = nameOf(edit);
      const host = connectionHost(c.baseUrl); const count = c.models.filter(m => m.id.trim()).length;
      const key = edit.removeKey ? 'key removed on Save' : edit.keyInput.trim() || edit.hasApiKey ? 'API key saved' : 'no API key';
      meta.textContent = [host || 'no base URL yet', plural(count, 'model'), key].join(' · ');
      const users = usedBy(form, c.id).map(h => HARNESS_NAMES[h]);
      card.dataset.state = users.length ? 'used' : 'unused';
      status.textContent = users.length ? `Used by ${list(users)}` : 'Not used';
      const ready = firstModelId(c) !== null;
      unused.hidden = users.length > 0;
      unusedText.textContent = ready ? 'No agent starts with this connection yet.' : 'Add a model ID, then choose which agent uses it.';
      quick.forEach(node => { node.hidden = !ready; });
      toggle.setAttribute('aria-label', `${expanded.has(c.id) ? 'Close' : 'Edit'} ${nameOf(edit)}`);
    }
    paints.push(paint); paint(); paintOpen();
    return card;
  }
  function render() {
    paints = [];
    connections.replaceChildren(...form.connections.map(connectionCard)); empty.hidden = Boolean(form.connections.length);
    add.disabled = form.connections.length >= MAX_CONNECTIONS; renderAssignments();
  }
  return {
    element,
    setDisabled(disabled: boolean) { element.disabled = disabled || !loaded; },
    async load() {
      loaded = false; element.disabled = true;
      const view = parseModelView(await invoke('model_settings_load'));
      if (!view) throw new Error('The shared model settings could not be read. Reload to try again.');
      form = fromView(view); expanded.clear(); loaded = true; render(); element.disabled = false;
    },
    prepare(): Save {
      if (!loaded) throw new Error('Reload shared model settings before saving.');
      const built = buildModelSave(form); if ('problem' in built) throw new Error(built.problem); return built.request;
    },
    async save(request: Save) {
      const view = parseModelView(await invoke('model_settings_save', { request }));
      if (!view) throw new Error('The shared model settings could not be confirmed. Reload before saving again.');
      form = fromView(view); render();
    },
    clearSecrets() {
      for (const c of form.connections) { c.keyInput = ''; c.removeKey = false; }
      element.querySelectorAll<HTMLInputElement>('.model-api-key').forEach(input => { input.value = ''; });
    },
  };
}

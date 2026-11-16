import { APIS, HARNESSES, MAX_CONNECTIONS, MAX_MODELS, PRESETS, buildModelSave, emptyForm, fromView, newModel, parseModelView,
  type Edit, type Harness, type Save } from './model-settings-model.ts';
import './model-settings-panel.css';

type Invoke = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className = '', text?: string) => {
  const node = document.createElement(tag); node.className = className; if (text !== undefined) node.textContent = text; return node;
};
const names: Record<Harness, string> = { opencode: 'OpenCode', pi: 'Pi', hermes: 'Hermes' };
const apiNames = ['OpenAI Chat Completions', 'OpenAI Responses', 'Anthropic Messages', 'Google Generative AI'];
function button(text: string, click: () => void) { const node = el('button', '', text); node.type = 'button'; node.onclick = click; return node; }
function field(text: string, input: HTMLElement) {
  const label = el('label', 'model-field'); label.append(el('span', '', text), input); return label;
}
function option(value: string, text: string) { const node = el('option', '', text); node.value = value; return node; }

/** Edits shared connections; the containing Settings drawer owns loading and the Save action. */
export function createModelSettingsPanel(invoke: Invoke, changed: (message: string) => void) {
  const element = el('fieldset', 'model-settings'); element.disabled = true;
  const intro = el('p', 'hint', 'Save a connection once and reuse it in OpenCode, Pi or Hermes. Keys are encrypted for this Windows user and are never displayed again. Changes apply to new sessions.');
  const connections = el('div', 'model-connections');
  const empty = el('p', 'model-empty', 'No shared connections yet. Add your provider or a local endpoint below.');
  const addRow = el('div', 'settings-inline model-add');
  const preset = el('select'); preset.id = 'models-preset'; preset.setAttribute('aria-label', 'New connection provider');
  PRESETS.forEach((p, i) => preset.append(option(String(i), p.name)));
  const add = button('Add connection', () => {
    if (form.connections.length >= MAX_CONNECTIONS) return;
    const selected = PRESETS[Number(preset.value)] ?? PRESETS[0];
    const edit: Edit = { connection: { id: crypto.randomUUID(), ...selected, models: [newModel()] }, hasApiKey: false, keyInput: '', removeKey: false };
    form.connections.push(edit); render();
    const last = connections.lastElementChild;
    if (last instanceof HTMLDetailsElement) { last.open = true; last.querySelector<HTMLInputElement>('.model-id')?.focus(); }
    dirty();
  }); add.id = 'models-add-connection'; addRow.append(preset, add);
  const heading = el('h3', '', 'Use for new sessions');
  const assignments = el('div', 'model-assignments');
  const note = el('p', 'hint', 'Hermes: selection saved only. Launching Hermes from Shinatama is not available yet. Native settings keeps the harness’s own model and sign-in.');
  const limits = el('p', 'hint', 'Enter exact model IDs from your provider. Context/output limits and capabilities are editable; the starting values are placeholders to adjust for your model. Pi custom-provider cost estimates are not configured here.');
  element.append(intro, connections, empty, addRow, limits, heading, assignments, note);
  let form = emptyForm(); let loaded = false;
  const dirty = () => changed('Model changes are not saved yet. Press Save to apply them to new sessions.');
  function input(value: string, change: (value: string) => void, maxLength = 200) {
    const node = el('input'); node.value = value; node.maxLength = maxLength; node.autocomplete = 'off'; node.spellcheck = false;
    node.oninput = () => { change(node.value); dirty(); }; return node;
  }
  function renderAssignments() {
    assignments.replaceChildren(...HARNESSES.map(harness => {
      const row = el('div', 'model-assignment'); row.dataset.harness = harness;
      const provider = el('select'); provider.setAttribute('aria-label', `${names[harness]} connection`);
      provider.append(option('', 'Native settings'));
      form.connections.forEach(c => provider.append(option(c.connection.id, c.connection.name || 'Unnamed connection')));
      const selected = form.assignments[harness];
      if (selected && !form.connections.some(c => c.connection.id === selected.connectionId)) provider.append(option(selected.connectionId, 'Removed connection · choose again'));
      provider.value = selected?.connectionId ?? '';
      const model = el('select'); model.setAttribute('aria-label', `${names[harness]} model`);
      const connection = form.connections.find(c => c.connection.id === selected?.connectionId)?.connection;
      model.append(option('', connection ? 'Choose a model' : 'Uses native model'));
      connection?.models.filter(m => m.id.trim()).forEach(m => model.append(option(m.id.trim(), m.id.trim())));
      if (selected?.modelId && connection && !connection.models.some(m => m.id.trim() === selected.modelId)) model.append(option(selected.modelId, 'Removed model · choose again'));
      model.value = selected?.modelId ?? ''; model.disabled = !connection;
      provider.onchange = () => {
        const c = form.connections.find(c => c.connection.id === provider.value)?.connection;
        form.assignments[harness] = c ? { connectionId: c.id, modelId: c.models.find(m => m.id.trim())?.id.trim() ?? '' } : null;
        renderAssignments(); dirty();
      };
      model.onchange = () => { if (form.assignments[harness]) form.assignments[harness]!.modelId = model.value; dirty(); };
      row.append(el('strong', '', names[harness]), field('Connection', provider), field('Model', model)); return row;
    }));
  }
  function connectionCard(edit: Edit, index: number) {
    const c = edit.connection;
    const card = el('details', 'model-connection'); card.open = form.connections.length === 1;
    const summary = el('summary'); const title = el('strong', '', c.name || 'New connection');
    const count = el('span', 'hint', `${c.models.length} model${c.models.length === 1 ? '' : 's'}`);
    summary.append(title, count);
    const body = el('div', 'model-connection-body');
    const grid = el('div', 'model-fields');
    const name = input(c.name, value => { c.name = value; title.textContent = value || 'New connection'; renderAssignments(); }, 100);
    const api = el('select'); api.setAttribute('aria-label', `API protocol for ${c.name}`);
    APIS.forEach((value, i) => api.append(option(value, apiNames[i]))); api.value = c.api;
    api.onchange = () => { c.api = APIS.find(value => value === api.value) ?? APIS[0]; dirty(); };
    const url = input(c.baseUrl, value => { c.baseUrl = value; }, 2048); url.type = 'url'; url.placeholder = 'https://provider.example/v1';
    const urlField = field('Base URL', url); urlField.classList.add('model-wide');
    grid.append(field('Connection name', name), field('API protocol', api), urlField);
    const keyRow = el('div', 'settings-inline');
    const key = input(edit.keyInput, value => { edit.keyInput = value; }, 4096); key.type = 'password'; key.autocomplete = 'new-password'; key.className = 'model-api-key';
    const paintKey = () => { key.placeholder = edit.removeKey ? 'Will be removed on Save' : edit.hasApiKey ? 'Saved · type to replace' : 'Optional for local endpoints'; };
    const removeKey = button('Remove key', () => { edit.removeKey = true; edit.keyInput = ''; key.value = ''; paintKey(); removeKey.hidden = true; dirty(); });
    removeKey.hidden = !edit.hasApiKey || edit.removeKey; paintKey(); keyRow.append(key, removeKey);
    key.setAttribute('aria-label', `API key for ${c.name}`);
    const keyField = el('div', 'model-field model-wide'); keyField.append(el('span', '', 'API key'), keyRow); grid.append(keyField);
    const models = el('div', 'model-list');
    function renderModels() {
      models.replaceChildren(...c.models.map((m, i) => {
        const row = el('div', 'model-entry');
        const id = input(m.id, value => {
          const previous = m.id.trim(); m.id = value;
          for (const h of HARNESSES) { const s = form.assignments[h]; if (s?.connectionId === c.id && s.modelId === previous) s.modelId = value.trim(); }
          renderAssignments();
        }); id.className = 'model-id'; id.placeholder = 'Exact model ID';
        const number = (value: number, change: (value: number) => void) => {
          const node = input(String(value), text => change(Number(text)), 8); node.type = 'number'; node.min = '1'; node.max = '10000000'; node.step = '1'; return node;
        };
        const context = number(m.contextWindow, value => { m.contextWindow = value; });
        const output = number(m.maxTokens, value => { m.maxTokens = value; });
        const check = (text: string, checked: boolean, change: (value: boolean) => void) => {
          const node = el('input'); node.type = 'checkbox'; node.checked = checked; node.onchange = () => { change(node.checked); dirty(); };
          const label = el('label', 'settings-check'); label.append(node, document.createTextNode(text)); return label;
        };
        const flags = el('div', 'model-flags'); flags.append(check('Reasoning', m.reasoning, value => { m.reasoning = value; }), check('Images', m.vision, value => { m.vision = value; }));
        const remove = button('Remove', () => { c.models.splice(i, 1); renderModels(); renderAssignments(); dirty(); }); remove.setAttribute('aria-label', `Remove model ${m.id || i + 1}`);
        row.append(field('Model ID', id), field('Context tokens', context), field('Output tokens', output), flags, remove); return row;
      }));
      count.textContent = `${c.models.length} model${c.models.length === 1 ? '' : 's'}`;
      addModel.disabled = c.models.length >= MAX_MODELS;
    }
    const addModel = button('Add model', () => { if (c.models.length >= MAX_MODELS) return; c.models.push(newModel()); renderModels(); models.lastElementChild?.querySelector<HTMLInputElement>('input')?.focus(); dirty(); });
    const remove = button('Remove connection', () => {
      form.connections.splice(index, 1); render();
      // Keep assignments until explicitly changed, so deleting a connection cannot silently change the next model.
      dirty();
    }); remove.className = 'model-remove-connection';
    renderModels(); body.append(grid, models, addModel, remove); card.append(summary, body); return card;
  }
  function render() {
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
      form = fromView(view); loaded = true; render(); element.disabled = false;
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

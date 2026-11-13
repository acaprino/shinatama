import { openModal } from './modal.ts';
import { buildSave, formFromView, parseView, summary, MAX_VARIABLES, type Form, type Row, type SaveRequest } from './settings-model.ts';
import './settings-panel.css';

type Options = {
  desktop: boolean;
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  notify(message: unknown, tone?: 'info' | 'error'): void;
  restoreFocus?: () => void;
};

const el = <K extends keyof HTMLElementTagNameMap>(tag: K, className?: string, text?: string) => {
  const node = document.createElement(tag); if (className) node.className = className; if (text !== undefined) node.textContent = text; return node;
};
const message = (error: unknown) => typeof error === 'string' ? error : (error as { message?: string } | null)?.message ?? 'The settings could not be saved.';

/** Settings drawer: full access, a GitHub sign-in, and variables every new agent session receives.
 * Saved secrets never come back to this page; they can be replaced or removed. */
export function createSettingsPanel(options: Options) {
  const dialog = el('dialog'); dialog.id = 'settings-dialog'; dialog.setAttribute('aria-labelledby', 'settings-title');
  const heading = el('div', 'dialog-heading'); const title = el('h1', undefined, 'Settings'); title.id = 'settings-title';
  const closeButton = el('button', undefined, '×'); closeButton.type = 'button'; closeButton.setAttribute('aria-label', 'Close settings');
  heading.append(title, closeButton);

  // Agent access
  const access = el('section', 'settings-section'); access.append(el('h2', undefined, 'Agent access'));
  const fullLabel = el('label', 'settings-check'); const full = el('input'); full.type = 'checkbox'; full.id = 'settings-full-access';
  fullLabel.append(full, document.createTextNode(' Full access: start agents without their own protections'));
  const fullHint = el('p', 'hint', 'Claude starts in bypass-permissions mode, Codex without its sandbox and approvals, OpenCode allows every tool. '
    + 'Pi and PowerShell never asked. An agent can then change or delete anything your Windows account can, inside or outside the project. '
    + 'The private home keeps their settings and sign-ins apart; it does not confine their files. Applies to sessions started after you save.');
  access.append(fullLabel, fullHint);

  // GitHub
  const github = el('section', 'settings-section'); github.append(el('h2', undefined, 'GitHub'));
  const userLabel = el('label', undefined, 'Username'); userLabel.htmlFor = 'settings-github-user';
  const user = el('input'); user.id = 'settings-github-user'; user.autocomplete = 'off'; user.spellcheck = false; user.maxLength = 39;
  const tokenLabel = el('label', undefined, 'Personal access token'); tokenLabel.htmlFor = 'settings-github-token';
  const token = el('input'); token.id = 'settings-github-token'; token.type = 'password'; token.autocomplete = 'new-password'; token.spellcheck = false; token.maxLength = 255;
  const tokenRow = el('div', 'settings-inline'); const removeToken = el('button', undefined, 'Remove token'); removeToken.type = 'button'; removeToken.id = 'settings-github-remove';
  tokenRow.append(token, removeToken);
  const githubHint = el('p', 'hint', 'GitHub no longer accepts an account password, so use a personal access token (a fine-grained one limited to the repositories you need is safest). '
    + 'Agents receive it as GH_TOKEN and as the Git credential for github.com, so git push and gh never ask. Stored encrypted for this Windows user; any agent you start can read it.');
  github.append(userLabel, user, tokenLabel, tokenRow, githubHint);

  // Variables
  const variables = el('section', 'settings-section'); variables.append(el('h2', undefined, 'Environment variables'));
  const list = el('div', 'settings-vars'); list.setAttribute('role', 'list');
  const add = el('button', undefined, 'Add variable'); add.type = 'button'; add.id = 'settings-add-variable';
  const varsHint = el('p', 'hint', 'Added to every new agent session. A secret value is stored encrypted and is never shown again; type a new one to replace it.');
  variables.append(list, add, varsHint);

  const status = el('p', 'settings-status'); status.setAttribute('role', 'status'); status.id = 'settings-status';
  const actions = el('div', 'dialog-actions');
  const reload = el('button', undefined, 'Reload'); reload.type = 'button';
  const save = el('button', 'primary', 'Save'); save.type = 'button'; save.id = 'settings-save';
  const close = el('button', undefined, 'Close'); close.type = 'button';
  actions.append(reload, save, close);
  dialog.append(heading, access, github, variables, status, actions);
  (document.querySelector('#app') ?? document.body).append(dialog);

  let form: Form = { fullAccess: true, githubUsername: '', tokenInput: '', removeToken: false, hasToken: false, rows: [] };
  let loaded = false; let busy = false;

  const say = (text: string, error = false) => { status.textContent = text; status.dataset.tone = error ? 'error' : 'info'; };
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
    const line = el('div', 'settings-var'); line.setAttribute('role', 'listitem');
    const name = el('input'); name.placeholder = 'NAME'; name.value = row.name; name.setAttribute('aria-label', 'Variable name'); name.autocomplete = 'off'; name.spellcheck = false; name.maxLength = 128;
    name.oninput = () => { row.name = name.value; };
    const value = el('input'); value.value = row.value; value.setAttribute('aria-label', `Value of ${row.name || 'the variable'}`); value.autocomplete = 'off'; value.spellcheck = false;
    const paintValue = () => { value.type = row.secret ? 'password' : 'text'; value.placeholder = row.hasValue && !row.touched && row.secret ? 'Saved. Type to replace' : 'value'; };
    value.oninput = () => { row.value = value.value; row.touched = true; };
    const secretLabel = el('label', 'settings-secret'); const secret = el('input'); secret.type = 'checkbox'; secret.checked = row.secret;
    secret.onchange = () => { row.secret = secret.checked; paintValue(); };
    secretLabel.append(secret, document.createTextNode(' Secret'));
    const remove = el('button', undefined, '×'); remove.type = 'button'; remove.setAttribute('aria-label', `Remove ${row.name || 'variable'}`);
    remove.onclick = () => { form.rows.splice(index, 1); paintRows(); };
    paintValue();
    line.append(name, value, secretLabel, remove);
    return line;
  }
  function paint() {
    full.checked = form.fullAccess; user.value = form.githubUsername; token.value = form.tokenInput;
    paintToken(); paintRows();
  }
  function read() { form.fullAccess = full.checked; form.githubUsername = user.value; form.tokenInput = token.value; }

  async function load() {
    if (!options.desktop) { say('Open the desktop app to change settings.', true); save.disabled = true; return; }
    busy = true; say('Loading…');
    try {
      const view = parseView(await options.invoke('launch_settings_load'));
      if (!view) throw new Error('The saved settings are not in a form this version understands.');
      form = formFromView(view); loaded = true; save.disabled = false; paint();
      say(`New sessions start with: ${summary(form)}.`);
    } catch (error) {
      loaded = false; save.disabled = false; say(`${message(error)} Agents start with their own protections until you save.`, true);
    } finally { busy = false; }
  }
  async function submit() {
    if (busy || !options.desktop) return;
    read();
    const built = buildSave(form);
    if ('problem' in built) { say(built.problem, true); return; }
    busy = true; save.disabled = true; say('Saving…');
    try {
      const view = parseView(await options.invoke<unknown>('launch_settings_save', { request: built.request satisfies SaveRequest }));
      if (!view) throw new Error('The saved settings could not be confirmed.');
      form = formFromView(view); loaded = true; paint();
      say(`Saved. New sessions start with: ${summary(form)}.`);
    } catch (error) { say(message(error), true); options.notify(message(error), 'error'); }
    finally { busy = false; save.disabled = false; }
  }

  full.onchange = () => { form.fullAccess = full.checked; say(`Not saved yet: ${summary({ ...form, fullAccess: full.checked })}.`); };
  token.oninput = () => { form.tokenInput = token.value; paintToken(); };
  removeToken.onclick = () => { form.removeToken = true; token.value = ''; form.tokenInput = ''; paintToken(); say('The token will be removed when you save.'); };
  add.onclick = () => {
    if (form.rows.length >= MAX_VARIABLES) return;
    form.rows.push({ name: '', secret: false, value: '', hasValue: false, touched: true }); paintRows();
    list.querySelector<HTMLInputElement>('.settings-var:last-child input')?.focus();
  };
  save.onclick = () => { void submit(); };
  reload.onclick = () => { if (!busy) void load(); };
  closeButton.onclick = close.onclick = () => dialog.close();
  dialog.addEventListener('close', () => { token.value = ''; form.tokenInput = ''; if (!document.querySelector('dialog[open]')) options.restoreFocus?.(); });
  dialog.addEventListener('submit', event => event.preventDefault());

  return { show() { if (!dialog.open) openModal(dialog); if (!loaded || !busy) void load(); } };
}

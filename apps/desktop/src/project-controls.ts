import { componentElement, formHeaderHtml, uiButton, uiInput, uiPart } from '@acaprino/imago/ui';

function identified<T extends HTMLElement>(node: T, id: string) {
  node.id = id;
  return node;
}

export function projectControls() {
  const dialog = identified(uiPart('dialog'), 'project-dialog');
  dialog.setAttribute('aria-labelledby', 'project-dialog-title');
  const form = identified(uiPart('form'), 'project-form');
  const heading = componentElement(formHeaderHtml('Project roots',
    'A root is the folder that holds your projects. Every folder directly inside it appears as a project.',
    false, 'project-dialog-title'));
  heading.querySelector('[data-close]')!.setAttribute('data-cancel', '');
  const button = (id: string, label: string, primary = false) => uiButton(label, { primary, attrs: { id } });
  const field = (id: string, label: string, placeholder: string, type = 'text') => {
    const input = identified(uiInput(type), id);
    input.name = id;
    input.setAttribute('aria-label', label);
    input.placeholder = placeholder;
    input.spellcheck = false;
    input.autocomplete = 'off';
    return input;
  };
  const list = (id: string, label: string) => {
    const node = identified(uiPart('stack', undefined, 'im-list'), id);
    node.setAttribute('role', 'list');
    node.setAttribute('aria-label', label);
    node.setAttribute('aria-live', 'polite');
    return node;
  };
  const roots = identified(uiPart('region'), 'roots-panel');
  roots.setAttribute('aria-label', 'Your project roots');
  const add = uiPart('row');
  add.append(field('root-path', 'Root folder path', 'Paste a root folder, e.g. D:\\Projects'),
    button('add-root', 'Add'), button('choose-root', 'Choose root folder…', true));
  roots.append(list('root-list', 'Root folders'), add, identified(uiPart('hint'), 'default-root-summary'));
  const catalog = uiPart('region');
  const title = uiPart('header');
  title.append(uiPart('heading', 'Projects in your roots'), button('refresh-projects', 'Rescan'));
  const filter = field('project-filter', 'Filter projects', 'Filter by project name or folder ( / )', 'search');
  filter.maxLength = 128;
  const pagination = identified(uiPart('row'), 'catalog-pagination'); pagination.hidden = true;
  pagination.append(button('catalog-previous', 'Previous'), identified(uiPart('text'), 'catalog-page'), button('catalog-next', 'Next'));
  catalog.append(title, filter, identified(uiPart('status'), 'catalog-state'), list('catalog-project-list', 'Projects in roots'),
    pagination, identified(uiPart('heading', 'Single folders'), 'recent-project-heading'), list('recent-project-list', 'Recent folders'));
  const single = uiPart('region');
  const folder = field('folder', 'Folder to open as a project', 'D:\\Projects\\my-app'); folder.required = true;
  const submit = uiButton('Use folder'); submit.type = 'submit';
  const row = uiPart('row');
  row.append(folder, button('browse-project', 'Browse… F3'), submit);
  single.append(identified(uiPart('heading', 'Just one folder'), 'single-folder-heading'),
    uiPart('hint', 'Opens one folder as a project, without adding a root.'), row);
  const error = identified(uiPart('alert'), 'project-error'); error.hidden = true;
  const actions = uiPart('actions');
  const close = uiButton('Close'); close.setAttribute('data-cancel', '');
  actions.append(button('create-project', 'New project (n)'), close);
  form.append(heading, roots, catalog, single, error, actions);
  dialog.append(form);
  return dialog;
}

export function stopControls() {
  const dialog = identified(uiPart('dialog'), 'confirm-dialog');
  dialog.setAttribute('aria-labelledby', 'confirm-title');
  dialog.setAttribute('aria-describedby', 'confirm-message');
  const form = uiPart('form');
  form.method = 'dialog';
  const actions = uiPart('actions');
  const cancel = uiButton('Cancel'); cancel.value = 'cancel'; cancel.type = 'submit'; cancel.autofocus = true;
  const stop = uiButton('Stop', { danger: true }); stop.value = 'stop'; stop.type = 'submit';
  actions.append(cancel, stop);
  form.append(identified(uiPart('heading', 'Stop terminal?'), 'confirm-title'),
    identified(uiPart('paragraph'), 'confirm-message'), actions);
  dialog.append(form);
  return dialog;
}

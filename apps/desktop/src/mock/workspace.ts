// The answers of the browser for the commands of this area (see mock/index.ts): the saved layout of the window. It starts with three
// panes (two terminals side by side, and under the second the thread's conversation and its team map), the way a person left them, and
// keeps what the page saves, so a layout change in the browser is saved and read again as it is with Rust.
import { parseWorkspaceLayout, serializeWorkspaceLayout, type WorkspaceLayout } from '../workspace-model.ts';
import type { Answers } from './index.ts';

const size = { width: 640, height: 480 };
const terminal = (id: string, threadId: string) => ({ id, resource: { kind: 'terminal', instanceId: id, threadId, profileId: null }, hidden: false });

const SEED = {
  version: 1,
  root: {
    kind: 'split', id: 'split-root', axis: 'columns', weights: [3, 2], children: [
      { kind: 'group', id: 'pane-main', tabs: ['i_aaaaaaaaaaaa', 'i_aaaaaaaaaaab', 'i_aaaaaaaaaaac', 'saved-t_aaaaaaaaaaae'], activeTabId: 'i_aaaaaaaaaaaa', collapsed: false, lastUsableSize: size },
      { kind: 'split', id: 'split-side', axis: 'rows', weights: [3, 2], children: [
        { kind: 'group', id: 'pane-cleaner', tabs: ['i_bbbbbbbbbbba'], activeTabId: 'i_bbbbbbbbbbba', collapsed: false, lastUsableSize: size },
        { kind: 'group', id: 'pane-room', tabs: ['chat-w_aaaaaaaaaaaa', 'team-w_aaaaaaaaaaaa'], activeTabId: 'chat-w_aaaaaaaaaaaa', collapsed: false, lastUsableSize: size },
      ] },
    ],
  },
  views: [
    terminal('i_aaaaaaaaaaaa', 't_aaaaaaaaaaaa'), terminal('i_aaaaaaaaaaab', 't_aaaaaaaaaaab'), terminal('i_aaaaaaaaaaac', 't_aaaaaaaaaaac'),
    terminal('i_bbbbbbbbbbba', 't_bbbbbbbbbbba'),
    { id: 'saved-t_aaaaaaaaaaae', resource: { kind: 'terminal', instanceId: null, threadId: 't_aaaaaaaaaaae', profileId: null }, hidden: false },
    { id: 'chat-w_aaaaaaaaaaaa', resource: { kind: 'chat', resourceId: 'chat-w_aaaaaaaaaaaa' }, hidden: false },
    { id: 'team-w_aaaaaaaaaaaa', resource: { kind: 'team', resourceId: 'team-w_aaaaaaaaaaaa' }, hidden: false },
  ],
  focusedGroupId: 'pane-main',
  maximizedGroupId: null,
};

const seeded = parseWorkspaceLayout(SEED);
const json = (layout: WorkspaceLayout): string => { const text = serializeWorkspaceLayout(layout); return text.ok ? text.value : ''; };

let saved = { revision: 1, layoutJson: seeded.ok ? json(seeded.value) : '', ownership: 'unclaimed' };
const snapshot = () => ({ workspaceId: 'main', revision: saved.revision, schemaVersion: 1, layoutJson: saved.layoutJson, ownership: saved.ownership, readOnly: false });

export const answers: Answers = {
  global_workspace_load: () => seeded.ok ? snapshot() : null,
  global_workspace_save: args => {
    const request = (args as { request: { layoutJson: string } }).request;
    saved = { revision: saved.revision + 1, layoutJson: request.layoutJson, ownership: 'ours' };
    return snapshot();
  },
  global_workspace_release: () => { saved = { ...saved, ownership: 'unclaimed' }; return null; },
  global_workspace_recover: null,
  workspace_allow_close: null,
  interface_reload_ready: null,
  app_relaunch: null,
};

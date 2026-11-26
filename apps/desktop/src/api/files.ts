// The files of the project (browse, search, read, edit, save). The pages of the browser tab are in browser.ts.
import { invoke } from './core.ts';

export const filesApi = {
  list: (project: string, path: string, requestId: string) => invoke<unknown>('files_list', { project, path, requestId }),
  search: (project: string, query: string, requestId: string) => invoke<unknown>('files_search', { project, query, requestId }),
  cancel: (requestId: string) => invoke<void>('files_cancel', { requestId }),
  read: (project: string, path: string) => invoke<unknown>('files_read', { project, path }),
  save: (request: unknown) => invoke<unknown>('files_save', { request }),
  image: (project: string, path: string) => invoke<unknown>('files_image', { project, path }),
  markdown: (text: string) => invoke<unknown>('files_markdown', { text }),
  /** Kept for the saved data of the visual editor; the editor itself is gone (Source and Preview stay). */
  visualParse: (text: string) => invoke<unknown>('files_visual_parse', { text }),
  visualApply: (text: string, edits: unknown[]) => invoke<string>('files_visual_apply', { text, edits }),
};

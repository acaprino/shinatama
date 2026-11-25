// The files of the project (browse, search, read, edit, save) and the pages of the browser tab (ADR-0056).
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

export const browserApi = {
  open: (target: { url?: string; file?: string }) => invoke<unknown>('browser_open', target),
  info: (page: string) => invoke<unknown>('browser_info', { page }),
  frames: (page: string) => invoke<void>('browser_frames', { page }),
  navigate: (page: string, action: string, url?: string) => invoke<void>('browser_navigate', { page, action, url }),
  input: (page: string, events: unknown[]) => invoke<void>('browser_input', { page, events }),
  resize: (page: string, width: number, height: number, dpr: number) => invoke<void>('browser_resize', { page, width, height, dpr }),
  visibility: (page: string, visible: boolean) => invoke<void>('browser_visibility', { page, visible }),
  control: (page: string, take: boolean) => invoke<unknown>('browser_control', { page, take }),
  close: (page: string) => invoke<void>('browser_close', { page }),
  external: (page: string) => invoke<void>('browser_external', { page }),
};

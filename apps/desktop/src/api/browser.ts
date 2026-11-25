// The pages of the browser tab (ADR-0056): a page Rust drives through the browser engine. Every command of src-tauri/src/browser.rs.
import { Channel } from '@tauri-apps/api/core';
import type { NavigateAction, PageInfo } from '../browser-model.ts';
import { inTauri, invoke } from './core.ts';

export const browserApi = {
  open: (target: { url?: string; file?: string }) => invoke<PageInfo>('browser_open', target),
  info: (page: string) => invoke<PageInfo>('browser_info', { page }),
  /** From now on every frame of the page (a JPEG) goes to `onFrame`; a second call replaces the first. Outside the window the mock gets a plain object with `onmessage`. */
  frames: (page: string, onFrame: (bytes: ArrayBuffer) => void) => {
    const channel: { onmessage?: (bytes: ArrayBuffer) => void } = inTauri ? new Channel<ArrayBuffer>() : {};
    channel.onmessage = onFrame;
    return invoke<void>('browser_frames', { page, channel });
  },
  navigate: (page: string, action: NavigateAction, url?: string) => invoke<void>('browser_navigate', { page, action, url }),
  input: (page: string, events: readonly unknown[]) => invoke<void>('browser_input', { page, events }),
  resize: (page: string, width: number, height: number, dpr: number) => invoke<void>('browser_resize', { page, width, height, dpr }),
  visibility: (page: string, visible: boolean) => invoke<void>('browser_visibility', { page, visible }),
  control: (page: string, take: boolean) => invoke<PageInfo>('browser_control', { page, take }),
  close: (page: string) => invoke<void>('browser_close', { page }),
  external: (page: string) => invoke<void>('browser_external', { page }),
};

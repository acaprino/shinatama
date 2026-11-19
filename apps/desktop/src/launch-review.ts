import { invoke } from '@tauri-apps/api/core';

export type LaunchAction = 'start' | 'resume';
export type LaunchPlacement = { program: 'system'; home: 'box' | 'system' } | { program: 'own'; home: 'box' };
export type LaunchIntent = { agent: string; action: LaunchAction; placement: LaunchPlacement };
export type LaunchSummary = {
  agent: string;
  action: LaunchAction;
  placement: LaunchPlacement;
  status: 'available' | 'refused' | 'incomplete';
  issues: { code: string; reason?: string | null }[];
  synthetic: boolean;
};

/** Selector-only review through the same engine service as CLI launch preflight.
 * This function returns a summary; it never starts or resumes a process. */
export function reviewLaunch(request: LaunchIntent): Promise<LaunchSummary> {
  return invoke<LaunchSummary>('launch_preflight', { request });
}

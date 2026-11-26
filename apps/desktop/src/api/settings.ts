// What the person sets and what the app watches over: terminal presentation, launch, models, memory, the proxy of the models, the
// alerts, Muro's sync, the storage and the resources.
import { invoke } from './core.ts';

/** The presentation preferences as Rust keeps them. `skin` stays in the saved data (always `shinatama`): the five skins are gone. */
export type Presentation = { version: 1; skin: string; followSkin: boolean; fontFamily: string; fontSize: number; lineHeight: number };
export type PresentationSnapshot = { revision: number; settings: Presentation };
export type StorageDiagnostics = { directory: string; database_path: string; read_only: boolean; message: string | null };

export const settingsApi = {
  presentationLoad: () => invoke<PresentationSnapshot>('presentation_load'),
  presentationSave: (expectedRevision: number, settings: Presentation) => invoke<PresentationSnapshot>('presentation_save', { request: { expectedRevision, settings } }),
  launchSettingsLoad: () => invoke<unknown>('launch_settings_load'),
  launchSettingsSave: (request: unknown) => invoke<unknown>('launch_settings_save', { request }),
  launchPreflight: (request: unknown) => invoke<unknown>('launch_preflight', { request }),
  modelSettingsLoad: () => invoke<unknown>('model_settings_load'),
  modelSettingsSave: (request: unknown) => invoke<unknown>('model_settings_save', { request }),
  memorySettingsLoad: () => invoke<unknown>('memory_settings_load'),
  memorySettingsSave: (request: unknown) => invoke<unknown>('memory_settings_save', { request }),
  muroStatus: () => invoke<unknown>('muro_status'),
  muroSyncNow: () => invoke<unknown>('muro_sync_now'),
  muroOpen: () => invoke<void>('muro_open'),
  sidecarStatus: () => invoke<unknown>('sidecar_status'),
  sidecarStart: () => invoke<unknown>('sidecar_start'),
  sidecarStop: (force: boolean) => invoke<unknown>('sidecar_stop', { force }),
  sidecarRetry: () => invoke<unknown>('sidecar_retry'),
  agentStatus: () => invoke<unknown>('agent_status'),
  agentCheck: (agentId: string) => invoke<unknown>('agent_check', { agentId }),
  attentionCurrent: () => invoke<unknown>('attention_current'),
  attentionConfigure: (enabled: boolean, muted: boolean) => invoke<unknown>('attention_configure', { enabled, muted }),
  attentionPreview: (kind: string) => invoke<unknown>('attention_preview', { kind }),
  attentionDismiss: (id: string) => invoke<void>('attention_dismiss', { id }),
  attentionOpen: (id: string) => invoke<void>('attention_open', { id }),
  maintenanceActivity: () => invoke<void>('maintenance_activity'),
  maintenanceStatus: () => invoke<unknown>('maintenance_status'),
  maintenanceCheck: () => invoke<unknown>('maintenance_check'),
  titlesEnabled: () => invoke<boolean>('titles_enabled'),
  titlesSetEnabled: (enabled: boolean) => invoke<void>('titles_set_enabled', { enabled }),
  storageLocation: () => invoke<StorageDiagnostics>('storage_location'),
  resourcesStatus: () => invoke<unknown>('resources_status'),
  resourcesTelemetry: (telemetry: unknown) => invoke<unknown>('resources_telemetry', { telemetry }),
};

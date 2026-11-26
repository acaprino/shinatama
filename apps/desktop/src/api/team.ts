// The team map of a thread (ADR-0058): who works with whom, templates, evaluation and placement.
import { invoke } from './core.ts';

export const teamApi = {
  get: (request: unknown) => invoke<unknown>('team_get', { request }),
  evaluate: (request: unknown) => invoke<unknown>('team_evaluate', { request }),
  apply: (request: unknown) => invoke<unknown>('team_apply', { request }),
  place: (request: unknown) => invoke<unknown>('team_place', { request }),
  templates: () => invoke<unknown>('team_templates'),
  templateSave: (request: unknown) => invoke<unknown>('team_template_save', { request }),
  templateDelete: (request: unknown) => invoke<unknown>('team_template_delete', { request }),
};

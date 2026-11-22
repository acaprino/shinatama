import { buttonHtml, componentElement, panelStateHtml } from '@acaprino/imago/ui';

/**
 * The state layer of Imago's panel states (loading bars, empty, error with Retry), for a list that is one section of a
 * view whose other sections stay usable: `setPanelState` takes the whole body of a panel, so a section writes the same
 * layer here, from `panelStateHtml`, and wires its button itself. The view whose body is the list uses `setPanelState`.
 */
export type SectionState =
  | { kind: 'loading'; rows: number }
  | { kind: 'empty'; title?: string; text: string; action?: { label: string; run: () => void } }
  | { kind: 'error'; text: string; retry: { label: string; run: () => void } };

export function stateLayer(state: SectionState): HTMLElement {
  const layer = state.kind === 'loading' ? componentElement(panelStateHtml({ kind: 'loading', rows: state.rows }))
    : state.kind === 'empty' ? componentElement(panelStateHtml({ kind: 'empty', ...(state.title ? { title: state.title } : {}), text: state.text, ...(state.action ? { actionHtml: buttonHtml(state.action.label, 'state-action') } : {}) }))
    : componentElement(panelStateHtml({ kind: 'error', text: state.text, retryHtml: buttonHtml(state.retry.label, 'state-retry') }));
  const run = state.kind === 'empty' ? state.action?.run : state.kind === 'error' ? state.retry.run : undefined;
  const button = layer.querySelector<HTMLButtonElement>('button');
  if (button && run) button.onclick = () => run();
  return layer;
}

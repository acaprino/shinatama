import { createTilingWorkspace, type TilingWorkspaceOptions } from '@acaprino/imago/ui';
import {
  keyboardNeighbor, MIN_PANE_SIZE, paneRects, parseWorkspaceLayout, reduceWorkspaceLayout,
  type LayoutAction, type LayoutError, type LayoutResult, type PanelView, type WorkspaceLayout,
} from './workspace-model.ts';

export { placeWorkspaceBinding } from '@acaprino/imago/ui';

export interface WorkspaceRenderer {
  render(layout: WorkspaceLayout): LayoutResult<void>;
  destroy(): void;
}

type WorkspaceOptions = TilingWorkspaceOptions<PanelView, WorkspaceLayout, LayoutError>;
export interface WorkspaceRendererOptions extends Omit<WorkspaceOptions,
  'model' | 'dispatch' | 'newTabLabel' | 'emptyActionLabel' | 'unavailableLabel'> {
  readonly dispatch: (action: LayoutAction, expectedLayout: WorkspaceLayout) => void;
}

export function createWorkspaceRenderer(options: WorkspaceRendererOptions): WorkspaceRenderer {
  return createTilingWorkspace<PanelView, WorkspaceLayout, LayoutError>({
    ...options,
    model: {
      minimumPaneSize: MIN_PANE_SIZE,
      validate: parseWorkspaceLayout,
      paneRects,
      keyboardNeighbor,
      preview(base, action) {
        const result = reduceWorkspaceLayout(base, action);
        return result.ok ? { ok: true, value: result.layout } : result;
      },
    },
    labelForView: options.labelForView ?? (view => view.resource.kind === 'terminal' ? 'Terminal' : view.resource.kind),
    unavailableLabel: view => view.resource.kind === 'terminal' ? 'This terminal is no longer running.' : 'This view is not available yet.',
    newTabLabel: 'New session in this pane',
    emptyActionLabel: '+ New session',
  });
}

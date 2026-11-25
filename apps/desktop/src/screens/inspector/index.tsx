// INSPECTOR (placeholder, Phase A). The side panel stack of the Workspace view and the Sessions view: the session detail, the files
// (a tree and the preview of the file, Source and Preview), the archives, what agents left on disk (leftovers), the instances, the settings
// of a thread and the preview of a hop. The panels are `PanelSpec` (./spec.ts); the one on top is `app.ui.panel`. Owned by the agent that
// fills the inspector in.
import { Inspector as Side } from '@acaprino/sigrid/layout';
import { Empty } from '@acaprino/sigrid/components';
import { useApp, useController } from '../../hooks.ts';

const TITLES = {
  session: 'Session', files: 'Files', archives: 'Archives', leftovers: 'Leftovers', instances: 'Instances', thread: 'Thread', hop: 'Hop',
} as const;

export function Inspector() {
  const app = useApp();
  const ui = useController(app.ui);
  const panel = ui.panel;
  if (!panel) return null;
  return (
    <Side title={TITLES[panel.kind]} open onOpenChange={open => { if (!open) ui.closePanel(); }}>
      <Empty title={TITLES[panel.kind]} description="This panel is written in the next phase." />
    </Side>
  );
}

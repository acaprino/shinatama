// The tools of the Workspace view in the title bar, before the switch of light and dark: a new session (the one solid action), the files of
// the project and what agents left on disk. Each is also a command of the palette (src/keys.ts).
import { Icon } from '@acaprino/sigrid/primitives';
import { ToolbarButton } from '@acaprino/sigrid/layout';
import { useApp } from '../../hooks.ts';

export function WorkspaceTools() {
  const app = useApp();
  return (
    <>
      <ToolbarButton variant="default" label="New session" shortcut="Ctrl Shift T" icon={<Icon name="plus" />} onClick={() => { void app.opens.newSession(); }}>New session</ToolbarButton>
      <ToolbarButton label="Files" icon={<Icon name="folder" />} onClick={() => app.files.open()}>Files</ToolbarButton>
      <ToolbarButton label="Leftovers" icon={<Icon name="clock" />} onClick={() => app.ui.openPanel({ kind: 'leftovers' })}>Leftovers</ToolbarButton>
    </>
  );
}

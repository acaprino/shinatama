// The tools of the Workspace view in the title bar, before the switch of light and dark: a new session (the one solid action), the files of
// the project and what agents left on disk (icons alone, to keep the bar short: their names are their hints). Each is also a command of the palette (src/keys.ts).
import { useMedia } from '@acaprino/sigrid';
import { Icon } from '@acaprino/sigrid/primitives';
import { ToolbarButton } from '@acaprino/sigrid/layout';
import { useApp } from '../../hooks.ts';

export function WorkspaceTools() {
  const app = useApp();
  // In a narrow window the bar keeps icons: the names stay as hints and for screen readers.
  const narrow = useMedia('(width < 620px)');
  return (
    <>
      <ToolbarButton variant="default" label="New session" shortcut="Ctrl Shift T" icon={<Icon name="plus" />} onClick={() => { void app.opens.newSession(); }}>{narrow ? null : 'New session'}</ToolbarButton>
      <ToolbarButton label="Files" icon={<Icon name="folder" />} onClick={() => app.files.open()} />
      <ToolbarButton label="Leftovers" icon={<Icon name="clock" />} onClick={() => app.ui.openPanel({ kind: 'leftovers' })} />
    </>
  );
}

// DIALOGS. The one dialog that is open (`app.ui.dialog`, a `DialogSpec`): a new session, a new project, the projects of the window (the
// picker and the overview), and the confirmations. A dialog holds its own form; the controller only says which one is open. The confirmation is
// Sigrid's `ConfirmDialog`: the confirming button carries the verb and the object, and the description names the consequence. What can be taken
// back is done at once and offers Undo in a notice, never a dialog.
import { ConfirmDialog } from '@acaprino/sigrid/components';
import { useApp, useController } from '../../hooks.ts';
import { NewProject } from './new-project.tsx';
import { NewSession } from './new-session.tsx';
import { ProjectsOverview } from './overview.tsx';
import { ProjectPicker } from './picker.tsx';

export function Dialogs() {
  const app = useApp();
  const ui = useController(app.ui);
  const dialog = ui.dialog;
  if (!dialog) return null;
  switch (dialog.kind) {
    case 'confirm':
      return (
        <ConfirmDialog
          open
          onOpenChange={open => { if (!open) ui.closeDialog(); }}
          title={dialog.title}
          description={dialog.description}
          confirmLabel={dialog.confirm}
          destructive={dialog.danger}
          onConfirm={() => ui.answer(true)}
        />
      );
    case 'new-session': return <NewSession spec={dialog} />;
    case 'new-project': return <NewProject />;
    case 'project-picker': return <ProjectPicker {...(dialog.browse ? { browse: true } : {})} />;
    case 'projects-overview': return <ProjectsOverview />;
  }
}

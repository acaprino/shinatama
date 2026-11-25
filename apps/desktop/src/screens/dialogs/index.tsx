// DIALOGS. The one dialog that is open (`app.ui.dialog`, a `DialogSpec`): a new session, a new project, the project picker and overview, a
// hop, and the confirmations. The confirmation is real already (Sigrid's `ConfirmDialog`): the confirming button carries the verb and the
// object, and the description names the consequence. The others are placeholders (Phase A), owned by the agent that fills the dialogs in.
import { ConfirmDialog, Dialog } from '@acaprino/sigrid/components';
import { Empty } from '@acaprino/sigrid/components';
import { useApp, useController } from '../../hooks.ts';

export function Dialogs() {
  const app = useApp();
  const ui = useController(app.ui);
  const dialog = ui.dialog;
  if (!dialog) return null;
  if (dialog.kind === 'confirm') {
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
  }
  return (
    <Dialog open onOpenChange={open => { if (!open) ui.closeDialog(); }} title={dialog.kind}>
      <Empty title={dialog.kind} description="This dialog is written in the next phase." />
    </Dialog>
  );
}

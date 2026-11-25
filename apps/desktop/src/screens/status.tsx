// The status strip: the runtime (a badge and a sentence), what runs, and Muro's sync at the end. One live region: the shell reads it when the
// phrase changes.
import { Badge, Spacer, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../hooks.ts';

export function Status() {
  const app = useApp();
  const ui = useController(app.ui);
  useController(app.terminals);
  useController(app.threads);
  const running = [...app.terminals.sessions.values()].filter(session => !session.ended && !session.disposed).length;
  const { runtime } = ui;
  return (
    <>
      <Badge tone={runtime.failed ? 'danger' : runtime.readOnly ? 'warning' : 'success'}>{runtime.failed ? 'Error' : runtime.readOnly ? 'Read-only' : 'Ready'}</Badge>
      <Text truncate title={runtime.title || undefined}>{runtime.text}</Text>
      <Spacer />
      <Text variant="muted">{`${running} running · ${app.threads.openWorksets.length} threads on`}</Text>
    </>
  );
}

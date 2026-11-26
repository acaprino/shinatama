// The index of session text that the Sessions view searches: how much is indexed and how the last run went, and the buttons that index,
// stop and forget. Indexing only reads the session archives; it never starts or resumes an agent. Whether indexing is allowed at all is a
// setting.
import { useEffect } from 'react';
import { Note } from '@acaprino/sigrid/components';
import { Button, Cluster, Text, TextList } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { indexButtons, indexLines } from '../../session-index-model.ts';
import { Frame } from './frame.tsx';

export function SearchIndexPanel() {
  const app = useApp();
  const sessions = useController(app.sessions);
  useEffect(() => { void app.sessions.refreshIndex(); }, [app]);
  const { index, indexBusy } = sessions;
  const buttons = indexButtons(index, indexBusy);
  return (
    <Frame title="Search index">
      <Text variant="muted">Searches the text you and the agents wrote in indexed sessions. Indexing only reads your session archives; it never starts or resumes an agent.</Text>
      <TextList items={indexLines(index)} />
      {index && !index.index.enabled ? (
        <Note tone="info" title="Indexing is off" actions={<Button size="sm" variant="outline" onClick={() => app.ui.setView('settings')}>Turn on in Settings</Button>}>It is a setting of Sessions, with the titles of what runs.</Note>
      ) : null}
      <Cluster gap="sm">
        <Button variant="outline" disabled={!buttons.start} onClick={() => { void sessions.startIndex(); }}>Index sessions</Button>
        <Button variant="outline" disabled={!buttons.stop} onClick={() => { void sessions.stopIndex(); }}>Stop</Button>
        <Button variant="destructive" disabled={!buttons.forget} onClick={() => { void sessions.forgetIndex(); }}>Forget all indexed text</Button>
      </Cluster>
      <Text variant="muted">Only changed sessions are read again. Forgetting removes the indexed text only; your sessions and threads are not touched.</Text>
    </Frame>
  );
}

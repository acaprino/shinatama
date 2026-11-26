// The facts of one saved session, from the list of the Sessions view: what it is and where it is, and the way to read its conversation
// or to hand it to another agent. Nothing here starts or resumes an agent.
import { DescriptionList, Note } from '@acaprino/sigrid/components';
import { Button, Cluster, Heading, Icon } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { agentName, archiveName, sizeText, when, type SessionRootSelector } from '../../session-library-model.ts';
import { Frame } from './frame.tsx';

export function SummaryPanel({ root, nativeId }: { root: SessionRootSelector; nativeId: string }) {
  const app = useApp();
  const sessions = useController(app.sessions);
  const found = sessions.row(root, nativeId);
  const title = found?.row.title || 'Untitled session';
  const available = found?.root.available ?? true;
  return (
    <Frame
      title="Session"
      footer={(
        <Cluster gap="sm">
          <Button disabled={!available} title="Read the conversation, search in it, export it or hop it to another agent" onClick={() => sessions.show(root, nativeId)}>Open conversation</Button>
          <Button variant="outline" disabled={!available} onClick={() => { void sessions.hop(root, nativeId); }}><Icon name="importExport" />Hop…</Button>
        </Cluster>
      )}
    >
      <Heading size="small" level={3}>{title}</Heading>
      {found ? (
        <DescriptionList
          aria-label="Facts of the session"
          items={[
            { name: 'Agent', value: agentName(found.root.harness) },
            { name: 'Folder', value: found.row.workingDirectory ?? 'Unknown', code: true },
            { name: 'Created', value: found.row.createdAt ? when(found.row.createdAt) : 'Unknown' },
            { name: 'Updated', value: when(found.row.updatedAt) },
            { name: 'Turns', value: found.row.turns === null ? 'Unknown' : found.row.turns.toLocaleString() },
            { name: 'Size', value: sizeText(found.row.bytes) },
            { name: 'Parent', value: found.row.parentNativeId ?? '—', code: found.row.parentNativeId !== null },
            { name: 'Archive', value: archiveName(found.root) },
            { name: 'Id', value: found.row.nativeId, code: true },
          ]}
        />
      ) : <Note tone="info" title="This session is not among those listed">Its archive may not be read yet, or the session came from a search of text. Its conversation can still be opened.</Note>}
      {found?.row.metadataTruncated ? <Note tone="info" title="Some metadata is unavailable">Its fixed field limits were exceeded.</Note> : null}
    </Frame>
  );
}

// A file open in a pane (`app.fileTabs`, "Open in a pane" of the Files panel): its name and language, Save, and its text in Sigrid's `CodeView`,
// which is edited in place and written only by Save, checked against the version that was read. After a restart the tab reads its file again.
import { useEffect } from 'react';
import { CodeView, codeLanguageOf } from '@acaprino/sigrid/code';
import { Empty, Note } from '@acaprino/sigrid/components';
import { Button, Cell, Cluster, Column, Icon, Progress, Text } from '@acaprino/sigrid/primitives';
import { useApp, useAfter, useController } from '../../hooks.ts';
import { dirty } from '../../files-model.ts';

export function FileTab({ viewId }: { viewId: string }) {
  const app = useApp();
  const tabs = useController(app.fileTabs);
  useEffect(() => { void app.fileTabs.ensure(viewId); }, [app, viewId]);
  const tab = tabs.tabs.get(viewId);
  const waiting = useAfter(tab?.state === 'loading');
  if (!tab) return <Empty fill icon={<Icon name="file" />} title="This file is not known to this window" description="Open it again from the project files." />;
  if (tab.state === 'loading') return waiting ? <Empty fill title={tab.path} action={<Progress inline aria-label="Reading the file" value={null} />} /> : null;
  const { buffer } = tab;
  if (!buffer) {
    return (
      <Empty
        fill
        icon={<Icon name="file" />}
        title={tab.path}
        description={<Note tone="error" title={tab.line} actions={<Button variant="outline" size="sm" onClick={() => { void app.fileTabs.reload(viewId); }}>Try again</Button>} />}
      />
    );
  }
  const changed = dirty(buffer);
  return (
    <Column
      fill
      onKeyDownCapture={event => {
        if ((event.ctrlKey || event.metaKey) && !event.shiftKey && event.key.toLowerCase() === 's') { event.preventDefault(); void app.fileTabs.save(viewId); }
      }}
    >
      <Cluster gap="sm" pad="sm" wrap={false}>
        <Cell grow><Text variant="strong" truncate title={buffer.path}>{buffer.path}</Text></Cell>
        <Text variant="muted">{codeLanguageOf(buffer.path)?.label ?? 'Plain text'}</Text>
        {changed ? <Text variant="muted">Unsaved changes</Text> : null}
        <Button variant="outline" size="sm" title="Read the file again" onClick={() => { void app.fileTabs.reload(viewId); }}>Reload</Button>
        <Button size="sm" disabled={!buffer.editable || !changed || tab.saving} loading={tab.saving} loadingText="Saving…" onClick={() => { void app.fileTabs.save(viewId); }}>Save</Button>
      </Cluster>
      <Cell grow>
        <CodeView value={buffer.text} fileName={buffer.path} readOnly={!buffer.editable} aria-label={`Source of ${buffer.path}`} onChange={text => app.fileTabs.edit(viewId, text)} />
      </Cell>
      {tab.line ? (tab.failed ? <Note tone="error" title={tab.line} /> : <Text role="status" variant="muted" truncate>{tab.line}</Text>) : null}
    </Column>
  );
}

// The archive browser: the metadata of one archive the person chose, a page at a time, as a list or grouped by the folder each session
// recorded, and the form that adds a read-only archive. Browsing never starts or resumes an agent.
import { useEffect, useState } from 'react';
import { Empty, FormField, ItemList, ItemRow, SearchField, DescriptionList } from '@acaprino/sigrid/components';
import { Button, Cluster, Column, Input, Progress, Section, Select, Text, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { filterPage, groupByFolder } from '../../controllers/session-archives.ts';
import { ARCHIVE_AGENTS, when } from '../../session-library-model.ts';
import { Frame } from './frame.tsx';

function AddArchive() {
  const app = useApp();
  const browser = useController(app.sessions.archiveBrowser);
  const [agent, setAgent] = useState('');
  const [path, setPath] = useState('');
  return (
    <Section title="Add a read-only archive" level={3}>
      <Text variant="muted">Choose an agent and its existing archive folder. Files stay in place and browsing does not change them.</Text>
      <form onSubmit={event => { event.preventDefault(); void browser.add(agent, path).then(added => { if (added) setPath(''); }); }} aria-label="Add a read-only archive">
        <Column gap="sm">
          <Select aria-label="Agent" value={agent} placeholder="Choose agent" onValueChange={setAgent} options={ARCHIVE_AGENTS.map(([value, label]) => ({ value, label }))} />
          <FormField label="Archive folder">
            <Input mono value={path} maxLength={4096} autoComplete="off" spellCheck={false} onChange={event => setPath(event.target.value)} />
          </FormField>
          <Cluster><Button type="submit" variant="outline" disabled={browser.busy !== null || !agent || !path.trim()}>Add archive</Button></Cluster>
        </Column>
      </form>
    </Section>
  );
}

export function ArchivesPanel() {
  const app = useApp();
  const sessions = app.sessions;
  const browser = useController(sessions.archiveBrowser);
  useEffect(() => () => browser.forget(), [browser]);
  const { page, root, busy } = browser;
  const rows = filterPage(page, browser.filter);
  const options = browser.roots.map((one, index) => ({ value: String(index), label: `${one.label || one.harness} · ${one.kind} · revision ${one.configurationRevision}${one.available ? '' : ' · Unavailable'}` }));
  const row = (item: (typeof rows)[number]) => (
    <ItemRow
      key={item.nativeId}
      title={item.title || 'Untitled session'}
      description={`${item.workingDirectory || 'Unknown folder'}`}
      meta={when(item.updatedAt)}
      current={browser.selected === item}
      onClick={() => browser.select(item)}
    />
  );
  const selected = browser.selected;
  return (
    <Frame
      title="Archive browser"
      footer={selected && root ? (
        <Cluster gap="sm" justify="end">
          <Button disabled={busy !== null || !root.available} onClick={() => sessions.show(root, selected.nativeId)}>Open detail</Button>
        </Cluster>
      ) : undefined}
    >
      <Text variant="muted">Browse the metadata of one archive you chose. Browsing never starts or resumes an agent.</Text>
      <Cluster gap="sm">
        <Button variant={browser.roots.length ? 'outline' : 'default'} disabled={busy !== null} onClick={() => { void browser.loadRoots(); }}>{browser.roots.length ? 'Reload archives' : 'Load archives'}</Button>
        <Button variant="outline" disabled={busy !== null || !root?.available} onClick={() => { void browser.reload(); }}>Reload sessions</Button>
      </Cluster>
      <Select aria-label="Archive" value={browser.chosen} placeholder="Choose an archive explicitly" disabled={busy === 'roots' || busy === 'archive' || !browser.roots.length} onValueChange={browser.choose.bind(browser)} options={options} />
      {root ? <Text variant="muted" mono truncate title={root.storeRootId}>{`Archive: ${root.storeRootId} · revision ${root.configurationRevision} · ${root.kind} · ${root.access === 'read_only' ? 'read only' : 'metadata reads only'}`}</Text> : null}
      <Text role="status" tone={browser.failed ? 'danger' : undefined} variant={browser.failed ? undefined : 'muted'}>{busy ? busy === 'roots' ? 'Loading archives…' : busy === 'archive' ? 'Registering read-only archive…' : 'Reading bounded session metadata…' : browser.message}</Text>
      {busy ? <Progress aria-label="Reading the archive" value={null} /> : null}
      {page ? (
        <Column gap="sm">
          <Cluster gap="sm">
            <SearchField aria-label="Filter this page" placeholder="Filter this page" value={browser.filter} onValueChange={value => browser.setFilter(value)} />
            <ToggleGroup aria-label="Current page view" value={browser.view} onValueChange={value => browser.setView(value as 'list' | 'projects')}>
              <ToggleGroupItem value="list">List</ToggleGroupItem>
              <ToggleGroupItem value="projects">Projects / sessions</ToggleGroupItem>
            </ToggleGroup>
          </Cluster>
          {browser.view === 'projects' ? <Text variant="muted">Current page only, grouped by the exact recorded working folder. Unknown project means the reader supplied no working folder. These groups are not linked to saved projects.</Text> : null}
          {rows.length ? (
            browser.view === 'list'
              ? <ItemList aria-label="Session metadata">{rows.map(row)}</ItemList>
              : groupByFolder(rows).map(group => (
                <Section key={group.folder ?? '\0'} title={group.folder ?? 'Unknown project'} level={4} end={<Text variant="muted">{group.rows.length} on this page</Text>}>
                  <ItemList aria-label={`Sessions in ${group.folder ?? 'unknown project'}`}>{group.rows.map(row)}</ItemList>
                </Section>
              ))
          ) : <Empty title={page.rows.length ? 'No session on this page matches the filter' : 'No listed sessions'} />}
          <Cluster gap="sm" justify="between">
            <Cluster gap="sm">
              <Button size="sm" variant="outline" disabled={busy !== null || !browser.history.length} onClick={() => { void browser.previous(); }}>Previous</Button>
              <Button size="sm" variant="outline" disabled={busy !== null || !page.next} onClick={() => { void browser.next(); }}>Next</Button>
            </Cluster>
            <Text variant="muted">{browser.position} · {rows.length} shown</Text>
          </Cluster>
        </Column>
      ) : null}
      {selected ? (
        <DescriptionList
          aria-label="Selected session metadata"
          items={[
            { name: 'Native ID', value: selected.nativeId, code: true }, { name: 'Title', value: selected.title ?? 'Unknown' },
            { name: 'Created', value: selected.createdAt ?? 'Unknown' }, { name: 'Updated', value: selected.updatedAt ?? 'Unknown' },
            { name: 'Working folder', value: selected.workingDirectory ?? 'Unknown', code: true }, { name: 'Parent ID', value: selected.parentNativeId ?? 'Unknown', code: true },
            { name: 'Turns', value: selected.turns === null ? 'Unknown' : String(selected.turns) }, { name: 'Bytes', value: selected.bytes === null ? 'Unknown' : String(selected.bytes) },
            { name: 'Archived', value: selected.archived === null ? 'Unknown' : selected.archived ? 'yes' : 'no' },
            selected.metadataTruncated ? { name: 'Some metadata', value: 'Unavailable because its fixed field limits were exceeded' } : null,
          ]}
        />
      ) : null}
      <AddArchive />
    </Frame>
  );
}

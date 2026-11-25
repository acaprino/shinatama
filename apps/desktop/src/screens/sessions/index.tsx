// SESSIONS. The Sessions view: every session Shinatama can read, the ones that run now and the ones saved in the agents' archives, as one
// table with a search above it. Typing filters what is listed; Enter searches the text of the conversations the index holds, and the
// matches replace the table until the search is cleared. A row opens its facts in the inspector (`app.sessions.select`), from which its
// conversation is read or handed to another agent; a running one takes the person to its pane.
import { useEffect } from 'react';
import { usePhone } from '@acaprino/sigrid';
import { DataTable, type DataColumn } from '@acaprino/sigrid/table';
import { ContextMenu, Empty, ItemList, ItemRow, Note, SearchField } from '@acaprino/sigrid/components';
import { Panel } from '@acaprino/sigrid/layout';
import { Badge, Button, Cluster, Count, Progress, Text } from '@acaprino/sigrid/primitives';
import type { ContextMenuEntry } from '@acaprino/sigrid/components';
import { useApp, useAfter, useController } from '../../hooks.ts';
import { doingText, STATE_WORDS, stateTone } from '../../instances-model.ts';
import { agentName, snippetParts, when, type SearchHit } from '../../session-library-model.ts';
import { folderName, tableRows, type TableRow } from '../../sessions-model.ts';

function Snippet({ text }: { text: string }) {
  return <Text variant="muted">{snippetParts(text).map((part, at) => part.match ? <mark key={at}>{part.text}</mark> : part.text)}</Text>;
}

function Hits({ hits }: { hits: SearchHit[] }) {
  const app = useApp();
  return (
    <ItemList aria-label="Matches in conversations">
      {hits.map(hit => (
        <ItemRow
          key={`${hit.storeRootId}\0${hit.configurationRevision}\0${hit.nativeId}`}
          title={hit.title || hit.nativeId}
          description={<><Text variant="muted">{agentName(hit.harness)} · {folderName(hit.workingDirectory)}</Text><br /><Snippet text={hit.snippet} /></>}
          meta={when(hit.updatedAt)}
          end={hit.sourceDeleted ? <Badge variant="secondary" title="Source deleted: still searchable, no longer resumable.">deleted</Badge> : null}
          disabled={hit.configurationRevision === 0}
          onClick={() => app.sessions.show({ storeRootId: hit.storeRootId, configurationRevision: hit.configurationRevision }, hit.nativeId)}
        />
      ))}
    </ItemList>
  );
}

export function Sessions() {
  const app = useApp();
  const sessions = useController(app.sessions);
  useController(app.projects);
  useController(app.ui);
  useEffect(() => { void app.sessions.ensureSaved(); return app.sessions.watchRunning(); }, [app]);
  const phone = usePhone();
  const { hits, searching, loadState, query } = sessions;
  const saved = sessions.saved;
  const running = sessions.shown;
  const rows = tableRows(running, saved);
  const waiting = useAfter(loadState === 'loading' && !saved.length);
  const filtered = query.trim() !== '';
  const total = saved.length + running.length;
  const current = app.ui.panel && (app.ui.panel.kind === 'summary' || app.ui.panel.kind === 'session')
    ? rows.find(row => row.kind === 'saved' && sessions.isShown(row.saved.root, row.saved.row.nativeId))?.id ?? null
    : null;

  const menuFor = (target: Element): readonly ContextMenuEntry[] => {
    const id = target.closest<HTMLElement>('[data-row-id]')?.dataset['rowId'];
    const row = rows.find(one => one.id === id);
    if (!row || row.kind === 'group') return [];
    if (row.kind === 'running') return [{ label: 'Focus its pane', icon: 'terminal', onSelect: () => { if (!sessions.focusRunning(row.row.id)) app.ui.info('This instance has no pane in this window.'); } }];
    const { root, row: item } = row.saved;
    return [
      { label: 'Show its facts', icon: 'info', onSelect: () => sessions.select(root, item.nativeId) },
      { label: 'Open conversation', icon: 'history', disabled: !root.available, onSelect: () => sessions.show(root, item.nativeId) },
      { label: 'Hop to another agent…', icon: 'importExport', disabled: !root.available, onSelect: () => { void sessions.hop(root, item.nativeId); } },
      'separator',
      { label: 'Copy the session id', icon: 'copy', onSelect: () => { void navigator.clipboard?.writeText(item.nativeId).then(() => app.ui.info('Session id copied.'), () => app.ui.error('The clipboard is unavailable.')); } },
    ];
  };

  const everything: DataColumn<TableRow>[] = [
    {
      id: 'title', header: 'Session', grow: true,
      cell: row => row.kind === 'saved' ? <Text truncate title={row.saved.row.title ?? undefined}>{row.saved.row.title || 'Untitled session'}</Text>
        : row.kind === 'running' ? <Text truncate>{doingText(sessions.titles?.rows.find(entry => entry.instanceId === row.row.id)) ?? `${agentName(row.row.agent)} ${row.row.id.slice(-4)}`}</Text> : null,
    },
    { id: 'agent', header: 'Agent', cell: row => row.kind === 'saved' ? agentName(row.saved.root.harness) : row.kind === 'running' ? agentName(row.row.agent) : null },
    {
      id: 'where', header: 'State or folder',
      cell: row => row.kind === 'running' ? <Badge tone={stateTone(row.row.state)} variant="outline">{STATE_WORDS[row.row.state] ?? row.row.state}</Badge>
        : row.kind === 'saved' ? <Text truncate title={row.saved.row.workingDirectory ?? undefined}>{folderName(row.saved.row.workingDirectory)}</Text> : null,
    },
    { id: 'updated', header: 'Updated', cell: row => row.kind === 'saved' ? <Text title={row.saved.row.updatedAt ?? undefined}>{when(row.saved.row.updatedAt)}</Text> : null },
    { id: 'turns', header: 'Turns', align: 'end', cell: row => row.kind === 'saved' && row.saved.row.turns !== null ? row.saved.row.turns.toLocaleString() : null },
  ];
  // A phone has the room for the title, the state and the time.
  const columns = phone ? everything.filter(column => !['agent', 'turns'].includes(column.id)) : everything;

  let body;
  if (hits) {
    body = hits.length
      ? <Hits hits={hits} />
      : <Empty title={`No indexed conversation contains “${sessions.searched}”`} description="If the index is empty, open Search index in the sidebar and index the sessions." />;
  } else if (loadState === 'failed' && !running.length) {
    body = <Empty title="The sessions could not be read" description={<Note tone="error" title={sessions.problem ?? ''} actions={<Button variant="outline" size="sm" onClick={() => { void sessions.loadSaved(); }}>Retry</Button>} />} />;
  } else if (loadState === 'loading' && !rows.length) {
    body = waiting ? <Empty title="Reading the archives" action={<Progress inline aria-label="Reading the archives" value={null} />} /> : null;
  } else if (!rows.length) {
    body = <Empty title={filtered ? 'No session matches the filter' : 'No session yet'} description={filtered ? 'Press Enter to search inside conversations.' : 'Sessions that run, and the ones saved by the agents, are listed here.'} />;
  } else {
    body = (
      <ContextMenu label="Actions for the session" items={menuFor}>
        <DataTable<TableRow>
          aria-label="Sessions"
          fill
          rows={rows}
          rowId={row => row.id}
          current={current}
          isGroupRow={row => row.kind === 'group'}
          groupCell={row => row.kind === 'group' ? <Cluster gap="sm"><Text variant="strong">{row.label}</Text><Count value={row.count} label="sessions" /></Cluster> : null}
          onRowPress={row => {
            if (row.kind === 'saved') sessions.select(row.saved.root, row.saved.row.nativeId);
            else if (row.kind === 'running' && !sessions.focusRunning(row.row.id)) app.ui.info('This instance has no pane in this window.');
          }}
          columns={columns}
        />
      </ContextMenu>
    );
  }

  return (
    <Panel
      title="Sessions"
      subtitle={hits ? `Matches in conversations · ${hits.length}${hits.length >= 50 ? ' (first 50)' : ''}` : `${saved.length} saved · ${running.length} running`}
      tools={(
        <SearchField
          aria-label="Filter sessions, or press Enter to search the text of conversations"
          placeholder="Filter · Enter searches text"
          value={query}
          onValueChange={value => sessions.setQuery(value)}
          onKeyDown={event => {
            if (event.key === 'Enter') { event.preventDefault(); void sessions.runSearch(); }
            else if (event.key === 'Escape' && query) { event.preventDefault(); event.stopPropagation(); sessions.clearSearch(); }
          }}
        />
      )}
      actions={(
        <Cluster gap="sm">
          {hits ? <Button size="sm" variant="outline" onClick={() => sessions.clearSearch()}>Clear search</Button> : null}
          <Button size="sm" variant="outline" title="Read the archives again" disabled={loadState === 'loading'} onClick={() => { void sessions.loadSaved(); void sessions.refreshRunning(true); }}>Reload</Button>
        </Cluster>
      )}
      footer={!hits && sessions.more ? <Button variant="outline" size="sm" onClick={() => { void sessions.loadOlder(); }}>Load older sessions</Button> : undefined}
    >
      {searching ? <Progress aria-label="Searching conversations" value={null} /> : null}
      {sessions.searchProblem ? <Note tone="error" title={sessions.searchProblem} /> : null}
      {sessions.failures.length ? <Note tone="warning" title={`${sessions.failures.length} archive${sessions.failures.length === 1 ? '' : 's'} could not be read`} items={sessions.failures.map(failure => `${failure.root.label ?? agentName(failure.root.harness)}: ${failure.failure}`)} /> : null}
      {body}
      {!hits && !rows.length ? null : <Text srOnly role="status">{total} sessions</Text>}
    </Panel>
  );
}

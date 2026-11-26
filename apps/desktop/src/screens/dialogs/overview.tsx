// The projects at a glance: pins, the Git branch and what is changed, the last thread (resumed from here), new sessions (also in a
// worktree, after the exact path and branch were confirmed) and what agents left on disk in each project, as Xiox sees it. Read-only for
// leftovers: nothing is removed from here.
import { useEffect, useState } from 'react';
import { Dialog, Empty, Note } from '@acaprino/sigrid/components';
import { Badge, Button, Card, Cluster, Column, Icon, Input, Progress, Spacer, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { itemText, leftoversFor, summaryText } from '../../leftovers-model.ts';
import { gitText, type ProjectRow } from '../../projects-overview-model.ts';

function Leftovers({ row }: { row: ProjectRow }) {
  const app = useApp();
  const leftovers = useController(app.leftovers);
  const { status } = leftovers;
  if (!status.available || !row.available) return null;
  const picture = leftoversFor(row.path, status.projects);
  const label = !picture ? 'Watch now' : picture.phase === 'ended' ? 'Restart Xiox' : 'Check now';
  return (
    <Cluster gap="sm">
      <Text variant="muted">{picture ? summaryText(picture) : 'Leftovers: not watched yet. Xiox starts with the first agent session here.'}</Text>
      {picture?.attention.length ? <Badge tone="warning">Needs attention</Badge> : null}
      <Spacer />
      <Button variant="outline" size="sm" disabled={picture?.phase === 'starting'} title="Xiox lists the worktrees, copies, bench and backup folders and builds that appeared. It never removes anything." onClick={() => { void leftovers.check(row.path, row.name || row.path); }}>{label}</Button>
      {picture?.newItems.length ? <Button variant="outline" size="sm" title={itemText(picture.newItems[0])} onClick={() => app.ui.openPanel({ kind: 'leftovers' })}>Show leftovers</Button> : null}
    </Cluster>
  );
}

function Row({ row }: { row: ProjectRow }) {
  const app = useApp();
  const overview = app.overview;
  const [tree, setTree] = useState(false);
  const [name, setName] = useState('task-1');
  const last = row.lastThread;
  return (
    <Card variant="quiet" as="article" aria-label={row.name || row.path}>
      <Cluster gap="sm" justify="between" wrap={false}>
        <Column gap="xs">
          <Text variant="strong">{row.name || row.path}</Text>
          <Text variant="muted" mono truncate>{row.available ? row.path : `${row.path} · unavailable`}</Text>
          <Text variant="muted">{gitText(row.git)}</Text>
          <Text variant="muted">{last ? `Last: ${last.title ?? 'Untitled thread'} · ${last.harness ?? 'no agent yet'}` : 'No thread yet'}</Text>
        </Column>
        <Button variant="ghost" size="icon-xs" aria-pressed={row.pinned} aria-label={row.pinned ? `Unpin ${row.name}` : `Pin ${row.name}`} onClick={() => { void overview.pin(row); }}><Icon name="pin" /></Button>
      </Cluster>
      <Cluster gap="sm">
        <Button variant="outline" size="sm" disabled={!row.available || !last?.harness} onClick={() => { if (last) void overview.resume(row, last); }}>Resume last</Button>
        <Button variant="outline" size="sm" disabled={!row.available} onClick={() => overview.newSession(row)}>New session</Button>
        <Button variant="outline" size="sm" aria-expanded={tree} disabled={!row.available || row.git.state !== 'known'} onClick={() => setTree(!tree)}>New in worktree…</Button>
      </Cluster>
      {tree ? (
        <Cluster gap="sm" wrap={false}>
          <Input aria-label={`Worktree name for ${row.name}`} mono maxLength={40} value={name} onChange={event => setName(event.target.value)} onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); void overview.worktree(row, name.trim()); } }} />
          <Button variant="outline" onClick={() => { void overview.worktree(row, name.trim()); }}>Create and open</Button>
        </Cluster>
      ) : null}
      <Leftovers row={row} />
    </Card>
  );
}

export function ProjectsOverview() {
  const app = useApp();
  const overview = useController(app.overview);
  useEffect(() => { void app.overview.load(); return () => app.overview.close(); }, [app]);
  const { rows } = overview;
  return (
    <Dialog
      open
      onOpenChange={next => { if (!next) app.ui.closeDialog(); }}
      title="Projects"
      description={rows?.length ? `${rows.length} project${rows.length === 1 ? '' : 's'}. Pinned first.` : undefined}
      width="xl"
      footer={(
        <>
          <Button variant="outline" onClick={() => app.ui.open({ kind: 'project-picker' })}>Choose projects…</Button>
          <Spacer />
          <Button variant="outline" onClick={() => app.ui.closeDialog()}>Close</Button>
        </>
      )}
    >
      <Column gap="md" aria-busy={overview.loading}>
        {overview.loading && !rows ? <Progress aria-label="Reading projects and Git status" value={null} /> : null}
        {overview.failure ? <Note tone="error" title={overview.failure} actions={<Button variant="outline" size="sm" onClick={() => { void app.overview.load(); }}>Retry</Button>} /> : null}
        {overview.note ? <Note tone={overview.note.error ? 'error' : 'info'} title={overview.note.text} /> : null}
        {rows && rows.length === 0 ? <Empty title="No projects yet" description="Choose a folder to list its Git status, its last thread and what agents left there." /> : null}
        {rows?.map(row => <Row key={row.id} row={row} />)}
      </Column>
    </Dialog>
  );
}

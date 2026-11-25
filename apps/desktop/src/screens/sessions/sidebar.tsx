// The sidebar of the Sessions view: what the list can be narrowed to (an agent, one archive, this project or all of them), the way to the
// archive browser and the running sessions, and the index the text search reads.
import { useEffect } from 'react';
import type { SidebarState } from '@acaprino/imago';
import { Sidebar, SidebarGroup, NavItem } from '@acaprino/sigrid/layout';
import { Button, Cluster, Count, Icon, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { indexSummary, isRunning } from '../../session-index-model.ts';
import { agentName, archiveName, sameRoot } from '../../session-library-model.ts';

export function SessionsSidebar({ state }: { state: SidebarState }) {
  const app = useApp();
  const sessions = useController(app.sessions);
  useController(app.projects);
  useEffect(() => { void app.sessions.refreshIndex(); }, [app]);
  const { agent, archive, scope, archives, index, indexBusy } = sessions;
  const total = sessions.agents.reduce((sum, one) => sum + one.count, 0);
  const project = app.projects.path;
  return (
    <Sidebar aria-label="Stores" {...state}>
      <SidebarGroup label="Agents">
        <NavItem current={agent === null} icon={<Icon name="all" />} end={<Count value={total} label="sessions" />} onClick={() => sessions.setAgent(null)}>All agents</NavItem>
        {sessions.agents.map(one => (
          <NavItem key={one.agent} current={agent === one.agent} icon={<Icon name="terminal" />} end={<Count value={one.count} label="sessions" />} onClick={() => sessions.setAgent(agent === one.agent ? null : one.agent)}>{agentName(one.agent)}</NavItem>
        ))}
      </SidebarGroup>
      <SidebarGroup label="Archives">
        <NavItem current={archive === null} icon={<Icon name="archive" />} onClick={() => sessions.setArchive(null)}>All archives</NavItem>
        {archives.map(one => (
          <NavItem
            key={sessions.archiveId(one.root)}
            current={archive !== null && sameRoot(archive, one.root)}
            icon={<Icon name="archive" />}
            end={<Count value={one.rows.length} label="sessions" />}
            onClick={() => sessions.setArchive(archive !== null && sameRoot(archive, one.root) ? null : { storeRootId: one.root.storeRootId, configurationRevision: one.root.configurationRevision })}
          >{archiveName(one.root)}</NavItem>
        ))}
        <NavItem icon={<Icon name="folder" />} onClick={() => app.ui.openPanel({ kind: 'archives' })}>Archive browser…</NavItem>
      </SidebarGroup>
      <SidebarGroup label="Projects">
        <NavItem current={sessions.scope === 'all'} icon={<Icon name="all" />} onClick={() => sessions.setScope('all')}>All projects</NavItem>
        {project ? <NavItem current={sessions.scope === 'project'} icon={<Icon name="folder" />} description={project} onClick={() => sessions.setScope('project')}>This project</NavItem> : null}
      </SidebarGroup>
      <SidebarGroup label="Running">
        <NavItem icon={<Icon name="terminal" />} end={<Count value={sessions.running.length} label="running" />} onClick={() => app.ui.openPanel({ kind: 'instances' })}>Running sessions…</NavItem>
      </SidebarGroup>
      <SidebarGroup label="Search index">
        <NavItem icon={<Icon name="search" />} description={indexSummary(index)} onClick={() => app.ui.openPanel({ kind: 'search-index' })}>Search index…</NavItem>
        <Cluster gap="sm" pad="sm">
          <Button size="sm" variant="outline" disabled={indexBusy || isRunning(index) || !index?.index.enabled} onClick={() => { void sessions.startIndex(); }}>Index sessions</Button>
          <Button size="sm" variant="destructive" disabled={indexBusy || isRunning(index) || !index || index.index.documents === 0} onClick={() => { void sessions.forgetIndex(); }}>Forget</Button>
        </Cluster>
        {isRunning(index) ? <Text variant="muted">Indexing…</Text> : null}
      </SidebarGroup>
    </Sidebar>
  );
}

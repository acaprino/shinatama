// The sidebar of the Workspace view (placeholder, Phase A): the projects and their threads, a colour dot for each thread that is on,
// the filter above them. Owned by the workspace agent.
import type { SidebarState } from '@acaprino/imago';
import { Count, Icon } from '@acaprino/sigrid/primitives';
import { NavItem, Sidebar, SidebarGroup, SidebarRow } from '@acaprino/sigrid/layout';
import { useApp, useController } from '../../hooks.ts';

export function WorkspaceSidebar({ state }: { state: SidebarState }) {
  const app = useApp();
  const threads = useController(app.threads);
  const rows = [...threads.lists.values()].flat();
  return (
    <Sidebar aria-label="Threads" header={<SidebarRow>Threads <Count value={rows.length} label="threads" /></SidebarRow>} {...state}>
      <SidebarGroup>
        {rows.map(row => (
          <NavItem
            key={row.id}
            icon={<Icon name="threads" />}
            current={threads.isOn(row.id)}
            onClick={() => { void (threads.isOn(row.id) ? threads.hideWorkset(row.id) : threads.showWorkset({ id: row.id, projectId: row.projectId })); }}
          >{row.title ?? row.lead?.title ?? 'Thread'}</NavItem>
        ))}
      </SidebarGroup>
    </Sidebar>
  );
}

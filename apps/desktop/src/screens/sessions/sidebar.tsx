// The sidebar of the Sessions view (placeholder, Phase A): the stores and the projects the library can be narrowed to.
import type { SidebarState } from '@acaprino/imago';
import { Sidebar, SidebarGroup, NavItem } from '@acaprino/sigrid/layout';
import { Icon } from '@acaprino/sigrid/primitives';

export function SessionsSidebar({ state }: { state: SidebarState }) {
  return (
    <Sidebar aria-label="Stores" {...state}>
      <SidebarGroup>
        <NavItem current icon={<Icon name="history" />}>All sessions</NavItem>
      </SidebarGroup>
    </Sidebar>
  );
}

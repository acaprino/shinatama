// The sidebar of the Settings view (placeholder, Phase A): the index of the sections of the page.
import type { SidebarState } from '@acaprino/imago';
import { Sidebar, SidebarGroup, NavItem } from '@acaprino/sigrid/layout';
import { Icon } from '@acaprino/sigrid/primitives';

export function SettingsSidebar({ state }: { state: SidebarState }) {
  return (
    <Sidebar aria-label="Sections" {...state}>
      <SidebarGroup>
        <NavItem current icon={<Icon name="settings" />}>All settings</NavItem>
      </SidebarGroup>
    </Sidebar>
  );
}

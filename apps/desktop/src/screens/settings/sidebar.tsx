// The sidebar of the Settings view: the index of the sections of the page. A press brings the section to the head of the page.
import type { SidebarState } from '@acaprino/imago';
import { Sidebar, SidebarGroup, NavItem } from '@acaprino/sigrid/layout';
import { Icon } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { SECTIONS } from '../../controllers/settings.ts';
import { SECTION_ICONS, scrollToSection } from './index.tsx';

export function SettingsSidebar({ state }: { state: SidebarState }) {
  const app = useApp();
  const settings = useController(app.settings);
  return (
    <Sidebar aria-label="Sections" {...state}>
      <SidebarGroup>
        {SECTIONS.map(([id, name]) => (
          <NavItem
            key={id}
            icon={<Icon name={SECTION_ICONS[id]} />}
            current={settings.section === id}
            onClick={() => { settings.markSection(id); scrollToSection(id); }}
          >{name}</NavItem>
        ))}
      </SidebarGroup>
    </Sidebar>
  );
}

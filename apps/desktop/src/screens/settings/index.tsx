// SETTINGS. One stacked page, with its section index in the sidebar: Appearance, Agents, Models & API keys, Memory, Alerts, GitHub,
// Environment variables, Sessions, Plugins and the Model proxy. Appearance, Alerts and Sessions apply as you change them; the rest waits for
// Save (the actions row of the window, `SettingsActions`). The theme and glass are the shell's, the zoom is the window's; the controllers are
// `app.settings` (the page, the terminal) and `app.services` (Muro, the proxy, memory, alerts).
import { useEffect } from 'react';
import { Panel } from '@acaprino/sigrid/layout';
import { Column, Icon, Section } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { SECTIONS, type SectionId } from '../../controllers/settings.ts';
import { Agents } from './agents.tsx';
import { Appearance } from './appearance.tsx';
import { GitHub, Memory, Sessions, Variables } from './launch.tsx';
import { Models } from './models.tsx';
import { Alerts, Plugins, Proxy } from './services.tsx';

export const sectionElementId = (id: SectionId): string => `settings-${id}`;

/** Brings a section to the head of the page: the body of the panel scrolls, never the window. */
export function scrollToSection(id: SectionId): void {
  const target = document.getElementById(sectionElementId(id));
  const body = target?.closest<HTMLElement>('[data-slot="panel-body"]');
  if (!target || !body) return;
  body.scrollTo({ top: target.getBoundingClientRect().top - body.getBoundingClientRect().top + body.scrollTop - 8, behavior: 'auto' });
  target.querySelector<HTMLElement>('h2')?.focus({ preventScroll: true });
}

export const SECTION_ICONS: Record<SectionId, 'appearance' | 'terminal' | 'key' | 'data' | 'notifications' | 'branch' | 'sliders' | 'history' | 'package' | 'gateway'> = {
  appearance: 'appearance', agents: 'terminal', models: 'key', memory: 'data', alerts: 'notifications', github: 'branch', variables: 'sliders', sessions: 'history', plugins: 'package', proxy: 'gateway',
};
const PARTS: Record<SectionId, () => React.JSX.Element> = {
  appearance: Appearance, agents: Agents, models: Models, memory: Memory, alerts: Alerts, github: GitHub, variables: Variables, sessions: Sessions, plugins: Plugins, proxy: Proxy,
};

export function Settings() {
  const app = useApp();
  const settings = useController(app.settings);
  useController(app.services);
  // The page is read when it opens, and what was typed in a secret field goes when it closes.
  useEffect(() => { settings.enter(); return () => settings.leave(); }, [settings]);
  // The index follows the scroll: the section nearest the head of the page is the one marked.
  useEffect(() => {
    const sections = SECTIONS.map(([id]) => document.getElementById(sectionElementId(id))).filter((node): node is HTMLElement => node !== null);
    const watch = new IntersectionObserver(entries => {
      const top = entries.filter(entry => entry.isIntersecting).sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
      if (top) settings.markSection(top.target.id.replace('settings-', '') as SectionId);
    }, { rootMargin: '0px 0px -70% 0px' });
    for (const node of sections) watch.observe(node);
    return () => watch.disconnect();
  }, [settings]);
  return (
    <Panel title="Settings" subtitle="Appearance, Alerts and Sessions apply as you change them. The rest applies when you press Save.">
      <Column gap="xl" pad="lg" width="page">
        {SECTIONS.map(([id, name]) => {
          const Part = PARTS[id];
          return (
            <Section key={id} id={sectionElementId(id)} title={name} icon={<Icon name={SECTION_ICONS[id]} />}>
              <Part />
            </Section>
          );
        })}
      </Column>
    </Panel>
  );
}

// The sidebar of the Workspace view (ADR-0055): the projects and their threads. A thread is a set of sessions with its own place in the
// window: a click turns it on (the window then shows its sessions, in its colour) or off; nothing here starts an agent. The state of the
// list (the projects, the threads that are on, the sessions that run) is `app.threads`; this file draws it: a group for each project,
// a row for each thread with its colour dot, its state line and its + and ⋯, a menu on the row for the rest, the filter above and the
// archived threads below.
import { useEffect } from 'react';
import type { SidebarState } from '@acaprino/imago';
import { ContextMenu, Empty, type ContextMenuEntry } from '@acaprino/sigrid/components';
import { Button, Checkbox, Count, Dot, Icon, Text } from '@acaprino/sigrid/primitives';
import { NavItem, Sidebar, SidebarGroup, SidebarRow } from '@acaprino/sigrid/layout';
import type { WorksetSummary } from '../../api/threads.ts';
import { useApp, useController } from '../../hooks.ts';
import { folderName } from '../../paths.ts';
import { agoText, threadStateLine } from '../../thread-sidebar.ts';

/** How often the names the agents gave the unnamed threads are asked for again while the sidebar is shown. */
const NAMES_MS = 15_000;

export function WorkspaceSidebar({ state }: { state: SidebarState }) {
  const app = useApp();
  const threads = useController(app.threads);
  useController(app.terminals);
  useController(app.chat);
  useController(app.projects);
  useController(app.opens);

  // The agents name the threads the person has not named: asked for now and then, never while a session starts.
  useEffect(() => {
    void threads.requestNames();
    const timer = setInterval(() => { void threads.requestNames(); }, NAMES_MS);
    return () => clearInterval(timer);
  }, [threads, threads.lists]);

  const projects = threads.activeProjectIds();
  const total = projects.reduce((sum, project) => sum + (threads.lists.get(project)?.length ?? 0), 0);
  const active = threads.activeWorkset()?.id ?? null;
  const busy = threads.busy;

  const nameOf = (project: string) => { const path = app.projects.pathOf(project); return path ? folderName(path) : project; };

  const menu = (row: WorksetSummary): ContextMenuEntry[] => {
    const on = threads.isOn(row.id);
    const talks = app.chat.talks(row.id) && !row.archived;
    const entries: ContextMenuEntry[] = [
      { label: 'Add a session', icon: 'plus', disabled: busy || row.archived, onSelect: () => { void threads.addSession(row); } },
      { label: 'Thread settings', icon: 'sliders', shortcut: 'Enter', onSelect: () => app.ui.openPanel({ kind: 'thread', threadId: row.id }) },
      ...(talks ? [
        { label: 'Open the conversation', icon: 'mail' as const, onSelect: () => app.chat.open({ id: row.id, projectId: row.projectId }) },
        { label: 'Open the team map', icon: 'graph' as const, onSelect: () => app.team.open({ id: row.id }) },
      ] : []),
      'separator',
      { label: 'New thread', icon: 'plus', disabled: busy || threads.showArchived, onSelect: () => { void threads.operate('created', () => threads.create(row.projectId)); } },
      { label: on ? 'Turn off thread' : 'Turn on thread', icon: on ? 'stop' : 'play', disabled: busy || row.archived, onSelect: () => { void threads.toggleOn(row); } },
      { label: row.archived ? 'Unarchive' : 'Archive', icon: 'archive', disabled: busy, onSelect: () => { void threads.setArchived(row, !row.archived); } },
      { label: 'Delete thread…', icon: 'delete', destructive: true, disabled: busy, onSelect: () => { void threads.deleteThread(row); } },
      'separator',
      { label: 'Remove from the sidebar', icon: 'close', disabled: busy, onSelect: () => { void threads.removeProject(row.projectId); } },
    ];
    return entries;
  };

  return (
    <Sidebar
      aria-label="Threads"
      search
      header={(
        <SidebarRow
          end={(
            <Button variant="ghost" size="icon-sm" aria-label="Choose projects" onClick={() => app.ui.open({ kind: 'project-picker', filter: true })}>
              <Icon name="folder" />
            </Button>
          )}
        >
          Threads <Count value={total} label="threads" />
        </SidebarRow>
      )}
      footer={(
        <SidebarRow leading={<Checkbox aria-label="Show archived threads" checked={threads.showArchived} disabled={busy || !projects.length} onCheckedChange={on => { void threads.setShowArchived(on === true); }} />}>
          Show archived threads
        </SidebarRow>
      )}
      {...state}
    >
      {projects.length === 0 ? (
        <SidebarGroup>
          <Empty
            title="No project in the sidebar"
            description="Choose the projects whose threads it lists."
            action={<Button variant="outline" onClick={() => app.ui.open({ kind: 'project-picker', filter: true })}>Choose projects</Button>}
          />
        </SidebarGroup>
      ) : projects.map(project => {
        const rows = threads.lists.get(project) ?? [];
        return (
          <SidebarGroup key={project} label={nameOf(project)} icon={<Icon name="folder" />} collapsible>
            {rows.map(row => {
              const name = threads.title(row.id) ?? 'New thread';
              const on = threads.isOn(row.id);
              const tone = threads.toneOf(row.id);
              const unread = app.chat.talks(row.id) && app.chat.unread(row.id);
              const running = threads.runningIn(row.id);
              const ago = agoText(row.usedAt);
              return (
                <ContextMenu key={row.id} items={menu(row)} label={`Actions for ${name}`}>
                  <NavItem
                    current={row.id === active}
                    aria-pressed={on}
                    icon={tone ? <Dot color={tone} /> : <Dot />}
                    description={`${threadStateLine(row, { on, running })}${ago ? ` · ${ago}` : ''}${unread ? ' · new messages' : ''}`}
                    end={unread ? <Dot tone="accent" /> : undefined}
                    disabled={busy}
                    title={`${name} · ${nameOf(project)}\n${on ? 'Click to turn it off: its running sessions stop, after asking, and stay in the thread, not running.' : 'Click to show its tabs.'}`}
                    onClick={() => { void threads.toggleOn(row); }}
                    actions={(
                      <>
                        <Button variant="ghost" size="icon-xs" aria-label={`Add a session to ${name}`} disabled={busy || row.archived} onClick={() => { void threads.addSession(row); }}>
                          <Icon name="plus" />
                        </Button>
                        <Button variant="ghost" size="icon-xs" aria-label={`Settings of ${name}`} onClick={() => app.ui.openPanel({ kind: 'thread', threadId: row.id })}>
                          <Icon name="more" />
                        </Button>
                      </>
                    )}
                  >
                    {name}
                  </NavItem>
                </ContextMenu>
              );
            })}
            {rows.length === 0 ? <Text variant="muted">{threads.showArchived ? 'No archived threads.' : 'No threads yet.'}</Text> : null}
            {!threads.showArchived ? (
              <NavItem icon={<Icon name="plus" />} disabled={busy} onClick={() => { void threads.operate('created', () => threads.create(project)); }}>
                {`New thread in ${nameOf(project)}`}
              </NavItem>
            ) : null}
          </SidebarGroup>
        );
      })}
    </Sidebar>
  );
}

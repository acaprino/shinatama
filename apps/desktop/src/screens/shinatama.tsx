// The window of Shinatama: Imago's `AppWindow` with its three views (Workspace, Sessions, Settings) and in its regions what each view
// says. The controllers (src/controllers) hold what the page remembers and ask Rust; this file puts them in the shell's regions and
// tells the window what the shell cannot know: the view in its title, the notices, the keys. Every area has a folder under screens/ that
// draws what is its own (docs/desktop-ui.md says who owns which).
import { useEffect, useMemo } from 'react';
import { AppWindow, focusContent, ScreenBoundary, useNotify, useWindow, type AppView } from '@acaprino/imago';
import { Column, Text } from '@acaprino/sigrid/primitives';
import { Shinatama } from '../app.ts';
import { AppContext, useApp, useController } from '../hooks.ts';
import { FOOTER } from '../keys.ts';
import type { ViewId } from '../controllers/ui.ts';
import { useShinatamaCommands } from './commands.tsx';
import { Dialogs } from './dialogs/index.tsx';
import { Inspector } from './inspector/index.tsx';
import { Sessions } from './sessions/index.tsx';
import { SessionsSidebar } from './sessions/sidebar.tsx';
import { Settings } from './settings/index.tsx';
import { SettingsActions } from './settings/actions.tsx';
import { SettingsSidebar } from './settings/sidebar.tsx';
import { TeamActions, TeamInspector } from './team/inspector.tsx';
import { Status } from './status.tsx';
import { Workspace } from './workspace/index.tsx';
import { WorkspaceSidebar } from './workspace/sidebar.tsx';
import { WorkspaceTools } from './workspace/tools.tsx';

const VIEWS: AppView<ViewId>[] = [
  { id: 'workspace', label: 'Workspace', icon: 'terminal', command: 'Go to the Workspace' },
  { id: 'sessions', label: 'Sessions', icon: 'history', command: 'Go to Sessions' },
  { id: 'settings', label: 'Settings', icon: 'settings', command: 'Go to Settings' },
];

function Window() {
  const app = useApp();
  const ui = useController(app.ui);
  useController(app.opens);
  const team = useController(app.team);
  const notify = useNotify();
  const window_ = useWindow();
  // The notices and the focus are the shell's: the controllers ask for them through these.
  ui.notice = (text, say) => notify(text, {
    ...(say?.error ? { kind: 'error' as const } : {}),
    ...(say?.undo ? { undo: { onClick: () => { void say.undo!(); } } } : {}),
  });
  ui.focusContent = focusContent;
  useShinatamaCommands(app);
  useEffect(() => { void app.start(); return () => app.dispose(); }, [app]);
  // The title of the window says the view (Alt+Tab and screen readers tell Workspace from Sessions).
  const { view } = ui;
  useEffect(() => { window_.setTitle(`${VIEWS.find(one => one.id === view)!.label} - Shinatama`); }, [window_, view]);

  // The Workspace stays mounted behind the other views: its terminals keep what they printed (and their history), the window keeps reading
  // them. It is hidden, not thrown away, while Sessions or Settings is shown.
  const screen = (
    <>
      <Column fill hidden={view !== 'workspace'}><Workspace /></Column>
      {view === 'sessions' ? <Sessions /> : view === 'settings' ? <Settings /> : null}
    </>
  );
  return (
    <AppWindow
      views={VIEWS}
      view={view}
      onView={next => ui.setView(next)}
      tools={view === 'workspace' ? <WorkspaceTools /> : undefined}
      // Ctrl+F is the search of the view: in the Workspace the terminal in the pane the person works in (a terminal keeps its own Ctrl+F), in
      // Sessions the filter of the list; Settings has none.
      search={view === 'workspace' ? () => app.terminals.openFind() : view === 'sessions' ? () => app.sessions.focusFilter() : null}
      status={<Status />}
      sidebar={state => view === 'workspace' ? <WorkspaceSidebar state={state} /> : view === 'sessions' ? <SessionsSidebar state={state} /> : <SettingsSidebar state={state} />}
      inspector={view === 'workspace' && team.inspecting ? <ScreenBoundary of={team.shownThread}><TeamInspector /></ScreenBoundary>
        : view !== 'settings' && ui.panel ? <ScreenBoundary of={ui.panel}><Inspector /></ScreenBoundary> : undefined}
      actions={view === 'settings' ? <SettingsActions /> : view === 'workspace' && team.acting ? <TeamActions /> : undefined}
      footer={app.opens.progress ? <Text truncate>{app.opens.progress}</Text> : undefined}
      hints={view === 'workspace' ? FOOTER : []}
    >
      {screen}
      <Dialogs />
    </AppWindow>
  );
}

export function Root() {
  const app = useMemo(() => new Shinatama(), []);
  return (
    <AppContext value={app}>
      <Window />
    </AppContext>
  );
}

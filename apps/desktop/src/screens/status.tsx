// The status strip: the runtime (a badge and a sentence), what runs, and the resources of the machine. One live region: the shell reads it
// when the phrase changes. In the Workspace it also counts the terminals and the panes; in Sessions it says whether the archives were read,
// how many sessions there are and whether the search index is on.
import { useMedia } from '@acaprino/sigrid';
import { Badge, Spacer, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../hooks.ts';
import { describeResources } from '../terminal-resources.ts';
import { useWindowTelemetry } from './workspace/telemetry.ts';

/** The strip of the Sessions view: the archives (read, being read, or not readable), the count, and the index the text search reads. */
function SessionsStatus() {
  const app = useApp();
  const sessions = useController(app.sessions);
  const { loadState, index } = sessions;
  const reading = loadState === 'loading' || loadState === 'idle';
  const failed = loadState === 'failed';
  return (
    <>
      <Badge tone={failed ? 'danger' : reading ? 'warning' : 'success'}>{failed ? 'Archives unreadable' : reading ? 'Reading the archives' : 'Archives read'}</Badge>
      <Text variant="muted">{`${sessions.saved.length} saved · ${sessions.shown.length} running`}</Text>
      <Spacer />
      <Badge {...(index?.index.enabled ? { tone: 'success' as const } : { variant: 'outline' as const })}>{index?.index.enabled ? 'Search index on' : 'Search index off'}</Badge>
    </>
  );
}

export function Status() {
  const app = useApp();
  const ui = useController(app.ui);
  const terminals = useController(app.terminals);
  useController(app.threads);
  useController(app.layout);
  useWindowTelemetry(app);
  // In a narrow window the strip keeps what matters: the runtime, and how many sessions run.
  const narrow = useMedia('(width < 620px)');
  const running = [...terminals.sessions.values()].filter(session => !session.ended && !session.disposed).length;
  const { runtime } = ui;
  const resources = describeResources(terminals.health);
  const panes = app.layout.groups.filter(group => group.activeTabId !== null).length;
  if (ui.view === 'sessions') return <SessionsStatus />;
  return (
    <>
      <Badge tone={runtime.failed ? 'danger' : runtime.readOnly ? 'warning' : 'success'}>{runtime.failed ? 'Error' : runtime.readOnly ? 'Read-only' : 'Ready'}</Badge>
      <Text truncate title={runtime.title || undefined}>{runtime.text}</Text>
      {ui.view === 'workspace' && !narrow ? <Text variant="muted">{`${terminals.sessions.size} terminal${terminals.sessions.size === 1 ? '' : 's'} · ${panes} pane${panes === 1 ? '' : 's'}`}</Text> : null}
      <Spacer />
      {narrow ? null : <Badge tone={resources.tone} title={resources.detail}>{resources.label}</Badge>}
      <Text variant="muted">{narrow ? `${running} running` : `${running} running · ${app.threads.openWorksets.length} threads on`}</Text>
    </>
  );
}

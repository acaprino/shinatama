// The settings of one thread: its name, whether its sessions talk to each other (their room, and the team map that comes with it), the
// sessions it holds, archiving it and deleting it (after asking). A thread's tab and row are the workspace's; this is only what can be
// changed about it.
import { useEffect, useState } from 'react';
import { FormField, Note } from '@acaprino/sigrid/components';
import { Button, Cell, Cluster, Column, Input, Section, Switch, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { agoText } from '../../thread-sidebar.ts';
import { closedSessionAction, sessionStateText, type ThreadSession } from '../../session-tabs.ts';
import { Frame } from './frame.tsx';

function OpenSession({ session }: { session: ThreadSession }) {
  const app = useApp();
  const settings = app.threadSettings;
  const [renaming, setRenaming] = useState(false);
  const [draft, setDraft] = useState('');
  const save = () => { void settings.renameSession(session.id, draft).then(done => { if (done) setRenaming(false); }); };
  if (renaming) {
    return (
      <form onSubmit={event => { event.preventDefault(); save(); }} aria-label={`Rename ${session.name}`}>
        <Cluster gap="sm" wrap={false}>
          <Input autoFocus value={draft} placeholder={session.name} aria-label={`New name of ${session.name}`} maxLength={256}
            onChange={event => setDraft(event.target.value)} onKeyDown={event => { if (event.key === 'Escape') { event.stopPropagation(); setRenaming(false); } }} />
          <Button type="submit" size="sm" variant="outline" disabled={settings.busy || !draft.trim()}>Save</Button>
        </Cluster>
      </form>
    );
  }
  return (
    <Column gap="xs" as="section" aria-label={session.name}>
      <Text variant="strong" truncate title={session.name}>{session.name}</Text>
      <Text variant="muted" title={`${sessionStateText(session.state)} · ${agoText(session.updatedAt)}`}>{sessionStateText(session.state)}</Text>
      <Cluster gap="xs">
        <Button size="sm" variant="outline" disabled={settings.busy} aria-label={`Show ${session.name}`} title="Show this session's tab, turning the thread on if it is off" onClick={() => { void settings.showSession(session.id); }}>Show</Button>
        <Button size="sm" variant="outline" disabled={settings.busy} aria-label={`Rename ${session.name}`} title="Give this session a name of your own" onClick={() => { setDraft(''); setRenaming(true); }}>Rename</Button>
        {session.state === 'unbound' ? <Button size="sm" variant="outline" disabled={settings.busy} aria-label={`Remove ${session.name}`} title="Remove this session from the thread: nothing was saved in it. Nothing else is deleted." onClick={() => { void settings.forgetSession(session); }}>Remove</Button> : null}
      </Cluster>
    </Column>
  );
}

function ClosedSession({ session }: { session: ThreadSession }) {
  const app = useApp();
  const settings = app.threadSettings;
  const action = closedSessionAction(session);
  return (
    <Column gap="xs" as="section" aria-label={session.name}>
      <Text variant="strong" truncate title={`${session.name} · ${agoText(session.updatedAt)}`}>{session.name}</Text>
      <Cluster gap="xs">
        <Button size="sm" variant="outline" disabled={settings.busy} title={action.title} aria-label={`${action.label} ${session.name}`} onClick={() => { void settings.restore(session.id); }}>{action.label}</Button>
        <Button size="sm" variant="destructive" disabled={settings.busy} title="Delete this session from Shinatama. The agent's own session files stay." aria-label={`Delete ${session.name}`} onClick={() => { void settings.forgetSession(session); }}>Delete</Button>
      </Cluster>
    </Column>
  );
}

export function ThreadPanel({ threadId }: { threadId: string }) {
  const app = useApp();
  const settings = useController(app.threadSettings);
  useController(app.threads);
  useController(app.chat);
  useEffect(() => { settings.show(threadId); return () => settings.forget(); }, [settings, threadId]);
  const thread = settings.thread;
  const [name, setName] = useState<string | null>(null);
  const shown = name ?? thread?.title ?? '';
  const sessions = settings.sessions;
  const open = sessions?.filter(session => session.state !== 'closed') ?? [];
  const closed = sessions?.filter(session => session.state === 'closed') ?? [];
  if (!thread) {
    return <Frame title="Thread settings"><Note tone="warning" title="This thread is not in the list any more.">It may have been deleted or archived from another window.</Note></Frame>;
  }
  const save = () => { void settings.rename(shown).then(done => { if (done) setName(null); }); };
  return (
    <Frame
      title={`Settings of ${settings.title}`}
      footer={(
        <Cluster gap="sm" justify="between">
          <Button variant="outline" disabled={settings.busy} title={thread.archived ? 'Put this thread back in the list.' : 'Move this thread out of the list. Nothing is deleted.'} onClick={() => { void settings.toggleArchived(); }}>{thread.archived ? 'Unarchive' : 'Archive'}</Button>
          <Button variant="destructive" disabled={settings.busy} title="Delete this thread and its sessions from Shinatama, after asking." onClick={() => { void settings.remove(); }}>Delete thread</Button>
        </Cluster>
      )}
    >
      <form onSubmit={event => { event.preventDefault(); save(); }} aria-label="Name of the thread">
        <Cluster gap="sm" align="end" wrap={false}>
          <Cell grow>
            <FormField label="Name">
              <Input value={shown} placeholder={settings.title} maxLength={256} onChange={event => setName(event.target.value)} />
            </FormField>
          </Cell>
          <Button type="submit" variant="outline" disabled={settings.busy || !shown.trim() || shown.trim() === (thread.title ?? '')}>Save</Button>
        </Cluster>
      </form>
      <Column gap="xs">
        <Switch checked={settings.talks && !thread.archived} disabled={settings.busy || thread.archived} onCheckedChange={on => { void settings.setTalking(on); }}>Sessions talk to each other</Switch>
        <Text variant="muted">Claude Code, Codex, OpenCode and Pi sessions opened from now on join the thread's Mukade room. Chat opens the conversation.</Text>
        {settings.talks && !thread.archived ? (
          <Cluster gap="sm">
            <Button size="sm" variant="outline" onClick={() => app.chat.open({ id: thread.id, projectId: thread.projectId })}>Open the conversation</Button>
            <Button size="sm" variant="outline" title="Who gives work to whom" onClick={() => app.team.open({ id: thread.id })}>Team map</Button>
          </Cluster>
        ) : null}
      </Column>
      {settings.message && settings.failed ? <Note tone="error" title={settings.message} /> : null}
      <Section title={sessions === null ? 'Sessions' : open.length ? `Sessions (${open.length})` : 'No open sessions in this thread.'} level={3}>
        {open.map(session => <OpenSession key={session.id} session={session} />)}
      </Section>
      {closed.length ? (
        <Section title={`Closed sessions (${closed.length})`} level={3}>
          {closed.map(session => <ClosedSession key={session.id} session={session} />)}
        </Section>
      ) : null}
    </Frame>
  );
}

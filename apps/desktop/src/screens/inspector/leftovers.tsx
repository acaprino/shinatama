// What agent sessions created in their projects (Xiox's observer): every worktree, copy, bench or backup folder and session folder,
// newest first, with who could have made it. Read-only: it copies paths and commands and selects a folder in Explorer; it never removes
// anything.
import { Badge, Button, Column, Cluster, Progress, Text } from '@acaprino/sigrid/primitives';
import { CopyButton, Empty, Note } from '@acaprino/sigrid/components';
import { useApp, useAfter, useController } from '../../hooks.ts';
import { ago, authorText, baseName, countText, size, terminalLabel, visibleItems, type LeftoverItem, type Leftovers } from '../../leftovers-model.ts';
import { Frame } from './frame.tsx';

const TAGS: Record<string, string> = { worktrees: 'Worktree', copies: 'Copy', named: 'Named', scratch: 'Session dir', builds: 'Build', logs: 'Log' };

function Item({ project, item, now }: { project: Leftovers; item: LeftoverItem; now: number }) {
  const app = useApp();
  const leftovers = app.leftovers;
  const where = item.path ?? item.label ?? item.key;
  const suspects = project.suspects[item.key] ?? [];
  const author = authorText(item, suspects);
  const open = item.sessions.length ? [] : suspects.filter(suspect => suspect.open);
  return (
    <Column gap="xs">
      <Cluster gap="sm" justify="between">
        <Cluster gap="sm">
          <Badge variant="outline">{TAGS[item.category] ?? item.category}</Badge>
          <Text variant="strong" truncate title={where}>{item.path ? baseName(item.path) : where}</Text>
        </Cluster>
        <Text variant="muted">{item.bytes === null ? '' : `${item.bytesPartial ? '≥ ' : ''}${size(item.bytes)}`}</Text>
      </Cluster>
      <Text mono truncate title={where}>{where}</Text>
      <Text variant="muted">{[item.reason, ago(item.firstSeenMs, now)].filter(Boolean).join(' · ')}</Text>
      {author ? <Text variant="muted">{author}</Text> : null}
      {item.blocked.length ? <Text variant="muted">Cannot be removed: {item.blocked.join(', ')}</Text> : null}
      <Cluster gap="sm">
        {open.map(suspect => <Button key={suspect.terminalId} size="sm" variant="outline" onClick={() => leftovers.goTo(suspect.terminalId)}>Go to {terminalLabel(suspect)}</Button>)}
        {item.path ? (
          <>
            <CopyButton value={item.path} label="Copy path" showLabel size="sm" variant="outline" onCopied={() => leftovers.say(`Copied ${item.path}`)} onCopyError={() => leftovers.say('The clipboard is unavailable.', true)} />
            <Button size="sm" variant="outline" title="Selects it in Explorer. Nothing is opened or removed." onClick={() => { void leftovers.reveal(item.path!); }}>Show in Explorer</Button>
          </>
        ) : null}
      </Cluster>
    </Column>
  );
}

function ProjectView({ project, now }: { project: Leftovers; now: number }) {
  const app = useApp();
  const leftovers = app.leftovers;
  const items = visibleItems(project);
  const archived = project.newItems.length - items.length;
  const folder = project.root ?? project.folders[0];
  const name = project.name ?? baseName(project.folders[0] ?? '');
  const live = project.liveSessions ? `${project.liveSessions} session${project.liveSessions === 1 ? '' : 's'} open` : 'no session open';
  const phase = project.phase === 'starting' ? 'first snapshot…' : project.phase === 'ended' ? `stopped${project.message ? `: ${project.message}` : ''}` : `checked ${ago(project.checkedAtMs ?? project.startedAtMs, now) || 'at start'}`;
  const facts = [
    items.length ? countText(items) : 'nothing new', project.growthBytes ? `+${size(project.growthBytes)}` : '',
    archived ? `${archived} archived session${archived === 1 ? '' : 's'}` : '', project.newItemsOmitted ? `${project.newItemsOmitted} more not listed` : '',
    project.stale ? 'last check failed' : '',
  ].filter(Boolean).join(' · ');
  const attention = project.attention.map(kind => kind === 'growth' ? 'grew over the limit' : kind === 'worktrees' ? 'too many new worktrees' : kind);
  return (
    <Column gap="sm" as="section" aria-label={name}>
      <Cluster gap="sm" justify="between">
        <Text variant="strong">{name}</Text>
        <Text variant="muted">{live} · {phase}</Text>
      </Cluster>
      <Text variant="muted">{facts}</Text>
      {attention.length ? <Note tone="warning" title={`Attention: ${attention.join(', ')}`} /> : null}
      {items.length ? <Column gap="md">{items.map(item => <Item key={item.key} project={project} item={item} now={now} />)}</Column> : null}
      <Cluster gap="sm">
        {folder ? <Button size="sm" variant="outline" onClick={() => { void leftovers.check(folder, name); }}>{project.phase === 'ended' ? 'Restart Xiox' : 'Check now'}</Button> : null}
        {project.cleanCommand ? (
          <CopyButton
            value={project.cleanCommand} label="Copy clean command" showLabel size="sm" variant="outline"
            onCopied={() => leftovers.say(`Clean command copied${project.cleanCommandTruncated ? ' (first 25 items)' : ''}. Paste it into a terminal: xiox clean lists what it removes and asks first.`)}
            onCopyError={() => leftovers.say('The clipboard is unavailable.', true)}
          />
        ) : null}
      </Cluster>
    </Column>
  );
}

export function LeftoversPanel() {
  const app = useApp();
  const leftovers = useController(app.leftovers);
  const { status, loaded, readFailure, note } = leftovers;
  const waiting = useAfter(!loaded);
  const now = Date.now();
  let body;
  if (!loaded) body = waiting ? <Progress aria-label="Reading what agents left on disk" value={null} /> : null;
  else if (!status.available) {
    body = readFailure !== null
      ? <Note tone="error" title={readFailure} actions={<Button size="sm" variant="outline" onClick={() => { void leftovers.refresh(); }}>Retry</Button>} />
      : <Empty title="Not watching" description={status.reason ?? 'Xiox is not running.'} />;
  } else if (!status.projects.length) body = <Empty title="Nothing watched yet" description="Xiox starts with the first agent session of a project." />;
  else {
    const ordered = [...status.projects].sort((a, b) => visibleItems(b).length - visibleItems(a).length || b.liveSessions - a.liveSessions);
    body = (
      <Column gap="lg">
        <Text variant="muted">{leftovers.count} new across {status.projects.length} project{status.projects.length === 1 ? '' : 's'}. Nothing is removed.</Text>
        {ordered.map(project => <ProjectView key={project.root ?? project.folders[0]} project={project} now={now} />)}
      </Column>
    );
  }
  return (
    <Frame title="Leftovers">
      <Text variant="muted">What agent sessions created: worktrees, copies, bench and backup folders. Shinatama never removes anything.</Text>
      {note ? <Note tone={note.error ? 'error' : 'info'} title={note.text} /> : null}
      {body}
    </Frame>
  );
}

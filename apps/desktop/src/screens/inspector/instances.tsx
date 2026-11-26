// The Shinatama windows and their agents that run: the state of each with its source, its branch, what it is doing and what it costs,
// a Focus that takes the person to its pane, the files two instances both changed, and the hops that were made lately. The states come
// only from what Shinatama observed; a terminal signal never approves anything.
import { useEffect } from 'react';
import { Badge, Button, Column, Cluster, Section, Text } from '@acaprino/sigrid/primitives';
import { DescriptionList, Empty, ItemList, ItemRow, Note } from '@acaprino/sigrid/components';
import { useApp, useController } from '../../hooks.ts';
import {
  collisionText, doingText, gitText, hopTime, SOURCE_WORDS, STATE_WORDS, stateTone, summaryLine, usageText, type InstanceRow,
} from '../../instances-model.ts';
import { agentName } from '../../session-library-model.ts';
import { Frame } from './frame.tsx';

function Instance({ row }: { row: InstanceRow }) {
  const app = useApp();
  const sessions = app.sessions;
  const titled = sessions.titles?.rows.find(entry => entry.instanceId === row.id);
  const thread = app.terminals.get(row.id)?.threadId ?? null;
  const paused = thread !== null && (sessions.titles?.excluded.includes(thread) ?? false);
  const doing = doingText(titled);
  const name = `${agentName(row.agent)} ${row.id.slice(-4)}`;
  return (
    <Column gap="sm" as="section" aria-label={name} data-instance-id={row.id}>
      <Cluster gap="sm" justify="between">
        <Cluster gap="sm">
          <Text variant="strong">{agentName(row.agent)}</Text>
          <Text variant="muted" title={row.id}>{row.id.slice(-4)}</Text>
        </Cluster>
        <Cluster gap="xs">
          {thread ? <Button size="sm" variant="outline" onClick={() => { void sessions.pauseSummaries(thread, !paused); }}>{paused ? 'Resume summaries' : 'Pause summaries'}</Button> : null}
          <Button size="sm" variant="outline" aria-label={`Focus ${name}`} onClick={() => { if (sessions.focusRunning(row.id)) app.ui.closePanels(); else app.ui.info('This instance has no pane in this window.'); }}>Focus</Button>
        </Cluster>
      </Cluster>
      <Cluster gap="sm">
        <Badge tone={stateTone(row.state)} variant="outline">{STATE_WORDS[row.state] ?? row.state}</Badge>
        <Text variant="muted">{row.certain ? 'certain' : 'inferred'} · {SOURCE_WORDS[row.stateSource] ?? row.stateSource}</Text>
      </Cluster>
      <DescriptionList
        aria-label={`Facts of ${name}`}
        items={[
          { name: 'Folder', value: row.cwd, code: true },
          { name: 'Git', value: gitText(row.git) },
          doing ? { name: 'Doing', value: doing } : null,
          titled?.usage ? { name: 'Tokens', value: usageText(titled.usage) } : null,
        ]}
      />
    </Column>
  );
}

export function InstancesPanel() {
  const app = useApp();
  const sessions = useController(app.sessions);
  useEffect(() => sessions.watchRunning(), [sessions]);
  const { running, titles, hops, runningProblem, runningRead } = sessions;
  const collisions = titles?.collisions ?? [];
  return (
    <Frame title="Running sessions">
      {runningProblem ? <Note tone="error" title={runningProblem} actions={<Button size="sm" variant="outline" onClick={() => { void sessions.refreshRunning(true); }}>Retry</Button>} /> : null}
      {runningRead && !running.length && !runningProblem ? <Empty title="No session is running" description="Sessions that run in any Shinatama window are listed here." /> : null}
      {running.length ? <Text variant="muted">{summaryLine(running)}</Text> : null}
      {collisions.length ? (
        <Note tone="warning" title={collisions.length === 1 ? 'Possible conflict' : 'Possible conflicts'} items={collisions.map(entry => collisionText(entry, running))} />
      ) : null}
      {running.map(row => <Instance key={row.id} row={row} />)}
      <Text variant="muted">States come only from what Shinatama observed. Running states are unsure until native agent hooks are connected; a terminal signal never approves anything.</Text>
      {hops.length ? (
        <Section title="Recent hops" level={3}>
          <ItemList aria-label="Recent hops">
            {hops.map(hop => (
              <ItemRow key={hop.id} title={`${agentName(hop.sourceAgent)} → ${agentName(hop.destinationAgent)}`} description={`${hop.mode} · ${hop.state}`} meta={hopTime(hop.updatedAtMs)} />
            ))}
          </ItemList>
        </Section>
      ) : null}
    </Frame>
  );
}

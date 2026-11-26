// The conversation of one saved session, read a page at a time and never started: its overview, its transcript (user, assistant, tool
// calls and results; reasoning and attachments are not read), its lineage, and the tools that act on what was read: search in it, export
// it, hand it to another agent, and read one raw entry. Read-only; a session that changed since is refused until it is reloaded.
import { useEffect, useState } from 'react';
import { DescriptionList, Message, Note, ToolCall } from '@acaprino/sigrid/components';
import { Button, Cluster, Column, Heading, Icon, Progress, Tabs, TabsContent, TabsList, TabsTrigger, Text } from '@acaprino/sigrid/primitives';
import { useApp, useAfter, useController } from '../../hooks.ts';
import {
  EVENT_KINDS, LINEAGE_STATES, ledgerFacts, positionText, type DetailEvent, type DetailPage, type LineageNode,
} from '../../session-detail-model.ts';
import { agentName, type SessionRootSelector } from '../../session-library-model.ts';
import { Frame } from './frame.tsx';

type Tab = 'overview' | 'transcript' | 'lineage';

function EventView({ event, page, onRaw, open }: { event: DetailEvent; page: DetailPage; onRaw: () => void; open: boolean }) {
  const label = EVENT_KINDS[event.kind];
  const text = event.text || '(No visible text recorded)';
  let body;
  switch (event.kind) {
    case 'user_text': body = <Message from="user" name={label}>{text}</Message>; break;
    case 'assistant_text': body = <Message from="assistant" name={label}>{text}</Message>; break;
    case 'tool_call': body = <ToolCall name={label} input={text} defaultOpen={open} />; break;
    case 'tool_result': body = <ToolCall name={label} output={text} defaultOpen={open} />; break;
    case 'compaction': body = <Note tone="info" title={label}>{text}</Note>; break;
    default: body = <Text variant="muted">{label}: {text}</Text>;
  }
  return (
    <Column gap="xs" aria-label={`${label}, entry ${event.entryOrdinal}`}>
      {body}
      <Cluster gap="sm" justify="between">
        <Text variant="caption" truncate title={`Entry ${event.entryOrdinal} · ID ${event.sourceEntryId ?? 'Unknown'} · time ${event.timestamp ?? 'Unknown'}`}>
          Entry {event.entryOrdinal} · {event.timestamp ?? 'time unknown'}
        </Text>
        <Button size="xs" variant="ghost" aria-label={`RAW entry ${event.entryOrdinal}`} title="Read the native record of this entry (behind a warning)" onClick={onRaw}><Icon name="code" />RAW</Button>
      </Cluster>
      {event.truncated || event.payloadOmitted ? <Text variant="muted">{event.truncated ? 'Displayed text is truncated at its fixed limit.' : 'Sensitive or raw payload is omitted.'}</Text> : null}
    </Column>
  );
}

function Lineage({ page }: { page: DetailPage }) {
  const app = useApp();
  const report = page.lineage;
  const branch = (label: string, nodes: LineageNode[], state: keyof typeof LINEAGE_STATES) => (
    <Column gap="xs" as="section" aria-label={label}>
      <Text variant="strong">{label} · {LINEAGE_STATES[state]}</Text>
      {nodes.map(node => (
        <Cluster key={node.nativeId} gap="sm">
          <Button size="sm" variant="outline" disabled={!node.canSelect} title={LINEAGE_STATES[node.state]} onClick={() => app.sessions.show(page.root, node.nativeId)}>{node.nativeId}</Button>
          <Text variant="muted">{LINEAGE_STATES[node.state]}</Text>
        </Cluster>
      ))}
    </Column>
  );
  return (
    <Column gap="md">
      <Text>Parent session ID: <Text mono>{page.overview.parentNativeId ?? 'Unknown'}</Text></Text>
      {branch('Ancestors', report.ancestors, report.ancestorState)}
      {branch('Direct children', report.children, report.childrenState)}
      <Text variant="muted">Direct children recorded: {report.totalDirectChildren ?? 'Unknown'}. Links open the read-only detail in this archive.</Text>
    </Column>
  );
}

export function SessionPanel({ root, nativeId, at }: { root: SessionRootSelector; nativeId: string; at?: { offset: number; fingerprint: string } }) {
  const app = useApp();
  const detail = useController(app.sessions.detail);
  const [tab, setTab] = useState<Tab>('transcript');
  const offset = at?.offset;
  const fingerprint = at?.fingerprint;
  // Shown when the panel is drawn, kept while another panel covers it (the way back finds it as it was), let go when it is closed.
  useEffect(() => {
    const jump = offset === undefined ? null : { offset, fingerprint: fingerprint! };
    if (!app.sessions.detail.isShowing(root, nativeId, jump)) app.sessions.detail.show(root, nativeId, jump);
    return () => {
      if (!app.ui.panels.some(panel => panel.kind === 'session' && panel.nativeId === nativeId)) app.sessions.detail.close();
    };
  }, [app, root.storeRootId, root.configurationRevision, nativeId, offset, fingerprint]); // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => app.sessions.detail.watchProxy(), [app, root.storeRootId, root.configurationRevision, nativeId]); // eslint-disable-line react-hooks/exhaustive-deps
  const { page, busy, failed, message } = detail;
  const waiting = useAfter(busy && !page);
  const request = detail.request;
  const tools = (
    <Cluster gap="sm">
      <Button size="sm" variant="outline" disabled={busy} onClick={() => { void detail.reload(); }}>Reload detail</Button>
      <Button size="sm" variant="outline" disabled={busy || !page} onClick={() => { if (request) { app.ui.openPanel({ kind: 'find', root: request.root, nativeId: request.nativeId, fingerprint: request.cursor!.fingerprint }); } }}>Find in conversation</Button>
      <Button size="sm" variant="outline" disabled={busy || !page} onClick={() => { if (page) app.ui.openPanel({ kind: 'export', selection: { root: page.root, nativeId: page.nativeId, fingerprint: page.fingerprint, harness: page.harness } }); }}>Export…</Button>
      <Button size="sm" disabled={busy || !page} onClick={() => { if (page) app.ui.openPanel({ kind: 'hop', selection: { root: page.root, nativeId: page.nativeId, fingerprint: page.fingerprint, harness: page.harness } }); }}>Hop…</Button>
    </Cluster>
  );
  return (
    <Frame title="Session detail">
      {page?.overview.title ? <Heading size="small" level={3}>{page.overview.title}</Heading> : null}
      <Text variant="muted" mono truncate title={`Session ${nativeId} · Archive ${root.storeRootId} · revision ${root.configurationRevision}`}>{`Session ${nativeId} · Archive ${root.storeRootId} · revision ${root.configurationRevision}`}</Text>
      {tools}
      {failed && !page ? <Note tone="error" title={message} actions={<Button size="sm" variant="outline" onClick={() => { void detail.reload(); }}>Retry</Button>} /> : <Text role="status" variant="muted">{busy ? 'Reading bounded session detail…' : message}</Text>}
      {busy && !page && waiting ? <Progress aria-label="Reading the session" value={null} /> : null}
      {page ? (
        <Tabs value={tab} onValueChange={value => setTab(value as Tab)}>
          <TabsList aria-label="Session detail">
            <TabsTrigger value="overview">Overview</TabsTrigger>
            <TabsTrigger value="transcript">Transcript</TabsTrigger>
            <TabsTrigger value="lineage">Lineage</TabsTrigger>
          </TabsList>
          <TabsContent value="overview">
            <DescriptionList
              aria-label="Overview"
              items={[
                { name: 'Harness', value: agentName(page.harness) }, { name: 'Harness version', value: page.overview.harnessVersion ?? 'Unknown' },
                { name: 'Title', value: page.overview.title ?? 'Unknown' }, { name: 'Working folder', value: page.overview.workingDirectory ?? 'Unknown', code: true },
                { name: 'Created', value: page.overview.createdAt ?? 'Unknown' }, { name: 'Updated', value: page.overview.updatedAt ?? 'Unknown' },
                ...ledgerFacts(page.ledger),
                detail.proxyLines ? { name: 'Model proxy', value: <Column gap="xs">{detail.proxyLines.map(line => <Text key={line}>{line}</Text>)}</Column> } : null,
              ]}
            />
          </TabsContent>
          <TabsContent value="transcript">
            <Column gap="md">
              {page.events.map(event => (
                <EventView key={event.ordinal} event={event} page={page} open={detail.jump === event.ordinal}
                  onRaw={() => { if (!busy) app.ui.openPanel({ kind: 'raw', selection: { root: page.root, nativeId: page.nativeId, entryOrdinal: event.entryOrdinal, fingerprint: page.fingerprint } }); }} />
              ))}
              {page.notices.map(notice => <Note key={notice} tone="info" title={notice} />)}
              <Cluster gap="sm" justify="between">
                <Cluster gap="sm">
                  <Button size="sm" variant="outline" disabled={busy || !detail.history.length} onClick={() => { void detail.previous(); }}>Previous</Button>
                  <Button size="sm" variant="outline" disabled={busy || !page.next} onClick={() => { void detail.next(); }}>Next</Button>
                </Cluster>
                <Text variant="muted">{positionText(detail.cursor?.offset ?? 0, page.events.length, page.totalEvents, 'events')}{!detail.history.length && (detail.cursor?.offset ?? 0) > 0 ? ' · Reload for first page' : ''}</Text>
              </Cluster>
              <Text variant="muted">RAW needs an explicit privacy confirmation. Export redacts secrets and never includes reasoning. Attachment files stay closed.</Text>
            </Column>
          </TabsContent>
          <TabsContent value="lineage"><Lineage page={page} /></TabsContent>
        </Tabs>
      ) : null}
    </Frame>
  );
}

// The preview of a hop: the exact seed that will be sent to another agent, byte for byte (R06), what it carries and what it loses, and
// the hop itself once the person confirms. Nothing is sent before: the prompt tells the destination to read the private seed file, and
// reasoning is never included.
import { useEffect } from 'react';
import { CopyButton, DescriptionList, Note } from '@acaprino/sigrid/components';
import { Button, Cluster, Column, Progress, RadioGroup, RadioGroupItem, Section, Text, TextList } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { destinations, hopFacts, type HopSelection } from '../../session-export-model.ts';
import { AGENT_CHOICES, agentName } from '../../session-library-model.ts';
import { Frame } from './frame.tsx';

export function HopPanel({ selection }: { selection: HopSelection }) {
  const app = useApp();
  const hop = useController(app.sessions.hopPreview);
  const options = destinations(selection.harness, AGENT_CHOICES);
  useEffect(() => {
    app.sessions.hopPreview.open(selection, options[0]?.[0] ?? '');
    return () => app.sessions.hopPreview.close();
  }, [app, selection]); // eslint-disable-line react-hooks/exhaustive-deps
  const { preview, busy } = hop;
  return (
    <Frame
      title="Hop to another agent"
      footer={preview ? (
        <Cluster gap="sm" justify="between">
          <CopyButton value={preview.bootstrap} label="Copy hand-off prompt" showLabel variant="outline" onCopied={() => app.ui.info('Hand-off prompt copied. Paste it into the destination agent; your clipboard may be visible to other applications.')} onCopyError={() => app.ui.error('The clipboard is unavailable.')} />
          <Button disabled={busy} onClick={() => { void hop.send(); }}>Hop to {agentName(preview.destination)}</Button>
        </Cluster>
      ) : undefined}
    >
      <Text variant="muted">{`From ${agentName(selection.harness)} · session ${selection.nativeId}`}</Text>
      <Section title="Destination" level={4}>
        <RadioGroup aria-label="Destination" value={hop.destination} disabled={busy} onValueChange={value => hop.setDestination(value)}>
          {options.map(([id, label]) => <RadioGroupItem key={id} value={id}>{label}</RadioGroupItem>)}
        </RadioGroup>
        <Cluster><Button variant={preview ? 'outline' : 'default'} disabled={busy || !hop.destination} onClick={() => { void hop.prepare(); }}>Prepare preview</Button></Cluster>
      </Section>
      {hop.failed ? <Note tone="error" title={hop.message} /> : <Text role="status" variant="muted">{hop.message}</Text>}
      {busy ? <Progress aria-label="Working" value={null} /> : null}
      {preview ? (
        <Column gap="md">
          <DescriptionList aria-label="What the destination receives" items={hopFacts(preview)} />
          {preview.carried.length ? <Section title="Carried" level={4}><TextList items={preview.carried} /></Section> : null}
          {preview.lost.length ? <Section title="Lost" level={4}><TextList items={preview.lost} /></Section> : null}
          <Section title="Exact seed" level={4}>
            <Text as="div" pre mono tabIndex={0} aria-label="Exact seed">{preview.seed}</Text>
          </Section>
        </Column>
      ) : null}
      <Text variant="muted">Nothing is sent until you confirm. The prompt tells the destination to read the private seed file; reasoning is never included.</Text>
    </Frame>
  );
}

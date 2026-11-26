// The attention widget's page: a small notice that floats over the desktop when Shinatama is behind another app. Its window is always on top
// and never takes the focus (src-tauri/src/widget_window.rs and attention.rs). It is one Card of Sigrid's parts: the portrait in an Avatar, the
// kind of notice in its tone, the title, the words, and the two answers. Its text stays Italian, as it has been.
import { useEffect, useMemo } from 'react';
import { useCommand } from '@acaprino/sigrid/assemblies';
import { Avatar, Button, Card, Cluster, Column, Cell, Heading, Icon, Text } from '@acaprino/sigrid/primitives';
import { useController } from '../hooks.ts';
import { KINDS_SAY, openLabel, WIDGET_WORDS, type Expression } from '../attention-model.ts';
import { AttentionWidget } from './attention.ts';
import curious from '../../../../docs/assets/widget/shinatama-curious-v1.png';
import happy from '../../../../docs/assets/widget/shinatama-happy-v1.png';
import smile from '../../../../docs/assets/widget/shinatama-smile-v1.png';

const PORTRAITS: Readonly<Record<Expression, string>> = { smile, happy, curious };

export function Widget({ widget }: { widget?: AttentionWidget }) {
  const own = useMemo(() => widget ?? new AttentionWidget(), [widget]);
  useEffect(() => { void own.start(); return () => own.dispose(); }, [own]);
  const { notice, expression, busy, problem } = useController(own);
  // Escape answers the notice the way its close button does. The window takes no focus, so this is for a person who gave it the focus.
  useCommand({ id: 'attention.dismiss', label: WIDGET_WORDS.dismiss, keys: 'Escape', when: notice !== null && !busy, run: () => { void own.dismiss(); } });
  if (!notice) return null;
  const kind = KINDS_SAY[notice.kind];
  return (
    <Card as="section" aria-label={WIDGET_WORDS.window} data-kind={notice.kind}>
      <Cluster wrap={false} align="start" gap="md">
        <Avatar name={WIDGET_WORDS.name} src={PORTRAITS[expression]} decorative />
        <Cell grow>
          <Column gap="sm">
            <Cluster wrap={false} justify="between">
              <Text variant="kicker" tone={kind.tone}>{kind.label}</Text>
              <Button variant="ghost" size="icon-xs" aria-label={WIDGET_WORDS.dismiss} title={WIDGET_WORDS.dismiss} disabled={busy} onClick={() => { void own.dismiss(); }}>
                <Icon name="close" />
              </Button>
            </Cluster>
            <Heading size="small" level={2} truncate>{notice.title}</Heading>
            <Text as="p" variant="soft">{notice.body}</Text>
            {problem ? <Text as="p" variant="note" tone="danger" role="alert">{problem}</Text> : null}
            <Cluster>
              <Button size="sm" disabled={busy} onClick={() => { void own.open(); }}>
                {openLabel(notice)}
                <Icon name="external" />
              </Button>
            </Cluster>
          </Column>
        </Cell>
      </Cluster>
    </Card>
  );
}

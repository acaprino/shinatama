// TEAM. The team map of a thread (ADR-0058): who works with whom. It is the content of a tab of a pane (kind `team`, id `team-<thread>`) and
// takes the whole sheet when the tab is shown. The map is Sigrid's `GraphCanvas` in its graph mode, on the places the person gave the
// cards; an agent and an arrow are edited in the window's inspector (./inspector.tsx) and Apply is in the actions row of the window
// (`TeamActions`). What the map remembers is `TeamMap` (controllers/team-map.ts): this draws it and calls its methods.
import { useEffect, useState } from 'react';
import '@acaprino/sigrid/graph.css';
import { GraphCanvas, GraphLegend, type Graph, type GraphChange, type GraphEdge, type GraphNode } from '@acaprino/sigrid/graph';
import { Disclosure, Empty, ItemList, ItemRow, Note } from '@acaprino/sigrid/components';
import { Panel } from '@acaprino/sigrid/layout';
import { Button, Checkbox, Cluster, Column, Icon, Input, Popover, PopoverContent, PopoverTrigger, Text, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { CARD } from '../../team-map-model.ts';
import { KIND_NAME, SHAPES, type ShapeId, type TeamMap } from '../../controllers/team-map.ts';

/** The cards and the arrows as the canvas takes them. */
export function graphOf(map: TeamMap): Graph {
  const bad = map.badAgents(), badLinks = map.badLinks(), pending = map.pendingLinkSet();
  const nodes: GraphNode[] = map.draft.agents.map((a): GraphNode => {
    const told = map.toldState(a);
    return {
      id: a.id,
      label: `${bad.has(a.id) ? '✕ ' : ''}${a.name || '(no name)'}`,
      detail: `${map.roleOf(a)} · ${told.text}`,
      icon: 'terminal',
      x: a.x, y: a.y, width: CARD.width + 50, height: CARD.height - 20,
    };
  });
  const edges: GraphEdge[] = map.draft.links.map((l, i): GraphEdge => {
    const said = l.kind === 'work' ? (l.report === 'strict' ? 'gives work · only done / blocked' : l.report === 'steps' ? 'gives work · every step' : 'gives work to') : 'observes';
    const short = l.words.length > 26 ? `${l.words.slice(0, 25)}…` : l.words;
    return {
      id: `${l.from}>${l.to}`, from: l.from, to: l.to, kind: 'link', directed: true,
      label: `${said}${short ? ` · “${short}”` : ''}`,
      style: l.kind === 'observes' ? 'dotted' : pending.has(i) ? 'dashed' : 'solid',
      tone: badLinks.has(i) ? 'destructive' : 'default',
    };
  });
  return { nodes, edges };
}

function Templates({ map }: { map: TeamMap }) {
  const [open, setOpen] = useState(false);
  const [name, setName] = useState('');
  const block = map.saveBlock();
  const save = async () => { if (await map.saveTemplate(name)) setName(''); };
  return (
    <Popover open={open} onOpenChange={next => { setOpen(next); if (next) void map.loadTemplates(); }}>
      <PopoverTrigger asChild><Button variant="outline" size="sm" title="Teams saved to use again, in any thread">Templates<Icon name="chevronDown" /></Button></PopoverTrigger>
      <PopoverContent width="sm">
        <Column gap="md">
          <Text variant="strong">Templates</Text>
          {map.templates === null ? <Text variant="muted">Reading the templates…</Text> : map.templates.length === 0
            ? <Text variant="muted">No template yet. Build a team, name it below and press Save this team.</Text>
            : (
              <ItemList aria-label="Saved teams">
                {map.templates.map(t => (
                  <ItemRow
                    key={t.id}
                    title={t.name}
                    description={`${t.agents.length} agent${t.agents.length === 1 ? '' : 's'}`}
                    onClick={() => { map.useTemplate(t); setOpen(false); }}
                    controlSide="end"
                    control={<Button variant="ghost" size="icon-xs" aria-label={`Delete ${t.name}`} title="Delete this template; the teams made from it are not touched" onClick={() => { void map.deleteTemplate(t); }}><Icon name="delete" /></Button>}
                  />
                ))}
              </ItemList>
            )}
          <Cluster gap="sm" wrap={false}>
            <Input aria-label="Name of the template" placeholder="Name this team" maxLength={60} value={name} onChange={event => setName(event.target.value)}
              onKeyDown={event => { if (event.key === 'Enter') { event.preventDefault(); void save(); } }} />
            <Button variant="outline" disabled={!!block || !name.trim() || map.templatesBusy} title={block || (!name.trim() ? 'Give the team a name first.' : 'Keep this team, with its places, to use again in any thread')} onClick={() => { void save(); }}>Save this team</Button>
          </Cluster>
          {map.templateNote.text ? <Text variant="muted" tone={map.templateNote.tone === 'error' ? 'danger' : undefined}>{map.templateNote.text}</Text> : null}
        </Column>
      </PopoverContent>
    </Popover>
  );
}

function Bar({ map }: { map: TeamMap }) {
  const few = map.draft.agents.length < 2;
  return (
    <Cluster gap="md">
      <Text variant="muted">Arrange as</Text>
      <ToggleGroup aria-label="Arrange the team as" value="" onValueChange={value => { if (value) map.arrangeAs(value as ShapeId); }}>
        {SHAPES.map(shape => <ToggleGroupItem key={shape.id} value={shape.id} disabled={few || map.applying}>{shape.name}</ToggleGroupItem>)}
      </ToggleGroup>
      <Button variant="outline" size="sm" disabled={!map.draft.agents.length || map.applying} title="Place the cards by their arrows: who gives work above, who takes it below, observers on the right" onClick={() => map.tidyNow()}>Tidy up</Button>
      <Checkbox label="Keep tidy" checked={map.keepTidy} onCheckedChange={on => map.setKeepTidy(on === true)} />
      <Templates map={map} />
      <Button variant="outline" size="sm" disabled={!map.snap?.enabled || map.applying || map.draft.agents.length >= 24} title="An agent with no session yet: choose its model and Apply opens one, or choose a session of the thread for it" onClick={() => map.addEmptyAgent()}><Icon name="plus" />New agent</Button>
    </Cluster>
  );
}

function Sessions({ map }: { map: TeamMap }) {
  const free = map.freeSessions();
  const full = map.draft.agents.length >= 24;
  return (
    <Column gap="sm">
      {free.length ? (
        <ItemList aria-label="Sessions of the thread">
          {free.map(s => {
            const off = s.connection === 'unsupported' || full;
            return (
              <ItemRow
                key={s.sessionId}
                title={map.sessionLabel(s)}
                description={`${map.harnessLabel(s.harness)} · ${s.state.replace('_', ' ')}${s.connection === 'unsupported' ? ' · cannot take part' : ''}`}
                disabled={off}
                onClick={() => map.addSessionAt(s)}
                end={<Icon name="plus" />}
              />
            );
          })}
        </ItemList>
      ) : (
        <Text variant="muted">{map.snap?.sessions.length ? 'Every session of the thread is on the map.' : 'This thread has no session yet. + on its row adds one.'}</Text>
      )}
    </Column>
  );
}

function Canvas({ map }: { map: TeamMap }) {
  // The width the map has on screen is what tidying and arranging lay the cards out for.
  useEffect(() => {
    const canvas = document.querySelector('[aria-label="Map of the team"]');
    if (!canvas) return;
    const watch = new ResizeObserver(() => map.setWidth(canvas.clientWidth));
    watch.observe(canvas);
    map.setWidth(canvas.clientWidth);
    return () => watch.disconnect();
  }, [map]);
  const graph = graphOf(map);
  const onChange = (_next: Graph, change: GraphChange) => {
    if (change.kind === 'join') map.connectAgents(change.edge.from, change.edge.to);
    else if (change.kind === 'remove') {
      if (change.nodes.length) map.takeOffMany(change.nodes);
      else for (const edge of change.edges) map.deleteLink(edge.from, edge.to);
    }
  };
  const selected = map.selection && 'agent' in map.selection ? map.selection.agent : null;
  return (
    <GraphCanvas
      fill
      model="graph"
      direction="down"
      aria-label="Map of the team"
      nodes={graph.nodes}
      edges={graph.edges}
      layoutKey={`${map.thread.id}:${map.layoutRevision}`}
      selected={selected}
      onSelect={id => map.selectAgent(id)}
      movable
      onMove={(id, place) => map.moveAgent(id, place)}
      editable
      onChange={onChange}
    />
  );
}

export function Team({ threadId }: { threadId: string }) {
  const app = useApp();
  const map = app.team.map(threadId);
  useController(map);
  useEffect(() => {
    app.team.show(threadId, true);
    return () => app.team.show(threadId, false);
  }, [app, threadId]);
  if (!map.enabled) {
    return <Empty fill title="This thread has no team" description="The sessions of this thread do not talk to each other, so it has no team. Turn it on in the thread's settings." />;
  }
  const issues = map.issues();
  return (
    <Panel title="Team map" subtitle={map.title()} tools={<Bar map={map} />} footer={<TeamKey map={map} />}>
      <Column fill gap="sm" pad="sm">
        <Disclosure label="Sessions of the thread" summary={`${map.freeSessions().length} not on the map`} >
          <Sessions map={map} />
        </Disclosure>

          {issues.length ? (
            <Note tone={issues.some(i => i.level === 'error') ? 'error' : 'warning'} title="Problems with this team" items={issues.map(issue => (
              <Button key={`${issue.code}:${issue.text}`} variant="link" size="inline" onClick={() => map.selectIssue(issue)}>{issue.level === 'error' ? 'Error: ' : 'Warning: '}{issue.text}</Button>
            ))} />
          ) : null}
          {map.status.text ? <Note tone={map.status.tone === 'error' ? 'error' : 'info'} title={map.status.text} /> : null}
          {map.draft.agents.length === 0 && map.snap ? (
            <Text variant="muted">Choose a session from the list to put it on the map (or New agent). Then join two cards by dragging from the edge of one to the other, and choose the arrow in the box at the end to say what kind of relation it is. Or start from Templates.</Text>
          ) : null}
        <Canvas map={map} />
      </Column>
    </Panel>
  );
}

function TeamKey({ map }: { map: TeamMap }) {
  return (
    <Cluster gap="md" justify="between">
      <GraphLegend aria-label="Kinds of arrow" entries={[{ label: KIND_NAME.work }, { label: KIND_NAME.observes, style: 'dotted' }, { label: 'not applied', style: 'dashed' }]} />
      <Text variant="muted">{map.draft.agents.length ? 'Delete takes the chosen card off the map (its session stays open)' : ''}</Text>
    </Cluster>
  );
}


// The team map's box at the end of the window (the shell's inspector) and its actions row. The box is about what is chosen on the map: an
// agent (its name, its model or the session that takes its place, how it hands out work, notes for it, who it gives work to) or an arrow
// (the kind of relation, how it reports, words of the person, what each is told). Apply and Discard are the actions of the window.
import { useEffect, useState } from 'react';
import { FormField, ItemList, ItemRow, Note } from '@acaprino/sigrid/components';
import { Inspector } from '@acaprino/sigrid/layout';
import { Badge, Button, Cluster, Column, Icon, Input, Select, Spacer, Text, Textarea, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { KIND_NAME, KIND_TIP, MAX_NAME_LENGTH, MAX_WORDS, MAX_NOTES_BYTES, REPORTS, SHARE_NAME, SHARE_TIP, modelKey, type TeamMap } from '../../controllers/team-map.ts';
import { MAX_LINKS, type Link, type Share } from '../../team-map-model.ts';

function NameField({ map, id }: { map: TeamMap; id: string }) {
  const agent = map.agentOf(id)!;
  const [text, setText] = useState(agent.name);
  useEffect(() => { setText(agent.name); }, [id, agent.name]);
  return (
    <FormField label="Name">
      <Input
        value={text}
        maxLength={MAX_NAME_LENGTH}
        onChange={event => { setText(event.target.value); map.setName(id, event.target.value); }}
        onBlur={() => { if (!text.trim()) setText(map.agentOf(id)?.name ?? 'Agent'); }}
      />
    </FormField>
  );
}

function AgentBox({ map, id }: { map: TeamMap; id: string }) {
  const a = map.agentOf(id)!;
  const view = map.current()?.roles[id];
  const others = map.draft.agents.filter(x => x.id !== id);
  const gives = map.draft.links.filter(l => l.kind === 'work' && l.from === id).map(l => map.nameOf(l.to));
  const mine = map.draft.links.filter(l => l.from === id || l.to === id);
  const lost = a.model && !map.holds(a.model) ? a.model : null;
  const modelOptions = [
    ...map.models.map(m => ({ value: modelKey(m), label: m.label })),
    ...(lost ? [{ value: modelKey(lost), label: `${lost.model ?? lost.program} · no longer in the Models settings` }] : []),
  ];
  const free = map.takeable();
  const full = map.draft.links.length >= MAX_LINKS;
  return (
    <Column gap="md">
      <NameField map={map} id={id} />
      {a.session ? (
        <FormField label="Model" help="A running session keeps its model.">
          <Input disabled value={a.model ? map.modelText(a.model) : `${map.programOf(a)} · its own sign-in and model`} readOnly />
        </FormField>
      ) : (
        <>
          <FormField label="Model" help="What the session opened for this agent runs; a session chosen for it keeps its own model.">
            <Select aria-label="Model" placeholder="Choose a model…" options={modelOptions} value={a.model ? modelKey(a.model) : ''} onValueChange={value => map.setModel(id, value)} />
          </FormField>
          <FormField label="Use a session" help="The session takes this agent's place: its name and arrows stay.">
            <Select aria-label="Use a session" placeholder="Use a session…" disabled={!free.length}
              options={free.map(s => ({ value: s.sessionId, label: map.sessionLabel(s) }))} value="" onValueChange={value => { if (value) map.fillWith(value, id); }} />
          </FormField>
        </>
      )}
      <Text as="p"><Text variant="strong">{view ? map.roleOf(a) : 'checking…'}</Text>{view ? ` · ${view.detail}` : ''}</Text>
      <Text as="p" variant="muted">The role is inferred from its arrows: change an arrow and it changes.</Text>
      {gives.length > 1 ? (
        <FormField label={`How it hands out work to ${gives.slice(0, -1).join(', ')} and ${gives.at(-1)}`} help={SHARE_TIP[a.share]}>
          <ToggleGroup aria-label="How it hands out work" value={a.share} onValueChange={value => map.updateAgent(id, { share: value as Share })}>
            {(Object.keys(SHARE_NAME) as Share[]).map(key => <ToggleGroupItem key={key} value={key}>{SHARE_NAME[key]}</ToggleGroupItem>)}
          </ToggleGroup>
        </FormField>
      ) : null}
      <FormField label="Notes for it (optional)" help={`Anything the arrows do not say, in your words. At most ${MAX_NOTES_BYTES} bytes.`}>
        <Textarea rows={3} value={a.notes} onChange={event => map.setNotes(id, event.target.value)} />
      </FormField>
      <FormField label="Connect to" help="This agent gives work to the one chosen.">
        <Select aria-label="Connect to" placeholder="Connect to…" disabled={full || !others.length}
          options={others.map(x => ({ value: x.id, label: x.name }))} value="" onValueChange={value => { if (value) map.connectAgents(id, value); }} />
      </FormField>
      {mine.length ? (
        <Column gap="xs">
          <Text variant="strong">Arrows</Text>
          <ItemList aria-label={`Arrows of ${a.name}`}>
            {mine.map(l => (
              <ItemRow
                key={`${l.from}>${l.to}`}
                title={`${map.nameOf(l.from)} ${KIND_NAME[l.kind]} ${map.nameOf(l.to)}`}
                onClick={() => map.selectLink(l.from, l.to)}
                leading={<Icon name="link" />}
              />
            ))}
          </ItemList>
        </Column>
      ) : null}
    </Column>
  );
}

function LinkBox({ map, from, to }: { map: TeamMap; from: string; to: string }) {
  const at = map.linkIndex(from, to);
  const l: Link = map.draft.links[at];
  const A = map.nameOf(from), B = map.nameOf(to);
  const strict = map.trials.strict ?? '';
  const pair = map.fresh() ? map.current()!.sentences[at] : undefined;
  const reverseTaken = map.draft.links.some(x => x.from === to && x.to === from);
  const kinds = ['work', 'observes'] as const;
  return (
    <Column gap="md">
      <FormField label="Kind of relation">
        <ToggleGroup aria-label="Kind of relation" value={l.kind} onValueChange={value => map.setKind(from, to, value as Link['kind'])}>
          {kinds.map(kind => <ToggleGroupItem key={kind} value={kind} disabled={kind !== l.kind && !!map.trials[kind]}>{kind === 'work' ? 'Gives work to' : 'Observes'}</ToggleGroupItem>)}
        </ToggleGroup>
      </FormField>
      {kinds.filter(kind => kind !== l.kind && map.trials[kind]).map(kind => <Note key={kind} tone="warning" title={`${A} ${KIND_NAME[kind]} ${B} cannot be set`}>{map.trials[kind]}</Note>)}
      <Text as="p" variant="muted">{KIND_TIP[l.kind](A, B)}</Text>
      {l.kind === 'work' ? (
        <FormField label={`How ${B} reports`} help={REPORTS.find(r => r[0] === l.report)?.[2]} {...(l.report !== 'strict' && strict ? { error: `Only done / blocked: ${strict}` } : {})}>
          <ToggleGroup aria-label={`How ${B} reports`} value={l.report} onValueChange={value => map.setReport(from, to, value as Link['report'])}>
            {REPORTS.map(([key, text]) => <ToggleGroupItem key={key} value={key} disabled={key === 'strict' && l.report !== 'strict' && !!strict}>{text}</ToggleGroupItem>)}
          </ToggleGroup>
        </FormField>
      ) : null}
      <FormField label="In your words (optional)">
        <Input maxLength={MAX_WORDS} value={l.words} onChange={event => map.setWords(at, event.target.value)} />
      </FormField>
      <Note tone="info" announce={false} title="What each is told">
        <Text as="p" pre>{pair ? `${A} is told: ${pair[0]}\n\n${B} is told: ${pair[1]}` : 'Checking what each is told…'}</Text>
      </Note>
      <Cluster gap="sm">
        <Button variant="outline" disabled={reverseTaken} onClick={() => map.reverse(from, to)}>Reverse</Button>
        <Button variant="outline" onClick={() => map.selectAgent(from)}>Open {A}</Button>
      </Cluster>
    </Column>
  );
}

function Boxes({ map }: { map: TeamMap }) {
  useController(map);
  const agent = map.selectedAgent();
  const sel = map.selection;
  const link = sel && 'link' in sel && map.selectedLink() >= 0 ? sel.link : null;
  if (!agent && !link) return null;
  const agentTitle = agent ? (agent.session ? 'Agent · running session' : 'Agent · no session yet') : '';
  return (
    <Inspector
      title={agent ? agentTitle : `${map.nameOf(link!.from)} → ${map.nameOf(link!.to)}`}
      open
      onOpenChange={open => { if (!open) map.select(null); }}
      footer={agent ? (
        <>
          {agent.session ? <Button variant="outline" onClick={() => map.focusSession(agent.session!)}>Show its tab</Button> : null}
          <Button variant="outline" title="The session stays open; it only leaves the team map" onClick={() => map.takeOff(agent.id)}>Take off the map</Button>
        </>
      ) : (
        <Button variant="destructive" onClick={() => map.deleteLink(link!.from, link!.to)}>Delete the arrow</Button>
      )}
    >
      {agent ? <AgentBox map={map} id={agent.id} /> : <LinkBox map={map} from={link!.from} to={link!.to} />}
    </Inspector>
  );
}

/** The inspector of the window while the team map has something chosen; the window gives it only then (`TeamController.inspecting`). */
export function TeamInspector() {
  const app = useApp();
  const team = useController(app.team);
  const map = team.shown;
  return map ? <Boxes map={map} /> : null;
}

function Actions({ map }: { map: TeamMap }) {
  useController(map);
  const bar = map.barState();
  const block = map.applyBlock();
  const open = map.lacking();
  const dirty = map.pending();
  return (
    <Cluster gap="sm" justify="end" wrap>
      <Badge tone={bar.tone === 'info' ? undefined : bar.tone}>{bar.tone === 'success' ? 'Applied' : bar.tone === 'warning' ? 'Not applied' : bar.tone === 'danger' ? 'Problem' : 'Team'}</Badge>
      <Text truncate>{bar.text}</Text>
      <Spacer />
      <Button variant="outline" disabled={!dirty || map.applying} title="Throw away what is not applied and read the team again" onClick={() => { void map.discard(); }}>Discard</Button>
      <Button disabled={!!block} loading={map.applying} loadingText="Applying…" title={block || (open ? 'Open a session for each agent that has none, then tell each concerned session its place' : 'Tell each concerned session its place')} onClick={() => { void map.apply(); }}>
        {open ? `Open ${open} session${open > 1 ? 's' : ''} and apply` : 'Apply'}
      </Button>
    </Cluster>
  );
}

/** The actions row of the window while a team map is on screen. */
export function TeamActions() {
  const app = useApp();
  const team = useController(app.team);
  const map = team.shown;
  return map && map.enabled ? <Actions map={map} /> : null;
}

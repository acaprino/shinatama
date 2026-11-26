// Settings, Models & API keys: the model each agent starts with leads the section, since it is what decides a new session; the connections
// follow (a provider endpoint, its API key, its models), each saying which agents use it so an unused one is never silent. Keys are encrypted
// for this Windows user and never come back to the page: they can be replaced or removed. Everything waits for Save.
import { FormField, Note } from '@acaprino/sigrid/components';
import { Badge, Button, Card, Checkbox, Cluster, Column, Input, Select, Table, TableBody, TableCell, TableHead, TableHeader, TableRow, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import {
  APIS, assignmentExists, connectionHost, connectionLabel, firstModelId, HARNESSES, HARNESS_NAMES, MAX_CONNECTIONS, MAX_MODELS, modelOptions, PRESETS, readChoice, usedBy, writeChoice,
  type Edit, type Harness,
} from '../../model-settings-model.ts';

const API_NAMES: Record<string, string> = { 'openai-completions': 'OpenAI Chat Completions', 'openai-responses': 'OpenAI Responses', 'anthropic-messages': 'Anthropic Messages', 'google-generative-ai': 'Google Generative AI' };
/** Agents Shinatama can launch with a shared connection today; Hermes only stores its choice. */
const LAUNCHABLE: Harness[] = ['opencode', 'pi'];
const plural = (count: number, word: string) => `${count} ${word}${count === 1 ? '' : 's'}`;
const list = (names: string[]) => names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}`;

function Assignments() {
  const app = useApp();
  const settings = useController(app.settings);
  const form = settings.models;
  return (
    <Column gap="md">
      <Text variant="strong">Model for new sessions</Text>
      {form.connections.length === 0 ? <Text variant="muted">Each agent uses its own sign-in and model until you add a connection below and choose one of its models here.</Text> : null}
      {HARNESSES.map(harness => {
        const chosen = form.assignments[harness];
        const exists = assignmentExists(form, harness);
        return (
          <FormField
            key={harness}
            label={HARNESS_NAMES[harness]}
            {...(!exists ? { error: 'That model was removed. Choose another one, or the agent’s own sign-in and model.' } : harness === 'hermes' ? { help: 'Saved only: Shinatama cannot launch Hermes yet.' } : {})}
          >
            <Select
              aria-label={`${HARNESS_NAMES[harness]} model`}
              options={modelOptions(form, harness)}
              value={chosen ? writeChoice(chosen) : ''}
              onValueChange={value => settings.assign(harness, readChoice(value))}
            />
          </FormField>
        );
      })}
    </Column>
  );
}

function ConnectionCard({ edit }: { edit: Edit }) {
  const app = useApp();
  const settings = useController(app.settings);
  const c = edit.connection;
  const open = settings.open.has(c.id);
  const users = usedBy(settings.models, c.id).map(harness => HARNESS_NAMES[harness]);
  const host = connectionHost(c.baseUrl);
  const count = c.models.filter(model => model.id.trim()).length;
  const key = edit.removeKey ? 'key removed on Save' : edit.keyInput.trim() || edit.hasApiKey ? 'API key saved' : 'no API key';
  const label = connectionLabel(edit);
  const ready = firstModelId(c) !== null;
  return (
    <Card variant="quiet" as="section" aria-label={label}>
      <Cluster gap="md" justify="between" wrap={false}>
        <Column gap="xs">
          <Text variant="strong">{label}</Text>
          <Text variant="muted">{[host || 'no base URL yet', plural(count, 'model'), key].join(' · ')}</Text>
        </Column>
        <Cluster gap="sm" wrap={false}>
          <Badge tone={users.length ? 'success' : 'warning'}>{users.length ? `Used by ${list(users)}` : 'Not used'}</Badge>
          <Button variant="outline" size="sm" aria-expanded={open} aria-label={`${open ? 'Close' : 'Edit'} ${label}`} onClick={() => settings.setConnectionOpen(c.id, !open)}>{open ? 'Close' : 'Edit'}</Button>
        </Cluster>
      </Cluster>
      {users.length === 0 ? (
        <Cluster gap="sm">
          <Text variant="muted">{ready ? 'No agent starts with this connection yet.' : 'Add a model ID, then choose which agent uses it.'}</Text>
          {ready ? LAUNCHABLE.map(harness => <Button key={harness} variant="outline" size="sm" onClick={() => settings.useFor(c.id, harness)}>Use for {HARNESS_NAMES[harness]}</Button>) : null}
        </Cluster>
      ) : null}
      {open ? (
        <Column gap="md">
          <FormField label="Base URL"><Input type="url" placeholder="https://provider.example/v1" maxLength={2048} autoComplete="off" spellCheck={false} value={c.baseUrl} onChange={event => settings.editConnection(c.id, { baseUrl: event.target.value })} /></FormField>
          <FormField label="Connection name"><Input maxLength={100} placeholder="e.g. My provider" value={c.name} onChange={event => settings.editConnection(c.id, { name: event.target.value })} /></FormField>
          <FormField label="API protocol">
            <Select aria-label="API protocol" options={APIS.map(api => ({ value: api, label: API_NAMES[api] }))} value={c.api} onValueChange={api => settings.editConnection(c.id, { api: APIS.find(one => one === api) })} />
          </FormField>
          <FormField label="API key" help={edit.removeKey ? 'Will be removed on Save' : edit.hasApiKey ? 'Saved · type to replace' : 'Optional for local servers'}>
            <Cluster gap="sm" wrap={false}>
              <Input aria-label="API key" type="password" autoComplete="new-password" maxLength={4096} value={edit.keyInput} onChange={event => settings.editConnection(c.id, { keyInput: event.target.value })} />
              {edit.hasApiKey && !edit.removeKey && !edit.keyInput ? <Button variant="outline" size="sm" onClick={() => settings.removeKey(c.id)}>Remove key</Button> : null}
            </Cluster>
          </FormField>
          <Text variant="strong">Models</Text>
          <Text variant="muted">Use the exact model IDs your provider expects. The token limits start at common values: adjust them to your model.</Text>
          <Table aria-label={`Models of ${label}`}>
            <TableHeader>
              <TableRow><TableHead>Model ID</TableHead><TableHead align="end">Context tokens</TableHead><TableHead align="end">Output tokens</TableHead><TableHead>Reasoning</TableHead><TableHead>Image input</TableHead><TableHead><Text srOnly>Remove</Text></TableHead></TableRow>
            </TableHeader>
            <TableBody>
              {c.models.map((model, index) => (
                <TableRow key={index}>
                  <TableCell><Input aria-label="Model ID" mono placeholder="e.g. qwen3-coder" maxLength={200} autoComplete="off" spellCheck={false} value={model.id} onChange={event => settings.editModel(c.id, index, { id: event.target.value })} /></TableCell>
                  <TableCell align="end"><Input aria-label="Context tokens" type="number" min={1} max={10000000} short="sm" value={String(model.contextWindow)} onChange={event => settings.editModel(c.id, index, { contextWindow: Number(event.target.value) })} /></TableCell>
                  <TableCell align="end"><Input aria-label="Output tokens" type="number" min={1} max={10000000} short="sm" value={String(model.maxTokens)} onChange={event => settings.editModel(c.id, index, { maxTokens: Number(event.target.value) })} /></TableCell>
                  <TableCell><Checkbox aria-label="Reasoning" checked={model.reasoning} onCheckedChange={on => settings.editModel(c.id, index, { reasoning: on === true })} /></TableCell>
                  <TableCell><Checkbox aria-label="Image input" checked={model.vision} onCheckedChange={on => settings.editModel(c.id, index, { vision: on === true })} /></TableCell>
                  <TableCell align="end"><Button variant="ghost" size="sm" disabled={c.models.length === 1} aria-label={`Remove model ${model.id || index + 1}`} onClick={() => settings.removeModel(c.id, index)}>Remove</Button></TableCell>
                </TableRow>
              ))}
            </TableBody>
          </Table>
          <Cluster gap="sm" justify="between">
            <Button variant="outline" size="sm" disabled={c.models.length >= MAX_MODELS} onClick={() => settings.addModel(c.id)}>Add model</Button>
            <Button variant="destructive" size="sm" onClick={() => settings.removeConnection(c.id)}>Remove connection</Button>
          </Cluster>
        </Column>
      ) : null}
    </Card>
  );
}

export function Models() {
  const app = useApp();
  const settings = useController(app.settings);
  const form = settings.models;
  return (
    <Column gap="lg">
      <Text variant="muted">Connect a provider once, then choose the model each agent starts with. Keys are encrypted for this Windows user and never shown again. Changes apply to new sessions after Save.</Text>
      {!settings.modelsLoaded && !settings.busy ? <Note tone="warning" title="The shared model settings could not be read." >Reload to try again.</Note> : null}
      <Assignments />
      <Column gap="md">
        <Text variant="strong">Connections</Text>
        <Text variant="muted">A connection is a provider endpoint, its API key and the models you use from it.</Text>
        {form.connections.length === 0 ? <Text variant="muted">No connections yet. Add your provider or a local server below, then choose its model above.</Text> : null}
        {form.connections.map(edit => <ConnectionCard key={edit.connection.id} edit={edit} />)}
        <Cluster gap="sm" align="end">
          <FormField label="New connection">
            <Select aria-label="New connection provider" options={PRESETS.map((preset, index) => ({ value: String(index), label: preset.name }))} value={String(settings.newPreset)}
              onValueChange={value => settings.setNewPreset(Number(value))} />
          </FormField>
          <Button variant="outline" disabled={form.connections.length >= MAX_CONNECTIONS || !settings.modelsLoaded} onClick={() => settings.addConnection()}>Add connection</Button>
        </Cluster>
      </Column>
    </Column>
  );
}

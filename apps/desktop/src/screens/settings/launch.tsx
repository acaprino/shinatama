// Settings, Memory, GitHub, Environment variables and Sessions. Memory, the token and the variables wait for Save; the switches of Sessions
// apply at once through the command their view always used. A saved secret never comes back to the page: it can be replaced or removed.
import { FormField, Note, SettingRow } from '@acaprino/sigrid/components';
import { Button, Checkbox, Cluster, Column, Icon, Input, Switch, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { MAX_MARGIN_MIB, MAX_VARIABLES } from '../../settings-model.ts';

export function Memory() {
  const app = useApp();
  const settings = useController(app.settings);
  const problem = settings.marginProblem;
  return (
    <>
      <SettingRow
        label="Check free memory before a session, shell or harness check starts"
        description="On: a start waits a few seconds for a fresh measurement and is refused when free RAM or allocatable memory would drop below what it needs to start plus this margin. Off: nothing waits for or is refused on memory; Windows may then page or close apps when memory runs out. Running sessions are never stopped by this setting. Applies to the next start after you save."
        control={<Switch aria-label="Check free memory" checked={settings.memoryEnabled} onCheckedChange={on => settings.setMemoryEnabled(on)} />}
      />
      <FormField label="Memory kept free (MiB)" help={`A whole number from 0 to ${MAX_MARGIN_MIB}.`} {...(problem ? { error: problem } : {})}>
        <Input type="number" short="sm" min={0} max={MAX_MARGIN_MIB} step={64} inputMode="numeric" disabled={!settings.memoryEnabled} value={settings.memoryMargin} onChange={event => settings.setMemoryMargin(event.target.value)} />
      </FormField>
    </>
  );
}

export function GitHub() {
  const app = useApp();
  const settings = useController(app.settings);
  const { launch } = settings;
  const saved = launch.hasToken && !launch.removeToken;
  return (
    <Column gap="md">
      <FormField label="Username"><Input autoComplete="off" spellCheck={false} maxLength={39} short="md" value={launch.githubUsername} onChange={event => settings.setGithubUser(event.target.value)} /></FormField>
      <FormField label="Personal access token" help={launch.hasToken && launch.removeToken ? 'Will be removed when you save' : saved ? 'Saved. Type to replace' : 'ghp_… or github_pat_…'}>
        <Cluster gap="sm" wrap={false}>
          <Input aria-label="Personal access token" type="password" autoComplete="new-password" spellCheck={false} maxLength={255} value={launch.tokenInput} onChange={event => settings.setToken(event.target.value)} />
          {saved && !launch.tokenInput ? <Button variant="outline" size="sm" onClick={() => settings.removeToken()}>Remove token</Button> : null}
        </Cluster>
      </FormField>
      <Text variant="muted">GitHub no longer accepts an account password, so use a personal access token (a fine-grained one limited to the repositories you need is safest). Agents receive it as GH_TOKEN and as the Git credential for github.com, so git push and gh never ask. Stored encrypted for this Windows user; any agent you start can read it.</Text>
    </Column>
  );
}

export function Variables() {
  const app = useApp();
  const settings = useController(app.settings);
  const { rows } = settings.launch;
  return (
    <Column gap="md">
      <Text variant="muted">Added to every new agent session. A secret value is stored encrypted and is never shown again; type a new one to replace it.</Text>
      {rows.map((row, index) => (
        <Cluster key={index} gap="sm" wrap={false} role="group" aria-label={`Variable ${row.name || index + 1}`}>
          <Input aria-label="Variable name" mono placeholder="NAME" maxLength={128} autoComplete="off" spellCheck={false} value={row.name} onChange={event => settings.setVariable(index, { name: event.target.value })} />
          <Input
            aria-label={`Value of ${row.name || 'the variable'}`} type={row.secret ? 'password' : 'text'} autoComplete="off" spellCheck={false}
            placeholder={row.hasValue && !row.touched && row.secret ? 'Saved. Type to replace' : 'value'} value={row.value}
            onChange={event => settings.setVariable(index, { value: event.target.value })}
          />
          <Checkbox label="Secret" checked={row.secret} onCheckedChange={on => settings.setVariable(index, { secret: on === true })} />
          <Button variant="ghost" size="icon-xs" aria-label={`Remove ${row.name || 'variable'}`} onClick={() => settings.removeVariable(index)}><Icon name="close" /></Button>
        </Cluster>
      ))}
      <Cluster><Button variant="outline" size="sm" disabled={rows.length >= MAX_VARIABLES} onClick={() => settings.addVariable()}>Add variable</Button></Cluster>
    </Column>
  );
}

export function Sessions() {
  const app = useApp();
  const settings = useController(app.settings);
  return (
    <>
      <SettingRow
        label="Activity titles"
        description="Sessions shows what each running session is doing, verbatim from its last tool call or message. Nothing leaves this computer."
        control={<Switch aria-label="Activity titles" checked={settings.titles === true} disabled={settings.titles === null || settings.switchesBusy} onCheckedChange={on => { void settings.setTitles(on); }} />}
      />
      <SettingRow
        label="Index session text for search"
        description="Lets Search index read your session archives. Off stops indexing; text already indexed stays searchable until you forget it there."
        control={<Switch aria-label="Index session text for search" checked={settings.indexing === true} disabled={settings.indexing === null || settings.switchesBusy} onCheckedChange={on => { void settings.setIndexing(on); }} />}
      />
      {settings.switchesLine.text ? <Note tone={settings.switchesLine.error ? 'error' : 'info'} title={settings.switchesLine.text} /> : null}
    </>
  );
}

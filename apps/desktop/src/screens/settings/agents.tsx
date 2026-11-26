// Settings, Agents: the check of an agent before it is opened (nothing is installed, signed in or sent), and whether agents start with their
// own protections. Full access waits for Save, as the launch settings always did.
import { Note, SettingRow } from '@acaprino/sigrid/components';
import { Badge, Button, Table, TableBody, TableCell, TableHead, TableHeader, TableRow, Text } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { PROVIDERS, STEP_LABELS, stepTone, type Checkup } from '../../agent-checkup.ts';
import { Switch } from '@acaprino/sigrid/primitives';

const AUTH = { authenticated: 'Signed in', unauthenticated: 'Not signed in', unknown: 'Unknown' } as const;
const State = ({ state, word }: { state: string; word: string }) => <Badge tone={stepTone(state)}>{word}</Badge>;

export function Agents() {
  const app = useApp();
  const settings = useController(app.settings);
  return (
    <>
      <Table aria-label="Agents">
        <TableHeader>
          <TableRow><TableHead>Agent</TableHead><TableHead>Program</TableHead><TableHead>Home</TableHead><TableHead>Model proxy</TableHead><TableHead>Sets</TableHead><TableHead>Sign-in</TableHead><TableHead><Text srOnly>Check</Text></TableHead></TableRow>
        </TableHeader>
        <TableBody>
          {PROVIDERS.map(provider => {
            const found = settings.checkups.get(provider.agentId);
            const checkup = found && !('error' in found) ? found as Checkup : null;
            return (
              <TableRow key={provider.agentId}>
                <TableCell><Text variant="strong">{provider.displayName}</Text></TableCell>
                {provider.checkable ? (
                  <>
                    <TableCell><State state={checkup?.program.state ?? 'unknown'} word={checkup ? checkup.program.state === 'found' ? `Found${checkup.program.version ? ` · ${checkup.program.version}` : ''}` : checkup.program.state === 'missing' ? 'Missing' : 'Unavailable' : 'Not checked'} /></TableCell>
                    {(['home', 'sidecar', 'sets'] as const).map(key => {
                      const state = checkup?.[key].state ?? 'unknown';
                      return <TableCell key={key}><State state={state} word={STEP_LABELS[state]} /></TableCell>;
                    })}
                    <TableCell><State state={checkup?.authentication ?? 'unknown'} word={checkup ? AUTH[checkup.authentication] : 'Not checked'} /></TableCell>
                  </>
                ) : (
                  <TableCell><Text variant="muted">Not qualified</Text></TableCell>
                )}
                {provider.checkable ? <></> : <><TableCell /><TableCell /><TableCell /><TableCell /></>}
                <TableCell align="end">
                  <Button variant="outline" size="sm" disabled={!provider.checkable || settings.checking !== null} loading={settings.checking === provider.agentId} loadingText="Checking…"
                    title={provider.checkable ? 'Check this agent: nothing is installed, signed in or sent' : 'This agent is not qualified yet'} aria-label={`Check ${provider.displayName}`}
                    onClick={() => { void settings.checkAgent(provider.agentId); }}>Check</Button>
                </TableCell>
              </TableRow>
            );
          })}
        </TableBody>
      </Table>
      {[...settings.checkups.values()].map(found => 'error' in found
        ? <Note key={found.error} tone="error" title={found.error} />
        : found.issues.length ? <Note key={found.agentId} tone="warning" title={`${found.displayName}: what the check found`} items={found.issues} /> : null)}
      <Text variant="muted">Check an agent before opening it. Sign-in takes place in its original terminal interface; checks do not install, sign in or send a message.</Text>
      <SettingRow
        label="Full access: start agents without their own protections"
        description="Claude starts in bypass-permissions mode, Codex without its sandbox and approvals, OpenCode allows every tool. Pi and PowerShell never asked. Applies to sessions started after you save."
        control={<Switch aria-label="Full access" checked={settings.launch.fullAccess} onCheckedChange={on => settings.setFullAccess(on)} />}
      />
      {settings.launch.fullAccess ? (
        <Note tone="warning" announce={false} title="An agent can then change or delete anything your Windows account can, inside or outside the project.">
          The private home keeps their settings and sign-ins apart; it does not confine their files.
        </Note>
      ) : null}
    </>
  );
}

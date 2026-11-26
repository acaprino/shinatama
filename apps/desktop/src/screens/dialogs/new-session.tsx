// New session: the agent (one choice), the project (a list of the recent ones, a path that can be typed, Browse…), and what the launch would
// say (a read-only review that starts nothing). Enter opens; Escape closes. It opens in the pane the person asked from, in the thread that
// pane belongs to when it has one (`OpensController.launchSession`); a refusal is said in a notice.
import { useEffect, useState, type FormEvent } from 'react';
import { Dialog, Disclosure, FormField, ItemList, ItemRow, Note } from '@acaprino/sigrid/components';
import { Button, Cluster, Column, Icon, Input, Spacer, Text, ToggleGroup, ToggleGroupItem } from '@acaprino/sigrid/primitives';
import { useApp, useController } from '../../hooks.ts';
import { reviewLaunch, type LaunchSummary } from '../../launch-review.ts';
import { errorText } from '../../api/core.ts';
import { firstHarness, rememberHarness, sortedHarnesses, startProblem } from '../../new-session-model.ts';
import { folderName, samePath } from '../../paths.ts';
import type { DialogSpec } from './spec.ts';

type Spec = Extract<DialogSpec, { kind: 'new-session' }>;

const REVIEW_WORDS: Record<LaunchSummary['status'], string> = {
  available: 'The engine would start this agent.', refused: 'The engine refuses this launch.', incomplete: 'The engine cannot finish this review yet.',
};

export function NewSession({ spec }: { spec: Spec }) {
  const app = useApp();
  const opens = useController(app.opens);
  const projects = useController(app.projects);
  const { ui } = app;
  const choices = sortedHarnesses(opens.harnesses);
  const [harness, setHarness] = useState(() => firstHarness(opens.harnesses, spec.harness));
  const [path, setPath] = useState(spec.path ?? projects.path);
  const [problem, setProblem] = useState('');
  const [browsing, setBrowsing] = useState(false);
  const [review, setReview] = useState<LaunchSummary | string | null>(null);
  // The projects it can start in: the ones the sidebar lists beside the one chosen, else the recent ones.
  const listed = opens.newSessionChoices(spec.scope, spec.path ?? projects.path);
  const folders = listed.length ? listed : projects.recent.map(project => ({ label: folderName(project.path), path: project.path }));
  useEffect(() => { void app.projects.refreshRecent(); }, [app]);
  useEffect(() => { setReview(null); }, [harness]);

  const open = () => {
    const found = startProblem(path, harness, opens.harnesses);
    if (found) { setProblem(found); return; }
    rememberHarness(harness);
    ui.closeDialog();
    void opens.launchSession(path.trim(), harness, spec.groupId ?? app.layout.focusedGroup(), spec.workset);
  };
  const browse = async () => {
    setBrowsing(true);
    const picked = await app.catalog.browse();
    setBrowsing(false);
    if (picked) { setPath(picked); setProblem(''); }
  };
  const runReview = async () => {
    setReview('Reviewing…');
    try { setReview(await reviewLaunch({ agent: harness, action: 'start', placement: { program: 'system', home: 'box' } })); }
    catch (error) { setReview(`The review could not run: ${errorText(error)}`); }
  };
  return (
    <Dialog
      open
      onOpenChange={next => { if (!next) ui.closeDialog(); }}
      title="New session"
      description={spec.workset ? 'It joins this thread.' : 'It starts in a thread of its own.'}
      width="lg"
      footer={(
        <>
          <Text variant="muted">Enter opens · Esc closes</Text>
          <Spacer />
          <Button variant="outline" onClick={() => ui.closeDialog()}>Cancel</Button>
          <Button type="submit" form="new-session-form" disabled={!choices.length}>Open</Button>
        </>
      )}
    >
      <form id="new-session-form" noValidate onSubmit={(event: FormEvent) => { event.preventDefault(); open(); }}>
        <Column gap="md">
          <FormField label="Agent">
            <ToggleGroup aria-label="Agent" value={harness} onValueChange={value => { setHarness(value); setProblem(''); }}>
              {choices.map(choice => <ToggleGroupItem key={choice.id} value={choice.id}>{choice.label}</ToggleGroupItem>)}
            </ToggleGroup>
          </FormField>
          {!choices.length ? <Note tone="warning" title="No agents are available yet." /> : null}
          <FormField label="Project" {...(problem ? { error: problem } : {})}>
            <Cluster gap="sm" wrap={false}>
              <Input aria-label="Project folder" mono autoComplete="off" spellCheck={false} maxLength={4096} placeholder="Folder path, or pick one below" value={path} onChange={event => { setPath(event.target.value); setProblem(''); }} />
              <Button type="button" variant="outline" disabled={browsing} onClick={() => { void browse(); }}><Icon name="folder" />Browse…</Button>
            </Cluster>
          </FormField>
          {folders.length ? (
            <ItemList aria-label="Recent projects">
              {folders.map(folder => (
                <ItemRow
                  key={folder.path} title={folder.label} description={folder.path} leading={<Icon name="folder" />} current={samePath(folder.path, path.trim())}
                  onClick={() => { setPath(folder.path); setProblem(''); }}
                />
              ))}
            </ItemList>
          ) : <Text variant="muted">No projects yet. Browse for a folder.</Text>}
          <Disclosure label="Launch review" summary={typeof review === 'object' && review ? review.status : undefined}>
            <Column gap="sm">
              <Text variant="muted">A read-only review by the same engine service as the command line. It starts nothing.</Text>
              <Cluster><Button type="button" variant="outline" size="sm" disabled={harness === 'powershell' || !harness} onClick={() => { void runReview(); }}>Review the launch</Button></Cluster>
              {typeof review === 'string' ? <Text variant="muted">{review}</Text> : review ? (
                <Note tone={review.status === 'available' ? 'success' : 'warning'} title={REVIEW_WORDS[review.status]} items={review.issues.map(issue => `${issue.code}${issue.reason ? `: ${issue.reason}` : ''}`)} />
              ) : null}
            </Column>
          </Disclosure>
        </Column>
      </form>
    </Dialog>
  );
}

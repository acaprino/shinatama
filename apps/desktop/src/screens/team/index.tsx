// TEAM (placeholder, Phase A). The team map of a thread (ADR-0058): who works with whom. It is the content of a tab of a pane (kind
// `team`, id `team-<thread>`) and takes the whole sheet when the tab is shown. The workspace's `renderTab` draws this component for such a
// tab. Owned by the agent that fills the team map in; the model is `team-map-model.ts`, the commands are `app.team`.
import { Empty } from '@acaprino/sigrid/components';

export function Team({ threadId }: { threadId: string }) {
  return <Empty fill title="Team map" description={`The team map of ${threadId} is written in the next phase.`} />;
}

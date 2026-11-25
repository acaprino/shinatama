// SESSIONS (placeholder, Phase A). The Sessions view: every session Shinatama can read, running or saved, in every store, with a search.
// A library with a search wants the whole sheet; the details of one session open in the inspector (`app.sessions.show`). Owned by the agent
// that fills the Sessions view in.
import { Empty } from '@acaprino/sigrid/components';

export function Sessions() {
  return <Empty fill title="Sessions" description="Every session, running or saved, is listed here in the next phase." />;
}

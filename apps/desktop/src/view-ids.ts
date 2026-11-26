// Pure: the ids of the views that belong to a thread. A thread has one team map and one conversation, each a tab of a pane, and the
// layout names them by the thread, so that a view that was saved is the same view when it is drawn again.
const TEAM = 'team-';
const CHAT = 'chat-';

export const teamViewId = (threadId: string): string => `${TEAM}${threadId}`;
export const chatViewId = (threadId: string): string => `${CHAT}${threadId}`;

/** The thread a team map view belongs to; null for any other view. */
export const threadOfTeamView = (viewId: string): string | null => viewId.startsWith(TEAM) ? viewId.slice(TEAM.length) : null;
/** The thread a conversation view belongs to; null for any other view. */
export const threadOfChatView = (viewId: string): string | null => viewId.startsWith(CHAT) ? viewId.slice(CHAT.length) : null;

// ADR-0057: what the conversation panel reads from a thread's room, checked before it is drawn.
export type ChatThread = { id: string; projectId: string };
export type ChatSession = {
  sessionId: string; harness: string | null; title: string | null; state: string;
  memberName: string | null; connection: 'in_room' | 'launched' | 'not_connected' | 'unsupported'; wakes: number | null;
};
export type ChatMember = { memberId: string; name: string; harness: string; role: string; active: boolean; sessionId: string | null; user: boolean; host: boolean };
export type ChatEvent = {
  eventId: string; timestamp: string; kind: string; actor: string | null; host: boolean; recipients: string[];
  text: string | null; signal: string | null; reason: string | null;
};
export type ChatSnapshot = { enabled: boolean; members: ChatMember[]; sessions: ChatSession[]; events: ChatEvent[] };

const MAX_EVENTS = 400;
const record = (value: unknown): value is Record<string, unknown> => !!value && typeof value === 'object' && !Array.isArray(value);
const str = (value: unknown, cap = 65536): value is string => typeof value === 'string' && value.length <= cap;
const optional = (value: unknown, cap = 65536): string | null => str(value, cap) ? value : null;
const decimal = (value: unknown): value is string => typeof value === 'string' && /^(0|[1-9][0-9]{0,18})$/.test(value);
/** Event ids are decimal strings up to 64 bits: compared as numbers without losing precision. */
export const laterEvent = (a: string, b: string) => a.length !== b.length ? a.length > b.length : a > b;

export function parseSnapshot(value: unknown): ChatSnapshot | null {
  if (!record(value) || typeof value.enabled !== 'boolean') return null;
  if (!value.enabled) return { enabled: false, members: [], sessions: [], events: [] };
  if (!Array.isArray(value.members) || !Array.isArray(value.sessions) || !Array.isArray(value.events)) return null;
  const members: ChatMember[] = [];
  for (const item of value.members.slice(0, 512)) {
    if (!record(item) || !str(item.memberId, 128) || !str(item.name, 32) || !str(item.harness, 24) || !str(item.role, 16)) continue;
    members.push({ memberId: item.memberId, name: item.name, harness: item.harness, role: item.role, active: item.active === true,
      sessionId: optional(item.sessionId, 14), user: item.user === true, host: item.host === true });
  }
  const sessions: ChatSession[] = [];
  for (const item of value.sessions.slice(0, 256)) {
    if (!record(item) || !str(item.sessionId, 14) || !str(item.state, 16)) continue;
    const connection = item.connection === 'in_room' || item.connection === 'launched' || item.connection === 'unsupported' ? item.connection : 'not_connected';
    sessions.push({ sessionId: item.sessionId, harness: optional(item.harness, 64), title: optional(item.title, 256), state: item.state,
      memberName: optional(item.memberName, 32), connection, wakes: typeof item.wakes === 'number' && Number.isSafeInteger(item.wakes) ? item.wakes : null });
  }
  const events: ChatEvent[] = [];
  for (const item of value.events.slice(-MAX_EVENTS)) {
    if (!record(item) || !decimal(item.eventId) || !str(item.kind, 16) || !str(item.timestamp, 64)) continue;
    events.push({ eventId: item.eventId, timestamp: item.timestamp, kind: item.kind, actor: optional(item.actor, 32), host: item.actorHost === true,
      recipients: Array.isArray(item.recipients) ? item.recipients.filter((name): name is string => str(name, 32)).slice(0, 64) : [],
      text: optional(item.text), signal: optional(item.signal, 32), reason: optional(item.reason, 512) });
  }
  return { enabled: true, members, sessions, events };
}

/** Adds what a later read returned; an event already held is not drawn twice. */
export function mergeEvents(held: readonly ChatEvent[], fresh: readonly ChatEvent[]): ChatEvent[] {
  const known = new Set(held.map(event => event.eventId));
  const merged = [...held, ...fresh.filter(event => !known.has(event.eventId))]
    .sort((a, b) => a.eventId === b.eventId ? 0 : laterEvent(a.eventId, b.eventId) ? 1 : -1);
  return merged.slice(-MAX_EVENTS);
}

/** One line of the conversation, as the user reads it. */
/** The member through which Shinatama tells the sessions their place in the team (ADR-0058). Its messages are notices of the team, not a person's chat line. */
export const TEAM_MEMBER = 'shinatama';
/** Whether a message is Shinatama's own: sent by a host member of the room named `shinatama`, or `shinatama-<n>` when an agent that joined by hand held the name first (that agent is no host, so its lines stay chat lines). */
export const isTeamHost = (event: Pick<ChatEvent, 'actor' | 'host'>) => event.host && event.actor !== null && /^shinatama(-\d+)?$/.test(event.actor);
/** Whether a line was written by the person at the window: `user`, or `user-<n>` when the host had to take another name. */
export const isUserLine = (event: Pick<ChatEvent, 'actor' | 'host'>) => event.actor === 'user' || (event.host && event.actor !== null && /^user-\d+$/.test(event.actor));

export function eventLine(event: ChatEvent): { who: string; what: string; kind: 'message' | 'signal' | 'presence' | 'team'; tag?: string } {
  const who = event.actor ?? 'Mukade';
  if (event.kind === 'message' && isTeamHost(event)) {
    return { who, what: event.text ?? '', kind: 'team', tag: event.recipients.length ? `to ${event.recipients.join(', ')}` : 'notice' };
  }
  if (event.kind === 'message') return { who: event.recipients.length ? `${who} → ${event.recipients.join(', ')}` : `${who} → room`, what: event.text ?? '', kind: 'message' };
  if (event.kind === 'signal') return { who, what: `signalled ${event.signal ?? '?'}${event.reason ? `: ${event.reason}` : ''}`, kind: 'signal' };
  const said: Record<string, string> = { join: 'joined the room', rejoin: 'joined the room again', leave: 'left the room', close: 'closed the room', reopen: 'reopened the room' };
  return { who, what: said[event.kind] ?? event.kind, kind: 'presence' };
}


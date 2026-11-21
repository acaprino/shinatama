import { keyedList, uiPart, uiButton, uiSelect, uiTextarea, uiOption } from '@acaprino/imago/ui';
// ADR-0057: the conversation of a thread whose sessions talk to each other. A side panel beside the
// terminals, not a dialog: the user can watch the sessions work while they read. It reads the room
// without consuming anything a session has not read, and writes only as the user's own member.
import { confirmsField } from './desktop-shell-model.ts';
import { eventLine, isUserLine, mergeEvents, parseSnapshot, type ChatEvent, type ChatSession, type ChatSnapshot, type ChatThread } from './thread-chat-model.ts';
export { laterEvent } from './thread-chat-model.ts';
export type { ChatThread } from './thread-chat-model.ts';

type Options = {
  invoke: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  notify(message: unknown, tone?: 'info' | 'error'): void;
  title(thread: ChatThread): string;
  tone(thread: ChatThread): string | null;
  harnessLabel(harness: string): string;
  /** Shows the session's tab, turning its thread on if needed. Starts nothing. */
  focusSession(sessionId: string): void;
  /** The latest event the user has now seen in this thread's room, and its time. */
  seen(threadId: string, eventId: string, timestamp: string): void;
  restoreFocus?: () => void;
};

const POLL_MS = 3000;
const MAX_TEXT = 16 * 1024;
const message = (error: unknown) => typeof error === 'string' ? error : (error as { message?: string } | null)?.message ?? 'The conversation could not be read.';

const CONNECTION_TEXT: Record<ChatSession['connection'], string> = {
  in_room: 'in the room',
  launched: 'launched with the room\'s kit: it joins as it starts, under the name Mukade gives it (see the room\'s members)',
  not_connected: 'not in the room yet: it joins at its first turn when opened or resumed after the thread started talking',
  unsupported: 'cannot take part: Shinatama gives this agent no Mukade tools',
};

export function createThreadChat(options: Options) {
  const panel = uiPart('aside', undefined, 'im-docked-inspector'); panel.id = 'thread-chat-panel'; panel.hidden = true; panel.setAttribute('aria-labelledby', 'thread-chat-title');
  const heading = uiPart('row');
  const title = uiPart('title', 'Conversation'); title.id = 'thread-chat-title'; title.tabIndex = -1;
  const close = uiButton('×'); close.setAttribute('aria-label', 'Close the conversation');
  heading.append(title, close);
  const thread = uiPart('paragraph');
  const status = uiPart('status'); status.setAttribute('role', 'status');
  const sessionsHead = uiPart('heading', 'Sessions');
  const sessions = uiPart('list', undefined, 'im-summary-list'); sessions.setAttribute('aria-label', 'Sessions of the thread');
  const logHead = uiPart('row');
  const logTitle = uiPart('heading', 'Messages');
  const filterLabel = uiPart('label'); const filter = uiSelect(); filter.id = 'thread-chat-filter';
  for (const [value, text] of [['all', 'Everything'], ['messages', 'Messages only']] as const) { const option = uiOption(value, text); filter.append(option); }
  filterLabel.append('Show ', filter);
  logHead.append(logTitle, filterLabel);
  const log = uiPart('orderedList', undefined, 'im-scroll-log'); log.setAttribute('aria-label', 'Conversation of the thread'); log.tabIndex = 0;
  const empty = uiPart('paragraph', 'Nothing written yet. Ask a session to talk to another one, or write to them below.');
  const form = uiPart('form');
  const toLabel = uiPart('label', 'To'); toLabel.htmlFor = 'thread-chat-to';
  const to = uiSelect(); to.id = 'thread-chat-to';
  const textLabel = uiPart('label', 'Message'); textLabel.htmlFor = 'thread-chat-text';
  const text = uiTextarea(); text.id = 'thread-chat-text'; text.rows = 3; text.maxLength = MAX_TEXT;
  text.placeholder = 'Write to a session. Refer to files by path.';
  const hint = uiPart('hint', 'You write as "user". A session reads it at once if it is waiting after a turn, else at its next turn. Ctrl+Enter sends.');
  hint.id = 'thread-chat-hint'; text.setAttribute('aria-describedby', hint.id);
  const send = uiButton('Send', { primary: true }); send.type = 'submit';
  const actions = uiPart('actions'); actions.append(send);
  form.append(toLabel, to, textLabel, text, hint, actions);
  panel.append(heading, thread, status, sessionsHead, sessions, logHead, log, empty, form);
  (document.querySelector('#work-area') ?? document.querySelector('#app'))?.append(panel);

  let current: ChatThread | null = null; let snapshot: ChatSnapshot | null = null; let events: ChatEvent[] = [];
  let timer = 0; let reading: number | null = null; let sending = false; let generation = 0; let toChosen = false;
  let lastSeenEvent: string | null = null;

  const paintText = (element: HTMLElement, value: string) => {
    if (element.textContent !== value) element.textContent = value;
  };
  function setStatus(value: string, tone: 'info' | 'error' = 'info') {
    paintText(status, value); status.dataset.tone = tone; status.classList.toggle('im-text-err', tone === 'error');
  }
  const atBottom = () => log.scrollHeight - log.scrollTop - log.clientHeight < 24;
  const paintSessions = keyedList<ChatSession>(sessions, session => session.sessionId, session => {
    const element = uiPart('item', undefined, 'im-stack');
    const name = uiPart('strong');
    const member = uiPart('text');
    const state = uiPart('text', undefined, 'im-muted');
    const show = uiButton('Show'); show.title = "Show this session's tab";
    show.onclick = () => options.focusSession(session.sessionId);
    element.append(name, member, state, show);
    return { element, update(row) {
      const agent = row.harness ? options.harnessLabel(row.harness) : 'Session';
      paintText(name, row.title && row.title !== agent ? `${row.title} · ${agent}` : agent);
      paintText(member, row.memberName ? '@' + row.memberName : '');
      const wakes = row.wakes ? ` · woken ${row.wakes}×` : '';
      paintText(state, `${row.state.replace('_', ' ')} · ${CONNECTION_TEXT[row.connection]}${wakes}`);
      if (element.dataset.connection !== row.connection) element.dataset.connection = row.connection;
    } };
  });
  const paintRecipients = keyedList<[string, string]>(to, ([value]) => value, ([value, label]) => {
    const element = uiOption(value, label);
    return { element, update: ([, nextLabel]) => paintText(element, nextLabel) };
  });

  function renderSessions() {
    const rows = snapshot?.sessions ?? [];
    paintSessions(rows);
    // The recipients: the sessions' members first, then any other member of the room.
    const chosen = to.value;
    const names = new Map<string, string>();
    for (const session of rows) if (session.memberName) names.set(session.memberName, `${session.memberName} · ${session.harness ? options.harnessLabel(session.harness) : 'session'}`);
    for (const member of snapshot?.members ?? []) if (member.active && !member.user && !names.has(member.name)) names.set(member.name, member.name);
    const choices: [string, string][] = [...names.entries(), ['', 'Whole room (wakes no one)']];
    paintRecipients(choices);
    // The first session is the default; a choice the user made is kept while it is offered.
    const selected = toChosen && (chosen === '' || names.has(chosen)) ? chosen : choices[0][0];
    if (to.value !== selected) to.value = selected;
  }
  const paintEvents = keyedList<ChatEvent>(log, event => event.eventId, event => {
    const line = eventLine(event); const item = uiPart('item', undefined, 'thread-chat-event'); item.dataset.kind = line.kind;
    const time = uiPart('time'); time.dateTime = event.timestamp;
    const at = new Date(event.timestamp); time.textContent = Number.isNaN(at.valueOf()) ? '' : at.toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' });
    const who = uiPart('text', line.who); if (isUserLine(event)) who.dataset.user = '';
    const what = uiPart('paragraph', line.what, 'im-wrap-text');
    if (line.tag) { const tag = uiPart('text', line.tag, 'thread-chat-tag im-chip'); who.append(' ', tag); }
    item.append(time, who, what); return { element: item };
  });
  function renderLog() {
    const keep = atBottom();
    const shown = filter.value === 'messages' ? events.filter(event => event.kind === 'message') : events;
    paintEvents(shown);
    empty.hidden = shown.length > 0;
    // New lines scroll into view only for a reader already at the end.
    if (keep) log.scrollTop = log.scrollHeight;
  }
  function render() {
    if (!current) return;
    paintText(thread, options.title(current));
    const tone = options.tone(current); if (tone) panel.style.setProperty('--im-item-color', tone); else panel.style.removeProperty('--im-item-color');
    renderSessions(); renderLog();
    send.disabled = sending || !snapshot?.enabled; to.disabled = text.disabled = !snapshot?.enabled;
  }

  async function read() {
    if (!current || reading === generation || document.hidden) return;
    const at = generation; const scope = current; reading = at;
    try {
      const since = events.at(-1)?.timestamp ?? null;
      const value = parseSnapshot(await options.invoke<unknown>('thread_chat_snapshot', { request: { worksetId: scope.id, since } }));
      if (at !== generation) return;
      if (!value) { setStatus('The conversation could not be read.', 'error'); return; }
      snapshot = value;
      if (!value.enabled) { events = []; setStatus('This thread\'s sessions do not talk to each other. Turn it on in the thread\'s settings (⋯).'); render(); return; }
      events = mergeEvents(events, value.events);
      setStatus('');
      render();
      const latest = events.at(-1);
      if (latest && latest.eventId !== lastSeenEvent) {
        lastSeenEvent = latest.eventId;
        options.seen(scope.id, latest.eventId, latest.timestamp);
      }
    } catch (error) {
      if (at === generation) setStatus(message(error), 'error');
    } finally { if (reading === at) reading = null; }
  }
  function schedule() {
    window.clearInterval(timer);
    timer = window.setInterval(() => { void read(); }, POLL_MS);
  }

  form.onsubmit = event => {
    event.preventDefault();
    const scope = current; const body = text.value;
    if (!scope || sending || !body.trim()) return;
    if (new TextEncoder().encode(body).length > MAX_TEXT) { setStatus('Write a message of at most 16 KB.', 'error'); return; }
    sending = true; render();
    void (async () => {
      try {
        await options.invoke('thread_chat_send', { request: { worksetId: scope.id, recipients: to.value ? [to.value] : [], text: body } });
        if (current === scope) { text.value = ''; setStatus(to.value ? `Sent to ${to.value}.` : 'Sent to the room.'); }
        await read();
      } catch (error) { if (current === scope) setStatus(message(error), 'error'); }
      finally { sending = false; render(); if (current === scope) text.focus(); }
    })();
  };
  text.onkeydown = event => {
    // Ctrl+Enter sends; Enter alone is a new line.
    if (confirmsField(event) && (event.ctrlKey || event.metaKey)) { event.preventDefault(); form.requestSubmit(); }
  };
  filter.onchange = renderLog;
  to.onchange = () => { toChosen = true; };
  close.onclick = () => hide();
  panel.onkeydown = event => { if (event.key === 'Escape' && !event.defaultPrevented) { event.preventDefault(); hide(); } };
  const visibility = () => { if (!document.hidden && current) void read(); };
  document.addEventListener('visibilitychange', visibility);

  function clearRoom() {
    snapshot = null; events = []; toChosen = false; lastSeenEvent = null;
    paintSessions([]); paintRecipients([]); paintEvents([]);
  }
  function hide() {
    if (panel.hidden) return;
    generation++; current = null; clearRoom(); window.clearInterval(timer);
    panel.hidden = true; options.restoreFocus?.();
  }
  return {
    element: panel,
    /** Opens the conversation of this thread, or brings it forward if it is already open. */
    open(scope: ChatThread) {
      if (current?.id !== scope.id) { generation++; current = { ...scope }; clearRoom(); setStatus('Reading the room…'); }
      panel.hidden = false; render(); title.focus(); schedule(); void read();
    },
    hide,
    current: (): string | null => current?.id ?? null,
    /** The thread's name or colour changed. */
    repaint: render,
    destroy() { hide(); document.removeEventListener('visibilitychange', visibility); panel.remove(); },
  };
}

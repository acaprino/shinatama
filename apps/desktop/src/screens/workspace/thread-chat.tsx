// ADR-0057: the conversation of a thread whose sessions talk to each other, a tab beside the terminals (the user watches the sessions work
// while reading). It reads the room without consuming anything a session has not read, and writes only as the user's own member. The room
// is read by `app.chat` while this tab is on screen; this file draws it: the sessions of the thread, the lines of the room, and the field
// a message is written in.
import { useEffect, useState } from 'react';
import { Disclosure, Empty, MarkdownBlocks, Message, Note } from '@acaprino/sigrid/components';
import { Avatar, Button, Cluster, Column, Heading, Select, Text } from '@acaprino/sigrid/primitives';
import { Conversation, PromptInput, type WorkspaceTabState } from '@acaprino/sigrid/assemblies';
import { useApp, useController } from '../../hooks.ts';
import {
  chosenRecipient, clockText, eventLine, isUserLine, recipientChoices, sessionLine, visibleEvents, type RoomFilter,
} from '../../thread-chat-model.ts';

/** The select cannot hold an empty value: the whole room is this one. */
const ROOM = '*room*';
const FILTERS: { value: RoomFilter; label: string }[] = [{ value: 'all', label: 'Everything' }, { value: 'messages', label: 'Messages only' }];

export function ThreadChat({ threadId, state }: { threadId: string; state: WorkspaceTabState }) {
  const app = useApp();
  const chat = useController(app.chat);
  const threads = useController(app.threads);
  const shown = useController(app.ui).view === 'workspace';
  const [text, setText] = useState('');
  // The room is read while this tab is on screen, and not otherwise.
  useEffect(() => (state.visible && shown ? chat.watch(threadId) : undefined), [chat, threadId, state.visible, shown]);

  const room = chat.room(threadId);
  const label = (harness: string) => threads.harnessLabel(harness);
  const choices = recipientChoices(room.snapshot, label);
  const to = chosenRecipient(choices, room.to);
  const lines = visibleEvents(room.events, room.filter);
  const enabled = room.snapshot?.enabled === true;
  const sessions = room.snapshot?.sessions ?? [];
  const title = threads.title(threadId) ?? 'Thread';

  if (room.failure) {
    return (
      <Empty
        fill
        title="The conversation could not be read"
        description={room.failure}
        action={<Button onClick={() => chat.retryRoom(threadId)}>Retry</Button>}
      />
    );
  }
  return (
    <Column fill gap="sm" pad="sm">
      <Cluster justify="between">
        <Heading size="small" truncate>{title}</Heading>
        <Cluster>
          <Text variant="muted">{`${sessions.length} session${sessions.length === 1 ? '' : 's'} in the room`}</Text>
          <Select
            size="sm"
            aria-label="Show"
            options={FILTERS}
            value={room.filter}
            onValueChange={value => chat.setFilter(threadId, value as RoomFilter)}
          />
        </Cluster>
      </Cluster>
      {room.status.text ? (room.status.error
        ? <Note tone="error" title={room.status.text} />
        : <Text variant="muted" role="status">{room.status.text}</Text>) : null}
      {sessions.length ? (
        <Disclosure label="Sessions of the thread" summary={String(sessions.length)}>
          <Column gap="xs" as="ul">
            {sessions.map(session => {
              const line = sessionLine(session, label);
              return (
                <Cluster key={session.sessionId} as="li" justify="between">
                  <Column gap="xs">
                    <Text variant="strong">{line.name}</Text>
                    <Text variant="muted">{`${line.member} ${line.state}`.trim()}</Text>
                  </Column>
                  <Button size="sm" variant="outline" aria-label={`Show ${line.name}`} onClick={() => { void threads.focusThreadSession(session.sessionId); }}>Show</Button>
                </Cluster>
              );
            })}
          </Column>
        </Disclosure>
      ) : null}
      {/* The thread is a part that fills: it takes the height the other parts leave, and scrolls inside it. */}
      <Conversation
        label="Conversation of the thread"
        empty={<Empty title="Nothing written yet" description="Ask a session to talk to another one, or write to them below." />}
      >
        {lines.map(event => {
          const line = eventLine(event);
          const user = isUserLine(event);
          const time = clockText(event.timestamp);
          if (line.kind === 'presence' || line.kind === 'signal') {
            return <Text key={event.eventId} variant="muted" as="p">{`${time} ${line.who} ${line.what}`}</Text>;
          }
          return (
            <Message
              key={event.eventId}
              from={user ? 'user' : 'assistant'}
              name={line.tag ? `${line.who} · ${line.tag}` : line.who}
              avatar={user ? undefined : <Avatar name={line.who} size="xs" decorative />}
              time={time}
            >
              <Cluster gap="xs">
                <Text variant="strong" aria-hidden>{line.who}</Text>
                {line.tag ? <Text variant="muted" aria-hidden>{line.tag}</Text> : null}
              </Cluster>
              <MarkdownBlocks text={line.what} />
            </Message>
          );
        })}
      </Conversation>
      <PromptInput
        label="Message"
        placeholder="Write to a session. Refer to files by path."
        value={text}
        onValueChange={setText}
        disabled={!enabled}
        busy={room.sending}
        maxLength={16000}
        tools={(
          <Select
            size="sm"
            aria-label="To"
            options={choices.map(choice => ({ value: choice.value || ROOM, label: choice.label }))}
            value={to || ROOM}
            onValueChange={value => chat.chooseRecipient(threadId, value === ROOM ? '' : value)}
          />
        )}
        // The field empties itself when a message is sent; one that did not go comes back.
        onSubmit={message => { void chat.send(threadId, to, message).then(sent => { if (!sent) setText(message); }); }}
      />
    </Column>
  );
}

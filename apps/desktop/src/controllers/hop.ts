// The hop of a session to another agent (C28): opens the destination, pastes the exact previewed seed once it is ready for input, and
// waits for it to say `ready <nonce>`. The preview is the screen's (`sessionsApi.hopPreview`); this runs what the person confirmed.
import { sessionsApi } from '../api/sessions.ts';
import type { SessionInfo } from '../api/terminals.ts';
import type { Shinatama } from '../app.ts';
import { sleep } from '../api/core.ts';

/** What the hop says about itself: the id Rust gave it, the destination's name and the seed's nonce. */
export interface HopPreview { hop_id: string; destination: string }

/** The folder question of an agent ("Do you trust this folder?"): answered by the person in the new pane, the hop continues after it. */
const TRUST = /trust (this|the files in this) folder|do you trust/i;

export class HopController {
  private readonly app: Shinatama;

  constructor(app: Shinatama) {
    this.app = app;
  }

  /** Runs the hop. `progress` says what it waits for; `release` is called when the person must answer in the destination's pane (the
   *  hop's dialog gives the pane back). Resolves to what to tell the person; throws what went wrong. */
  async run(preview: HopPreview, progress: (message: string) => void, release: () => void): Promise<string> {
    const { terminals, ui, layout } = this.app;
    progress(`Opening ${preview.destination}…`);
    let launched: { hopId: string; nonce: string; started: { session: SessionInfo; threadId: string } } | undefined;
    for (let attempt = 0; !launched; attempt++) {
      try { launched = await sessionsApi.hopLaunch({ hopId: preview.hop_id, rows: 30, cols: 110 }) as typeof launched; }
      catch (failure) {
        const text = typeof failure === 'string' ? failure : JSON.stringify(failure ?? '');
        if (!/still finishing|not yet ready|waiting for healthy resource|Retry when Resources/i.test(text) || attempt >= 30) throw text;
        await sleep(2000);
      }
    }
    await this.app.opens.attachSession(launched!.started.session, launched!.started.threadId);
    const id = launched!.started.session.id;
    const session = terminals.get(id);
    if (!session) throw 'The destination pane could not be attached.';
    progress('Waiting for the destination to be ready for input…');
    let last = '';
    let stableSince = Date.now();
    let released = false;
    for (const deadline = Date.now() + 120000; ; await sleep(250)) {
      if (session.ended) throw 'The destination exited before the seed was sent; the source is untouched.';
      const now = terminals.screenText(id);
      if (TRUST.test(now)) {
        if (!released) { released = true; release(); ui.info(`Answer ${preview.destination}'s folder question in the new pane; the hop continues after it.`); }
        stableSince = Date.now();
        last = now;
        continue;
      }
      if (now !== last) { last = now; stableSince = Date.now(); }
      if (now.trim() && Date.now() - stableSince > 2000) break;
      if (Date.now() > deadline) {
        await sessionsApi.hopFinish(launched!.hopId, 'fail');
        throw 'The destination never became ready for input; the hop failed and its pane stays open.';
      }
    }
    await sessionsApi.hopDeliver(launched!.hopId);
    progress('Seed sent. Waiting for the destination to confirm…');
    const receipt = new RegExp(`(^|[^\`])ready ${launched!.nonce}(?!\`)`);
    const seen = () => terminals.screenText(id).split('\n').some(line => !/reply exactly/i.test(line) && receipt.test(line));
    for (const deadline = Date.now() + 90000; Date.now() < deadline && !session.ended; await sleep(500)) {
      if (seen()) {
        await sessionsApi.hopFinish(launched!.hopId, 'receipt');
        const pane = layout.groupOf(id);
        if (pane) layout.focus(pane.id);
        return `Hop committed: ${preview.destination} confirmed the hand-off. The source session is untouched.`;
      }
    }
    const keep = !session.ended && await ui.ask({
      title: 'No confirmation from the destination',
      description: `${preview.destination} did not reply "ready ${launched!.nonce}" within 90 seconds. Keep it as the new session anyway, or mark the hop failed? Its pane stays open either way.`,
      confirm: 'Keep', danger: false,
    });
    await sessionsApi.hopFinish(launched!.hopId, keep ? 'accept_without_receipt' : 'fail');
    return keep ? 'Hop committed without confirmation.' : 'Hop marked failed; the destination pane stays open.';
  }
}

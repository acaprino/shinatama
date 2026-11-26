// What the window measures of itself and reports once a second, so that the backend can hold back optional work and new starts when the
// machine is short of resources: the worst delay of an input and the longest frame of the last second. Window measurements only: no keys,
// no terminal bytes, no paths, no document contents. The reading is DOM (events and frames); what is done with it is `app.terminals`.
import { useEffect } from 'react';
import type { Shinatama } from '../../app.ts';

const micros = (milliseconds: number) => Math.round(milliseconds * 1000);
const INPUTS = ['keydown', 'pointerdown', 'wheel'] as const;

/** Reports for as long as the window is shown; one request in flight, so a stuck one never makes more. */
export function useWindowTelemetry(app: Shinatama): void {
  useEffect(() => {
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let animation = 0;
    let previousFrame: number | null = null;
    let framePeakUs: number | null = null;
    let inputPeakUs: number | null = null;
    const input = (event: Event) => {
      const delay = performance.now() - event.timeStamp;
      if (Number.isFinite(delay) && delay >= 0) inputPeakUs = Math.max(inputPeakUs ?? 0, micros(delay));
    };
    const frame = (at: number) => {
      if (stopped) return;
      if (document.hidden) { previousFrame = null; framePeakUs = null; }
      else {
        if (previousFrame !== null) framePeakUs = Math.max(framePeakUs ?? 0, micros(at - previousFrame));
        previousFrame = at;
      }
      animation = requestAnimationFrame(frame);
    };
    const visibility = () => { previousFrame = null; framePeakUs = null; inputPeakUs = null; };
    const report = async () => {
      if (stopped) return;
      if (!document.hidden) {
        const sample = { input_latency_us: inputPeakUs, frame_time_us: framePeakUs, pending_input_events: null, pending_output_bytes: null, queued_launches: null } as const;
        inputPeakUs = null;
        framePeakUs = null;
        await app.terminals.reportTelemetry(sample);
      }
      if (!stopped) timer = setTimeout(() => { void report(); }, 1000);
    };
    for (const name of INPUTS) document.addEventListener(name, input, { capture: true, passive: true });
    document.addEventListener('visibilitychange', visibility);
    animation = requestAnimationFrame(frame);
    timer = setTimeout(() => { void report(); }, 1000);
    return () => {
      stopped = true;
      clearTimeout(timer);
      cancelAnimationFrame(animation);
      document.removeEventListener('visibilitychange', visibility);
      for (const name of INPUTS) document.removeEventListener(name, input, true);
    };
  }, [app]);
}

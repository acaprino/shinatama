import { invoke } from '@tauri-apps/api/core';

let initialized = false;

/** Window measurements only: no keys, terminal bytes, paths or document contents. */
export function initializeResourceTelemetry() {
  if (initialized) return;
  initialized = true;
  let stopped = false;
  let timer = 0;
  let animation = 0;
  let previousFrame: number | null = null;
  let framePeakUs: number | null = null;
  let inputPeakUs: number | null = null;
  let previousRoundtripUs: number | null = null;
  const micros = (milliseconds: number) => Math.round(milliseconds * 1000);

  const input = (event: Event) => {
    const delay = performance.now() - event.timeStamp;
    if (Number.isFinite(delay) && delay >= 0) {
      inputPeakUs = Math.max(inputPeakUs ?? 0, micros(delay));
    }
  };
  const frame = (at: number) => {
    if (stopped) return;
    if (document.hidden) {
      previousFrame = null;
      framePeakUs = null;
    } else {
      if (previousFrame !== null) framePeakUs = Math.max(framePeakUs ?? 0, micros(at - previousFrame));
      previousFrame = at;
    }
    animation = requestAnimationFrame(frame);
  };
  const visibility = () => {
    previousFrame = null;
    framePeakUs = null;
    inputPeakUs = null;
  };
  const report = async () => {
    if (stopped) return;
    if (!document.hidden) {
      const started = performance.now();
      const telemetry = {
        input_latency_us: inputPeakUs,
        frame_time_us: framePeakUs,
        ipc_roundtrip_us: previousRoundtripUs,
        // Native/terminal queues are instrumented by their owning component.
        pending_input_events: null,
        pending_output_bytes: null,
        queued_launches: null,
      };
      inputPeakUs = null;
      framePeakUs = null;
      try {
        await invoke<boolean>('resources_telemetry', { telemetry });
        previousRoundtripUs = micros(performance.now() - started);
      } catch { previousRoundtripUs = null; }
    }
    // One request in flight. A stuck request cannot create more timers or IPC work.
    if (!stopped) timer = window.setTimeout(() => { void report(); }, 1000);
  };
  for (const name of ['keydown', 'pointerdown', 'wheel']) {
    document.addEventListener(name, input, { capture: true, passive: true });
  }
  document.addEventListener('visibilitychange', visibility);
  window.addEventListener('pagehide', () => {
    stopped = true;
    clearTimeout(timer);
    cancelAnimationFrame(animation);
    document.removeEventListener('visibilitychange', visibility);
    for (const name of ['keydown', 'pointerdown', 'wheel']) document.removeEventListener(name, input, true);
  }, { once: true });
  animation = requestAnimationFrame(frame);
  timer = window.setTimeout(() => { void report(); }, 1000);
}

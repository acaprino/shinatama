import './resource-status.css';

type Policy = {
  stage: 'normal' | 'reduce_optional' | 'defer_starts';
  reason: string;
  reduce_optional: boolean;
  native_launch_enabled: boolean;
};
export type ResourceTelemetryReply = {
  accepted: boolean;
  policy: Policy | null;
  resources: {
    owned_private_commit_bytes: number | null;
    system_remaining_commit_bytes: number | null;
    observed_processes: number | null;
  };
};

const memory = (value: number | null) => value === null ? 'unavailable' : `${Math.round(value / 1024 / 1024)} MiB`;

/** A compact observation, not a claim that unsupported native launches are safe. */
export function renderResourceStatus(reply: ResourceTelemetryReply) {
  if (!reply?.resources) return;
  const anchor = document.querySelector('#runtime-status');
  if (!anchor) return;
  let badge = document.querySelector<HTMLSpanElement>('#resource-status');
  if (!badge) {
    badge = document.createElement('span');
    badge.id = 'resource-status';
    badge.className = 'resource-health';
    badge.setAttribute('role', 'status');
    anchor.after(badge);
  }
  const policy = reply.policy;
  const checking = !policy || ['missing_telemetry', 'stale_telemetry', 'partial_cpu_coverage'].includes(policy.reason);
  const label = checking ? 'Resources: checking' : policy.stage === 'normal' ? 'Resources: available' : 'Resources: reduced activity';
  if (badge.textContent !== label) badge.textContent = label;
  badge.dataset.stage = policy?.stage ?? 'defer_starts';
  badge.title = [
    `Observed process memory: ${memory(reply.resources.owned_private_commit_bytes)}`,
    `System commit headroom: ${memory(reply.resources.system_remaining_commit_bytes)}`,
    `Observed processes: ${reply.resources.observed_processes ?? 'unavailable'}`,
    `Optional background work: ${policy?.reduce_optional ? 'paused' : checking ? 'waiting for measurements' : 'available'}`,
    `Native agent launches: ${policy?.native_launch_enabled ? 'available' : 'unavailable'}`,
  ].join('\n');
}

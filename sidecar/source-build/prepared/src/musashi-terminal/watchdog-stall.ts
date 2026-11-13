export interface OwnerWatchdogOptions {
    /** Owner pids; the watchdog removes the ones it finds dead. */
    watchers: Set<number>;
    /** Last heartbeat per heartbeat-opted owner. */
    lastSeen: Map<number, number>;
    initialWatcherPid: number;
    isPidAlive: (pid: number) => boolean;
    isDead: (alive: boolean, lastSeen: number | undefined, nowMs: number, heartbeatGraceMs: number) => boolean;
    intervalMs: number;
    heartbeatGraceMs: number;
    idleGraceMs: number;
}

/** The part of the gap since the previous tick that the watchdog could not
 * observe because this process's own event loop was held. Scheduler skew up to
 * one interval is ordinary and not credited. */
export function unobservedStallMs(previousTickMs: number | undefined, nowMs: number, intervalMs: number): number {
    if (previousTickMs === undefined) return 0;
    const late = nowMs - previousTickMs - intervalMs;
    return late > intervalMs ? late : 0;
}

/** One owner-watchdog tick per call. Returns the shutdown reason once every
 * owner has been gone for the idle grace, otherwise undefined.
 *
 * Heartbeat silence and the idle grace are measured on the wall clock, but
 * while this process's own loop is held no heartbeat can be read: that time is
 * not evidence that an owner went silent, and it is credited back. On
 * 1 November 2026 a shared proxy held its loop for about 36 s, then counted all
 * six live Claude Code owners dead on its first tick and shut down with 324
 * connections open; every session received ECONNREFUSED. */
export function createOwnerWatchdog(options: OwnerWatchdogOptions): (nowMs: number) => string | undefined {
    let idleSince: number | null = null;
    let lastDead: number[] = [];
    let lastStaleHb: number[] = [];
    let previousTick: number | undefined;
    return (tickNow) => {
        const stallMs = unobservedStallMs(previousTick, tickNow, options.intervalMs);
        previousTick = tickNow;
        if (stallMs > 0) {
            for (const [pid, seen] of options.lastSeen) options.lastSeen.set(pid, seen + stallMs);
            if (idleSince !== null) idleSince += stallMs;
        }
        const dead: number[] = [];
        const staleByHeartbeat: number[] = [];
        for (const pid of options.watchers) {
            const alive = options.isPidAlive(pid);
            // #3544: heartbeat-opted owners ALSO die by liveness-proof
            // expiry — kill(pid,0) is an existence check, identity-blind
            // under Windows pid reuse. Legacy pids (no heartbeat on record)
            // keep kill-0-only semantics.
            if (options.isDead(alive, options.lastSeen.get(pid), tickNow, options.heartbeatGraceMs)) {
                options.watchers.delete(pid);
                options.lastSeen.delete(pid);
                dead.push(pid);
                if (alive) staleByHeartbeat.push(pid);
            }
        }
        if (dead.length > 0) lastDead = dead;
        if (staleByHeartbeat.length > 0) lastStaleHb = staleByHeartbeat; else lastStaleHb = [];
        if (options.watchers.size > 0) {
            idleSince = null;
            return undefined;
        }
        if (idleSince === null) {
            idleSince = tickNow;
            return undefined;
        }
        if (tickNow - idleSince < options.idleGraceMs) return undefined;
        let reason = lastDead.length === 1 && lastDead[0] === options.initialWatcherPid ? `parent-gone (pid ${lastDead[0]})` : `watchers-gone (pids: ${lastDead.join(", ")})`;
        if (lastStaleHb.length > 0) reason += ` [stale-heartbeat pids: ${lastStaleHb.join(", ")}]`;
        return reason;
    };
}

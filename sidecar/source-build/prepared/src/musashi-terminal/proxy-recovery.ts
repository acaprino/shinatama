interface RecoveryChild {
    pid: number;
    on?: (event: string, listener: (...args: unknown[]) => void) => unknown;
    off?: (event: string, listener: (...args: unknown[]) => void) => unknown;
}

export interface RecoveryHandle {
    origin: string;
    port: number;
    instanceId?: string;
    child?: RecoveryChild;
    attached?: boolean;
    logPath?: string;
    refusedWatcher?: boolean;
}

export interface RecoveryHealth {
    ok: boolean;
    pid?: number;
    instanceId?: string;
    watchdog?: { armed: boolean; watchers?: number[] };
}

interface RecoveryCallbacks {
    probe: (origin: string) => Promise<RecoveryHealth | undefined>;
    recover: () => Promise<RecoveryHandle>;
    registerOwner: (origin: string) => Promise<"ok" | "refused" | "failed">;
    ownerPid: number;
    report: (message: string) => void;
    /** Whether the endpoint still accepts TCP connections. A listener that
     * misses health probes is busy (its event loop is held, for example by a
     * large compression), not dead: a replacement could not bind its port and
     * would only lose the bind. Absent or failing, the probe alone decides. */
    listening?: (origin: string) => Promise<boolean>;
    /** Whether some process holds the port, from a bind attempt: true when it
     * fails with EADDRINUSE, false when it succeeds, undefined when it cannot
     * tell. It is asked before `listening` because it takes no slot in the
     * listener's accept queue. On Windows a listener whose loop is held
     * refuses new connections once 232 wait to be accepted (Node 24,
     * measured 2026-11-01, unchanged with backlog 2048), so a TCP check reads
     * a busy proxy as gone and each probe connection fills the queue further. */
    portHeld?: (port: number) => Promise<boolean | undefined>;
    /** How long a busy spell lasts before it is reported (default 15 s). */
    busyNoticeMs?: number;
    /** Longest interval between probes of a busy proxy (default 8 intervals).
     * Each probe that times out leaves a connection in the accept queue. */
    busyMaxDelayMs?: number;
    intervalMs?: number;
    maxRetryDelayMs?: number;
}

/** Keep the endpoint already baked into a running harness available. Startup
 * coordination and process ownership stay with the launcher; this monitor never
 * kills a listener just because its health request timed out, and never starts
 * a replacement beside a listener that still holds the port. */
export function startProxyRecovery(handle: RecoveryHandle, callbacks: RecoveryCallbacks): { stop: () => Promise<void> } {
    const intervalMs = Math.max(1, callbacks.intervalMs ?? 1_000);
    const maxRetryDelayMs = Math.max(intervalMs, callbacks.maxRetryDelayMs ?? 30_000);
    const busyNoticeMs = Math.max(0, callbacks.busyNoticeMs ?? 15_000);
    const busyMaxDelayMs = Math.max(intervalMs, callbacks.busyMaxDelayMs ?? intervalMs * 8);
    const origin = handle.origin;
    const port = handle.port;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    let inFlight: Promise<void> | undefined;
    let watchedChild: RecoveryChild | undefined;
    let childExit: ((...args: unknown[]) => void) | undefined;
    let urgent = false;
    let failedProbes = 0;
    let recoveryFailures = 0;
    let reportedRetryDelay: number | undefined;
    let registeredIdentity: string | undefined;
    let registrationIdentity: string | undefined;
    let refusedIdentity: string | undefined;
    let registrationFailures = 0;
    let registrationRetryAt = 0;
    let registrationFailureReported = false;
    let busySince: number | undefined;
    let busyReported = false;
    let busyChecks = 0;
    let instanceId = handle.instanceId;
    let proxyPid = handle.child?.pid;

    const retryDelay = (failures: number): number => Math.min(maxRetryDelayMs, intervalMs * 2 ** Math.min(failures, 30));

    const stillListening = async (): Promise<boolean> => {
        if (callbacks.portHeld) {
            let held: boolean | undefined;
            try {
                held = await callbacks.portHeld(port);
            } catch {
                held = undefined;
            }
            if (held !== undefined) return held;
        }
        if (!callbacks.listening) return false;
        try {
            return await callbacks.listening(origin);
        } catch {
            return false;
        }
    };

    const busySeconds = (): number => Math.round((Date.now() - (busySince ?? Date.now())) / 1000);

    const detachChild = (): void => {
        if (watchedChild && childExit) watchedChild.off?.("exit", childExit);
        watchedChild = undefined;
        childExit = undefined;
    };

    const followChild = (): void => {
        detachChild();
        if (stopped || handle.attached || !handle.child) return;
        const child = handle.child;
        watchedChild = child;
        childExit = () => {
            if (stopped || watchedChild !== child) return;
            detachChild();
            urgent = true;
            wake();
        };
        child.on?.("exit", childExit);
    };

    const adopt = (replacement: RecoveryHandle, health?: RecoveryHealth): void => {
        if (replacement.origin !== origin || replacement.port !== port) {
            throw new Error(`refusing recovery at ${replacement.origin}: the running harness is pinned to ${origin}`);
        }
        detachChild();
        // A shared replacement may have no child/log/refusal fields. Keeping
        // those from the dead process would give shutdown stale ownership.
        delete handle.child;
        delete handle.attached;
        delete handle.logPath;
        delete handle.refusedWatcher;
        delete handle.instanceId;
        Object.assign(handle, replacement);
        instanceId = replacement.instanceId ?? health?.instanceId;
        proxyPid = replacement.child?.pid ?? health?.pid;
        urgent = false;
        followChild();
    };

    const renewOwner = async (health: RecoveryHealth): Promise<void> => {
        const identity = `${health.instanceId ?? ""}:${health.pid ?? ""}`;
        if (!health.watchdog?.armed || refusedIdentity === identity) return;
        const watchers = health.watchdog.watchers;
        if (watchers?.includes(callbacks.ownerPid)) {
            registeredIdentity = identity;
            registrationFailures = 0;
            registrationRetryAt = 0;
            registrationFailureReported = false;
            return;
        }
        if (watchers === undefined && registeredIdentity === identity) return;
        // Probe every interval, but let repeated registration transport errors
        // back off as well: the default registration callback also logs them.
        if (identity === registrationIdentity && Date.now() < registrationRetryAt) return;
        if (identity !== registrationIdentity) {
            registrationIdentity = identity;
            registrationFailures = 0;
            registrationRetryAt = 0;
            registrationFailureReported = false;
        }
        let result: "ok" | "refused" | "failed";
        try {
            result = await callbacks.registerOwner(origin);
        } catch {
            result = "failed";
        }
        if (result === "refused") {
            refusedIdentity = identity;
            handle.refusedWatcher = true;
        } else if (result === "ok") {
            registeredIdentity = identity;
            delete handle.refusedWatcher;
            registrationFailures = 0;
            registrationRetryAt = 0;
            registrationFailureReported = false;
        } else {
            registrationRetryAt = Date.now() + retryDelay(registrationFailures++);
            if (!registrationFailureReported && !stopped) {
                callbacks.report(`bili: could not renew proxy ownership at ${origin} — retrying automatically`);
                registrationFailureReported = true;
            }
        }
    };

    const check = async (): Promise<number> => {
        const observedExit = urgent;
        urgent = false;
        let health: RecoveryHealth | undefined;
        try {
            health = await callbacks.probe(origin);
        } catch {
            health = undefined;
        }
        if (stopped) return intervalMs;
        const changedIdentity = health?.ok && (
            (instanceId !== undefined && health.instanceId !== undefined && instanceId !== health.instanceId) ||
            (proxyPid !== undefined && health.pid !== undefined && proxyPid !== health.pid)
        );
        if (health?.ok && !changedIdentity) {
            instanceId ??= health.instanceId;
            proxyPid ??= health.pid;
            failedProbes = 0;
            recoveryFailures = 0;
            if (reportedRetryDelay !== undefined) {
                callbacks.report(`bili: proxy at ${origin} is available again — existing sessions can continue`);
                reportedRetryDelay = undefined;
            }
            if (busyReported) {
                callbacks.report(`bili: proxy at ${origin} is answering again after ${busySeconds()}s busy — no restart was needed`);
            }
            busySince = undefined;
            busyReported = false;
            busyChecks = 0;
            await renewOwner(health);
            return intervalMs;
        }
        if (!changedIdentity && !observedExit && await stillListening()) {
            if (stopped) return intervalMs;
            busySince ??= Date.now();
            failedProbes = 0;
            if (!busyReported && Date.now() - busySince >= busyNoticeMs) {
                callbacks.report(`bili: proxy at ${origin} still holds its port but has not answered health checks for ${busySeconds()}s — it is busy, not dead; waiting instead of starting a duplicate`);
                busyReported = true;
            }
            return Math.min(busyMaxDelayMs, intervalMs * 2 ** Math.min(busyChecks++, 30));
        }
        busySince = undefined;
        busyReported = false;
        busyChecks = 0;
        failedProbes += 1;
        if (!changedIdentity && !observedExit && failedProbes < 2) return intervalMs;
        try {
            const replacement = await callbacks.recover();
            // Adopt even when stop arrived during bring-up: the launcher's
            // cleanup must see and release the newly spawned process.
            // A different healthy responder might belong to another lane.
            // recover() validates the pinned endpoint's code/config before
            // permitting it to take over this session's traffic or watchdog.
            adopt(replacement, health?.ok ? health : undefined);
            failedProbes = 0;
            recoveryFailures = 0;
            reportedRetryDelay = undefined;
            if (!stopped) callbacks.report(`bili: proxy restored at ${origin} — existing sessions can continue`);
            return intervalMs;
        } catch (error) {
            const delayMs = retryDelay(recoveryFailures++);
            if (!stopped && reportedRetryDelay !== delayMs) {
                const reason = error instanceof Error ? error.message : String(error);
                callbacks.report(`bili: proxy recovery at ${origin} failed (${reason}) — retrying in ${delayMs}ms`);
                reportedRetryDelay = delayMs;
            }
            return delayMs;
        }
    };

    const schedule = (delayMs: number): void => {
        if (stopped) return;
        timer = setTimeout(wake, delayMs);
        timer.unref?.();
    };

    function wake(): void {
        if (stopped) return;
        if (timer !== undefined) clearTimeout(timer);
        timer = undefined;
        if (inFlight) return;
        let delayMs = intervalMs;
        inFlight = check().then((nextDelay) => { delayMs = nextDelay; }).finally(() => {
            inFlight = undefined;
            schedule(urgent ? 0 : delayMs);
        });
    }

    followChild();
    schedule(intervalMs);
    return {
        async stop(): Promise<void> {
            stopped = true;
            if (timer !== undefined) clearTimeout(timer);
            timer = undefined;
            detachChild();
            await inFlight;
        },
    };
}

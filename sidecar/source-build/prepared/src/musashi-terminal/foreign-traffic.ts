import { isCodexClient } from "../codex-compact.js";
import { hostIdForLog } from "../log-mask.js";

type Headers = Record<string, string | string[] | undefined>;

/** The client a request identifies as, when it is one Musashi Terminal launches over
 *  the inherited HTTPS interception: Claude Code (its session header or CLI
 *  user agent), Codex (its user agent) or an agent carrying the injected
 *  BillionContext plugin (OpenCode, Pi), which stamps `x-bili-plugin`.
 *  Generic session or affinity headers are not enough: scripts started from a
 *  session inherit HTTPS_PROXY and send OpenCode-like headers too. */
export function launchedClientSignal(headers: Headers): "claude" | "codex" | "plugin" | undefined {
    const value = (name: string): string | undefined => {
        const raw = headers[name];
        const first = Array.isArray(raw) ? raw[0] : raw;
        return typeof first === "string" && first.trim().length > 0 ? first : undefined;
    };
    if (value("x-bili-plugin") !== undefined) return "plugin";
    if (value("x-claude-code-session-id") !== undefined) return "claude";
    if (/^claude-(?:cli|code)\//i.test(value("user-agent") ?? "")) return "claude";
    if (isCodexClient(headers)) return "codex";
    return undefined;
}

/** A conversation request that reached the proxy through the MITM tunnel
 *  without a launched-client signal. It is relayed byte for byte: the proxy
 *  serves the clients it launched, not every process that inherited their
 *  proxy environment. Requests routed to the proxy explicitly (`/bili/`, base
 *  URLs) are deliberate and stay processed. */
export function isForeignTraffic(mitmRoute: boolean, conversation: boolean, headers: Headers): boolean {
    return mitmRoute && conversation && launchedClientSignal(headers) === undefined;
}

const reportedHosts = new Set<string>();

/** One line per upstream host for the life of the proxy. */
export function reportForeignTraffic(upstream: string, headers: Headers, log: (level: string, msg: string) => void): void {
    const host = hostIdForLog(upstream);
    if (reportedHosts.has(host)) return;
    reportedHosts.add(host);
    const ua = headers["user-agent"];
    const agent = (Array.isArray(ua) ? ua[0] : ua) ?? "-";
    log("info", `[foreign-traffic] ${host}: requests without a launched-client signal are relayed verbatim (first user agent ${JSON.stringify(agent.slice(0, 80))}); no session, tools or notes are added`);
}

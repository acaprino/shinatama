import path from "node:path";
import { launcherEnv } from "./musashi-terminal/legacy-names.js";

/** Canonical BillionContext state belongs to this launcher's private sidecar. */
export function privateRoot(env: NodeJS.ProcessEnv = process.env): string {
    const root = launcherEnv("BILI_ROOT", env);
    if (!root || !path.isAbsolute(root)) throw new Error("MUSASHI_TERMINAL_BILI_ROOT must be absolute");
    return path.resolve(root);
}

/** Root config dir: user-editable configuration lives here. */
export function configDir(): string {
    return path.join(privateRoot(), "config");
}

/** Main config file path. */
export function configFile(): string {
    const env = process.env.BILI_CONFIG_FILE;
    if (env && env.length > 0) return path.resolve(env);
    return path.join(configDir(), "billion-context.json");
}

/** Root data dir: persistent session state lives here. */
export function dataDir(): string {
    return path.join(privateRoot(), "data");
}

/** Sessions dir: one JSON file per session. */
export function sessionsDir(): string {
    const env = process.env.BILI_SESSIONS_DIR;
    if (env && env.length > 0) return path.resolve(env);
    return path.join(dataDir(), "sessions");
}

/** Root cache dir: transient/ephemeral data (update-check throttle, etc.). */
export function cacheDir(): string {
    return path.join(privateRoot(), "cache");
}

/** Root state dir: log files and other per-host state. */
export function stateDir(): string {
    return path.join(privateRoot(), "state");
}

/** Default log file path. */
export function defaultLogFile(): string {
    return path.join(stateDir(), "bili.log");
}

/** Last-detected routing bypass for the claude lane (#2290): written by the
 *  SessionStart hook when the session environment indicates the managed
 *  ANTHROPIC_BASE_URL will not be honored (e.g. Claude Desktop overrides it),
 *  read by `bili doctor` to surface the finding after the fact. */
export function claudeRoutingWarningFile(): string {
    return path.join(stateDir(), "claude-routing-warning.json");
}
/** Body-dump dir (ACP_DUMP_BODY / BILI_DUMP_4XX): ACP_DUMP_DIR override first,
 *  else the XDG state dir so dumps co-locate with bili.log on every platform. */
export function dumpsDir(): string {
    const env = process.env.ACP_DUMP_DIR;
    if (env && env.length > 0) return env;
    return path.join(stateDir(), "dumps");
}

/** CA dir: root CA + dynamically-signed host certificates for MITM mode.
 *  Lives under the data dir (the private key is sensitive but regenerable).
 *  Created lazily by ca.ts on first MITM use. */
export function caDir(): string {
    return path.join(dataDir(), "ca");
}

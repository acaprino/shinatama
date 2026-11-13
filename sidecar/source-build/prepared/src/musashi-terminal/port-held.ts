import net from "node:net";

/** Whether some process holds a TCP port on the proxy's host, decided by a bind
 * attempt: true on EADDRINUSE, false when the bind succeeds (it is released at
 * once), undefined on any other outcome. Unlike a connection it takes no slot
 * in the holder's accept queue, and it still answers when that queue is full
 * and connections are refused (Windows, measured 2026-11-01). */
export function portHeld(host: string, port: number): Promise<boolean | undefined> {
    return new Promise((resolve) => {
        const server = net.createServer();
        server.unref();
        server.once("error", (error: NodeJS.ErrnoException) => resolve(error.code === "EADDRINUSE" ? true : undefined));
        server.listen({ host, port, exclusive: true }, () => server.close(() => resolve(false)));
    });
}

/** The bind host for an origin's listener: a loopback name resolves to the
 * IPv4 loopback the proxy binds. */
export function bindHostOf(origin: string): string | undefined {
    try {
        const host = new URL(origin).hostname.replace(/^\[|\]$/g, "");
        return host === "localhost" ? "127.0.0.1" : host;
    } catch {
        return undefined;
    }
}

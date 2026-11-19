import { ProjectionMemo } from "acp-kernel/wire";

// The WeakMap does not keep retired sessions alive. At most four independent
// 32 MiB projection stores survive; evicted/restarted sessions simply go cold.
const sessionKeys = new WeakMap<object, number>();
const stores = new Map<number, ProjectionMemo>();
let sequence = 0;

export function projectionMemoFor(session: object): ProjectionMemo {
    let key = sessionKeys.get(session);
    if (key === undefined) { key = ++sequence; sessionKeys.set(session, key); }
    let memo = stores.get(key);
    if (memo) stores.delete(key);
    else memo = new ProjectionMemo();
    stores.set(key, memo);
    while (stores.size > 4) stores.delete(stores.keys().next().value!);
    return memo;
}

// Optional upstream engines are omitted from the private runtime. Without jimp, egress image
// rescaling degrades to pass-through; without mysql2, the MySQL storage backend is unavailable
// and the default SQLite store is unaffected. Both stay external, lazily imported modules.
declare module "jimp";
declare module "mysql2/promise" {
    export type Pool = any;
    export type PoolConnection = any;
    export type PoolOptions = any;
    export function createPool(options: string | PoolOptions): Pool;
}

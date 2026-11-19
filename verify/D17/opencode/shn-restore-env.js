// D17 candidate: an OpenCode 2.x local plugin (dropped into <private config>/plugins/) that gives the programs
// OpenCode's shell tool starts the user's own XDG_* back, while OpenCode itself keeps its private XDG_* stores.
// For each XDG_* name: if SHN_USER_<name> is set (the user's original value, passed by the launcher) it is restored,
// otherwise the variable is removed (the user had none). Hook: shell "create.before" (packages/core/src/shell.ts).
const NAMES = ["XDG_DATA_HOME", "XDG_CONFIG_HOME", "XDG_STATE_HOME", "XDG_CACHE_HOME"];
export default {
  id: "shinatama.restore-user-env",
  setup(ctx) {
    ctx.shell.hook("create.before", (invocation) => {
      for (const name of NAMES) {
        const original = invocation.env[`SHN_USER_${name}`];
        if (original !== undefined) invocation.env[name] = original;
        else delete invocation.env[name];
      }
    });
  },
};

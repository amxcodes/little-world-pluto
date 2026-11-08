// A geometry engine owns mutable caches. Never cook two revisions concurrently.
// Intermediate queued revisions are superseded; the newest request always runs.
export function createSceneBuildQueue(build, publish) {
  let running = false;
  let pending = null;
  let latest = null;
  return async function enqueue(request) {
    pending = request;
    latest = request;
    if (running) return;
    running = true;
    try {
      while (pending) {
        const current = pending;
        pending = null;
        let result;
        try { result = await build(current); }
        catch (error) { result = { error: String(error?.message || error).slice(0, 2000) }; }
        if (latest === current) publish({ ...result, id: current.id });
      }
    } finally { running = false; }
  };
}

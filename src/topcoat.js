// Registers server-rendered tools on the page's model context. Each `execute`
// POSTs `{ name, arguments }` to the app's endpoint and hands back the JSON
// `ToolResult` it answers with. Invoked by `src/topcoat.rs` as
// `(<this>)(endpoint, tools)`.
((endpoint, tools) => {
  const mc = document.modelContext ?? navigator.modelContext;
  if (typeof mc?.registerTool !== "function") return;
  // A re-run (an HTMX/Alpine swap re-inserting this script) replaces the
  // previous set instead of colliding with it on every name.
  globalThis.__webmcpTopcoat?.abort();
  const page = (globalThis.__webmcpTopcoat = new AbortController());
  // `execute` must always resolve: a rejection reaches the agent as a bare
  // `UnknownError` with the reason thrown away.
  const fail = (text) => ({ content: [{ type: "text", text }], isError: true });
  for (const { exposedTo, ...tool } of tools) {
    const execute = async (args, options) => {
      try {
        const res = await fetch(endpoint, {
          method: "POST",
          credentials: "same-origin",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ name: tool.name, arguments: args ?? {} }),
          signal: options?.signal,
        });
        if (!res.ok) return fail(`${tool.name} failed: HTTP ${res.status}`);
        return await res.json();
      } catch (e) {
        return fail(e?.name === "AbortError" ? "cancelled" : String(e));
      }
    };
    const warn = (e) =>
      console.warn(`webmcp: registerTool("${tool.name}") was rejected:`, e);
    try {
      Promise.resolve(
        mc.registerTool(
          { ...tool, execute },
          { signal: page.signal, ...(exposedTo && { exposedTo }) },
        ),
      ).catch(warn);
    } catch (e) {
      warn(e);
    }
  }
})

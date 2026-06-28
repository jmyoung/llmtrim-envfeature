// Placeholder dashboard loader. Step C replaces this with the real UI.
// Loaded as an external script because the CSP (script-src 'self') blocks
// inline scripts; `withGlobalTauri` exposes the API on `window.__TAURI__`.
const out = document.getElementById("output");

async function refresh() {
  try {
    const { invoke } = window.__TAURI__.core;
    const dash = await invoke("get_dashboard");
    out.className = "";
    out.textContent = JSON.stringify(dash, null, 2);
  } catch (e) {
    out.className = "error";
    out.textContent = "Error: " + (e?.message ?? String(e));
  }
}

refresh();

// Listen for server-push events from the poll loop.
window.__TAURI__.event.listen("dashboard", (ev) => {
  out.className = "";
  out.textContent = JSON.stringify(ev.payload, null, 2);
});

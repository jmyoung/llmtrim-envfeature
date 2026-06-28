import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

import "./styles.css";
import type { Dashboard } from "./types.js";
import { el } from "./dom.js";
import { agentCard } from "./card.js";
import { formatBill, formatPct } from "./format.js";

// Poll-interval choices (seconds). The label is built with Intl so the unit
// reads naturally in the system locale.
const POLL_OPTIONS = [10, 30, 60, 120];

// Declared before the shell is built: `intervalLabel` (called while populating
// the interval <select> below) closes over `num`, so it must already exist.
const num = new Intl.NumberFormat(undefined);

// ---------------------------------------------------------------------------
// App shell — built once; data updates mutate referenced nodes in place so the
// per-second countdown never re-renders the card list.
// ---------------------------------------------------------------------------

const app = document.getElementById("app");
if (!app) throw new Error("missing #app root");

const heroPct = el("span", { class: "hero-pct", "aria-live": "polite" }, ["—"]);
const heroSub = el("span", { class: "hero-sub" }, ["of input trimmed"]);
const mark = el("span", { class: "mark", "aria-hidden": "true" });
const wordmark = el("span", { class: "wordmark" }, ["llmtrim"]);
const brand = el("div", { class: "brand" }, [mark, wordmark]);
const header = el("header", { class: "header" }, [
  brand,
  el("div", { class: "hero" }, [heroPct, heroSub]),
]);

const list = el("main", { class: "list" });

const countdown = el("span", { class: "countdown", "aria-live": "polite" }, ["…"]);

const intervalSelect = el("select", {
  class: "interval",
  "aria-label": "refresh interval",
}) as HTMLSelectElement;
for (const secs of POLL_OPTIONS) {
  const opt = el("option", { value: String(secs) }, [intervalLabel(secs)]);
  intervalSelect.append(opt);
}
intervalSelect.value = "30";
intervalSelect.addEventListener("change", onIntervalChange);

const quitBtn = el("button", { class: "quit", type: "button" }, ["Quit"]);
quitBtn.addEventListener("click", () => void invoke("quit"));

const footer = el("footer", { class: "footer" }, [
  countdown,
  el("div", { class: "footer-controls" }, [intervalSelect, quitBtn]),
]);

app.append(header, list, footer);

// ---------------------------------------------------------------------------
// Countdown — drives "Next update in Ns" from `next_update_secs`.
// ---------------------------------------------------------------------------

let remaining = 30;
const tickId = window.setInterval(() => {
  remaining = Math.max(0, remaining - 1);
  renderCountdown();
}, 1000);
window.addEventListener("beforeunload", () => window.clearInterval(tickId));

function renderCountdown(): void {
  countdown.textContent =
    remaining > 0 ? `Next update in ${num.format(remaining)}s` : "Updating…";
}

function intervalLabel(secs: number): string {
  return `Every ${num.format(secs)}s`;
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

function applyDashboard(d: Dashboard): void {
  heroPct.textContent = formatPct(d.totals.saved_pct, d.cards.length > 0);
  heroSub.textContent =
    d.totals.bill_micros > 0
      ? `trimmed · ${formatBill(d.totals.bill_micros)} billed`
      : "of input trimmed";

  list.replaceChildren();
  list.classList.remove("list-centered");

  if (d.cards.length === 0) {
    list.classList.add("list-centered");
    list.append(
      stateBlock(
        "No activity yet",
        "Start the llmtrim proxy to see per-agent savings here.",
      ),
    );
  } else {
    d.cards.forEach((card) => list.append(agentCard(card)));
  }

  remaining = d.next_update_secs;
  renderCountdown();
  if (String(d.next_update_secs) !== intervalSelect.value) {
    // Keep the control in sync if the backend interval differs (e.g. first load).
    const match = POLL_OPTIONS.includes(d.next_update_secs);
    if (match) intervalSelect.value = String(d.next_update_secs);
  }
}

function showError(message: string): void {
  heroPct.textContent = "—";
  heroSub.textContent = "no data";
  list.replaceChildren();
  list.classList.add("list-centered");
  list.append(stateBlock("Can't load savings", message, true));
}

function stateBlock(title: string, body: string, isError = false): HTMLElement {
  const icon = el("span", { class: "state-icon", "aria-hidden": "true" });
  icon.append(isError ? warningIcon() : activityIcon());
  return el("div", { class: isError ? "state state-error" : "state" }, [
    icon,
    el("span", { class: "state-title" }, [title]),
    el("span", { class: "state-body" }, [body]),
  ]);
}

const SVG_NS = "http://www.w3.org/2000/svg";

function svgIcon(paths: string[]): SVGSVGElement {
  const svg = document.createElementNS(SVG_NS, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", "18");
  svg.setAttribute("height", "18");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  for (const d of paths) {
    const p = document.createElementNS(SVG_NS, "path");
    p.setAttribute("d", d);
    svg.appendChild(p);
  }
  return svg;
}

// Activity line — the "no data yet" state.
function activityIcon(): SVGSVGElement {
  return svgIcon(["M3 12h4l3 8 4-16 3 8h4"]);
}

// Warning triangle — the error state.
function warningIcon(): SVGSVGElement {
  return svgIcon(["M12 3 2 20h20L12 3z", "M12 10v4", "M12 17.5v.5"]);
}

// ---------------------------------------------------------------------------
// IPC
// ---------------------------------------------------------------------------

async function refresh(): Promise<void> {
  try {
    const d = await invoke<Dashboard>("get_dashboard");
    applyDashboard(d);
  } catch (e) {
    showError(errorMessage(e));
  }
}

async function onIntervalChange(): Promise<void> {
  const secs = Number(intervalSelect.value);
  try {
    await invoke("set_poll_interval", { secs });
    await refresh();
  } catch (e) {
    showError(errorMessage(e));
  }
}

function errorMessage(e: unknown): string {
  // Tauri command rejections arrive as the sanitised string from the Rust side.
  if (typeof e === "string") return e;
  if (e instanceof Error) return e.message;
  return "Unexpected error.";
}

// Initial load + server-push subscription from the background poll loop.
void refresh();
void listen<Dashboard>("dashboard", (ev) => applyDashboard(ev.payload));

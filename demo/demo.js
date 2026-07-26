// Almanac interactive demo — a faithful 1:1 mirror of the Rust
// almanac-bridge rendering: RRULE, STATUS, emoji SUMMARY prefix,
// RELATED-TO;RELTYPE=DEPENDS-ON, and the lineage check engine.
// Runs entirely in the browser; no backend.

"use strict";

// ---------- model (mirrors crates/almanac-bridge/src/model.rs) ----------
const STATUS = {
  pending:   { emoji: "🟡", ical: "TENTATIVE",  label: "Pending",   css: "ev-pend" },
  running:   { emoji: "⏳", ical: "TENTATIVE",  label: "Running",    css: "ev-run"  },
  succeeded: { emoji: "✅", ical: "CONFIRMED",  label: "Succeeded", css: "ev-ok"   },
  failed:    { emoji: "❌", ical: "CANCELLED",   label: "Failed",    css: "ev-bad"  },
  skipped:   { emoji: "⏸",  ical: "CANCELLED",   label: "Skipped",   css: "ev-bad"  },
};

const DEFAULT_STATE = {
  schedules: [
    {
      id: "daily-brief", summary: "Daily research brief",
      description: "Produces a research brief every morning.",
      rrule: "FREQ=DAILY;BYHOUR=9", group: "research",
      produces: "research-brief", status: "succeeded",
    },
    {
      id: "weekly-strategy", summary: "Weekly strategy draft",
      description: "Reads the week's research briefs. Mondays.",
      rrule: "FREQ=WEEKLY;BYDAY=MO;BYHOUR=10", group: "strategy",
      consumes: { schema: "research-brief", minVersion: 2 }, status: "pending",
    },
    {
      id: "nightly-index", summary: "Nightly vector index rebuild",
      description: "Rebuilds the search index.",
      rrule: "FREQ=DAILY;BYHOUR=2", group: "infra",
      status: "failed",
    },
    {
      id: "pr-review", summary: "On PR merge: review summary",
      description: "Webhook-triggered one-off.",
      rrule: "", group: "code", status: "succeeded", webhook: true,
    },
  ],
  // lineage inputs:
  manifestPresent: true,
  versionOk: true,
};

let state = load() || clone(DEFAULT_STATE);

function clone(o) { return JSON.parse(JSON.stringify(o)); }
function load() {
  try { const s = localStorage.getItem("almanac-demo"); return s ? JSON.parse(s) : null; }
  catch { return null; }
}
function save() { try { localStorage.setItem("almanac-demo", JSON.stringify(state)); } catch {} }

// ---------- lineage check (mirrors lineage/check.rs) ----------
function checkLineage(schedule) {
  if (!schedule.consumes) return null;
  const { schema, minVersion } = schedule.consumes;
  if (!state.manifestPresent) return { schema, state: "Missing" };
  if (!state.versionOk) return { schema, state: "VersionMismatch", found: 1, need: minVersion };
  return { schema, state: "Ready", version: 3 };
}

function depMarker(dep) {
  if (!dep) return "";
  const cls = dep.state === "Ready" ? "ok" : dep.state === "Missing" ? "bad" : "warn";
  const txt =
    dep.state === "Ready" ? `✅ ${dep.schema} (v3, materialized)` :
    dep.state === "Missing" ? `❌ ${dep.schema} (no manifest in freshness window)` :
    `⚠️ ${dep.schema} (v1 found, need v${dep.need}+)`;
  return `<div class="dep-note"><span class="${cls}">${txt}</span></div>`;
}

// ---------- ICS rendering (mirrors ical/event.rs + feed.rs) ----------
function escapeIcs(s) {
  return s.replace(/\\/g, "\\\\").replace(/;/g, "\\;").replace(/,/g, "\\,").replace(/\n/g, "\\n");
}
// RFC 5545 line folding at 75 octets.
function fold(line) {
  if (line.length <= 75) return line;
  const out = [];
  let first = true;
  let rest = line;
  while (rest.length > 0) {
    const n = first ? 75 : 74;
    out.push((first ? "" : " ") + rest.slice(0, n));
    rest = rest.slice(n);
    first = false;
  }
  return out.join("\r\n");
}
function prop(key, val) { return fold(`${key}:${val}`); }

function nowIso() {
  // Use a fixed base for deterministic demo output.
  const base = new Date(Date.UTC(2026, 6, 25, 12, 0, 0));
  return base.toISOString().replace(/[-:]/g, "").replace(/\.\d{3}Z$/, "Z");
}

function renderVevent(s) {
  const st = STATUS[s.status];
  const uid = `${s.id}@almanac`;
  let summary = s.summary;
  let status = "TENTATIVE";
  let description = s.description;

  if (s.status !== "pending" || s.consumes) {
    // overlay run status
    summary = `${st.emoji} ${s.summary}`;
    status = st.ical;
    description += `\nRun: ${st.label}.`;
  }

  // lineage
  const dep = checkLineage(s);
  let relatedTo = "";
  if (dep && s.consumes) {
    // weekly-strategy consumes research-brief produced by daily-brief
    relatedTo = fold(`RELATED-TO;RELTYPE=DEPENDS-ON:daily-brief@almanac`);
    const mark =
      dep.state === "Ready" ? `✅ ${dep.schema} (v3, materialized)` :
      dep.state === "Missing" ? `❌ ${dep.schema} (no manifest in freshness window)` :
      `⚠️ ${dep.schema} (v1 found, need v${dep.need}+)`;
    description += `\n\nDependencies:\n${mark}`;
  }

  const lines = [
    "BEGIN:VEVENT",
    prop("UID", uid),
    prop("SUMMARY", escapeIcs(summary)),
    prop("DESCRIPTION", escapeIcs(description)),
    prop("DTSTART", "20260725T090000Z"),
    prop("DTEND", "20260725T093000Z"),
    prop("DTSTAMP", nowIso()),
    prop("STATUS", status),
    prop("CATEGORIES", escapeIcs(`almanac,${s.group}`)),
  ];
  if (s.rrule) lines.push(prop("RRULE", s.rrule));
  if (relatedTo) lines.push(relatedTo);
  lines.push("END:VEVENT");
  return lines.join("\r\n");
}

function renderFeed() {
  const cal = [
    "BEGIN:VCALENDAR",
    "VERSION:2.0",
    "PRODID:-//almanac//demo//EN",
    "CALSCALE:GREGORIAN",
    prop("X-WR-CALNAME", "demo"),
    prop("X-WR-CALDESC", "Agent schedules and artifact lineage — rendered by Almanac."),
    prop("X-WR-TIMEZONE", "UTC"),
  ];
  for (const s of state.schedules) cal.push(renderVevent(s));
  cal.push("END:VCALENDAR");
  return cal.join("\r\n");
}

// ---------- UI rendering ----------
function renderControls() {
  const list = document.getElementById("schedule-list");
  list.innerHTML = "";
  for (const s of state.schedules) {
    const row = document.createElement("div");
    row.className = "sched-row";
    const pills = Object.keys(STATUS).map((k) => {
      const active = s.status === k ? "active" : "";
      return `<button class="pill ${active}" data-sched="${s.id}" data-status="${k}"><span class="em">${STATUS[k].emoji}</span>${STATUS[k].label}</button>`;
    }).join("");
    row.innerHTML = `
      <div class="name">${s.summary}</div>
      <div class="rrule">${s.rrule || "(webhook — one-off)"}</div>
      <div class="status-pills">${pills}</div>
    `;
    list.appendChild(row);
  }
  list.querySelectorAll(".pill").forEach((btn) => {
    btn.addEventListener("click", () => {
      const sid = btn.getAttribute("data-sched");
      const st = btn.getAttribute("data-status");
      const sched = state.schedules.find((x) => x.id === sid);
      if (sched) {
        sched.status = st;
        // Auto-sync lineage toggles for convenience: if daily-brief succeeded,
        // assume a manifest exists; otherwise it doesn't.
        if (sid === "daily-brief") {
          state.manifestPresent = st === "succeeded" || st === "running";
        }
        save();
        renderAll();
      }
    });
  });

  document.getElementById("manifest-present").checked = state.manifestPresent;
  document.getElementById("version-ok").checked = state.versionOk;
}

function renderCalendar() {
  const el = document.getElementById("calendar-render");
  el.innerHTML = "";
  let count = 0;
  for (const s of state.schedules) {
    const st = STATUS[s.status];
    const dep = checkLineage(s);
    const ev = document.createElement("div");
    ev.className = `cal-event ${st.css}`;
    ev.innerHTML = `
      <div class="emoji">${st.emoji}</div>
      <div class="body">
        <div class="summary">${s.summary}</div>
        <div class="meta">${s.rrule || "webhook"} &middot; ${s.group}</div>
        ${depMarker(dep)}
      </div>
      <div class="status-tag">${st.ical}</div>
    `;
    el.appendChild(ev);
    count++;
  }
  document.getElementById("event-count").textContent = `${count} events`;
}

function renderIcs() {
  document.getElementById("ics-output").textContent = renderFeed();
}

function renderAll() {
  renderControls();
  renderCalendar();
  renderIcs();
}

// ---------- wiring ----------
document.getElementById("manifest-present").addEventListener("change", (e) => {
  state.manifestPresent = e.target.checked;
  save(); renderAll();
});
document.getElementById("version-ok").addEventListener("change", (e) => {
  state.versionOk = e.target.checked;
  save(); renderAll();
});
document.getElementById("reset").addEventListener("click", () => {
  state = clone(DEFAULT_STATE);
  save(); renderAll();
});
document.getElementById("copy").addEventListener("click", async (e) => {
  try {
    await navigator.clipboard.writeText(renderFeed());
    e.target.textContent = "Copied ✓";
    setTimeout(() => (e.target.textContent = "Copy"), 1400);
  } catch {
    e.target.textContent = "Copy failed";
  }
});

renderAll();

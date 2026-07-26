// Almanac dashboard client. Fetches the live state snapshot and renders
// the dependency DAG, calendar list, and agents panel. Polls every 4s.
(function () {
  "use strict";
  const COMMUNITY = window.__ALMANAC_COMMUNITY__;
  const STATE_URL = `/v1/communities/${COMMUNITY}/state`;
  const SUBSCRIBE_URL = `/calendar/${COMMUNITY}.ics`;

  const STATUS_MAP = {
    Pending: { emoji: "🟡", ical: "TENTATIVE", label: "Pending", cls: "sc-pend" },
    Running: { emoji: "⏳", ical: "TENTATIVE", label: "Running", cls: "sc-pend" },
    Succeeded: { emoji: "✅", ical: "CONFIRMED", label: "Succeeded", cls: "sc-ok" },
    Failed: { emoji: "❌", ical: "CANCELLED", label: "Failed", cls: "sc-bad" },
    Skipped: { emoji: "⏸", ical: "CANCELLED", label: "Skipped", cls: "sc-bad" },
  };

  const $ = (id) => document.getElementById(id);
  const escapeHtml = (s) =>
    String(s).replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));

  // tabs
  const tabs = ["graph", "schedule", "agents"];
  tabs.forEach((t) => {
    $(`tab-${t}`).addEventListener("click", (e) => {
      e.preventDefault();
      tabs.forEach((x) => {
        $(`tab-${x}`).classList.toggle("active", x === t);
        $(`view-${x}`).hidden = x !== t;
      });
    });
  });

  $("subscribe").addEventListener("click", (e) => {
    e.preventDefault();
    // Copy the subscribe URL + tell the user.
    navigator.clipboard
      ?.writeText(window.location.origin + SUBSCRIBE_URL)
      .catch(() => {});
    window.open(SUBSCRIBE_URL, "_blank");
  });

  function statusOf(snap, sched) {
    const run = snap.runs.find((r) => r.schedule_id === sched.schedule_id);
    if (!run) return STATUS_MAP.Pending;
    // RunStatus is tagged enum {kind: "Succeeded"} or {kind:"Skipped", detail:[...]}
    const kind = run.status.kind || "Pending";
    return STATUS_MAP[kind] || STATUS_MAP.Pending;
  }

  function depsFor(snap, sched) {
    // Produces + consumes from contracts.
    const produces = snap.contracts.filter(
      (c) => c.schedule_id === sched.schedule_id && c.role === "Produce"
    );
    const deps = (snap.lineage || {})[sched.schedule_id] || [];
    return { produces, deps };
  }

  function renderDag(snap) {
    const el = $("dag");
    const empty = $("dag-empty");
    el.innerHTML = "";
    if (!snap.schedules.length) {
      empty.hidden = false;
      return;
    }
    empty.hidden = true;
    // Sort: schedules with no consumes first (sources), then consumers.
    const hasConsume = (s) =>
      snap.contracts.some(
        (c) => c.schedule_id === s.schedule_id && c.role === "Consume"
      );
    const ordered = [...snap.schedules].sort(
      (a, b) => Number(hasConsume(a)) - Number(hasConsume(b))
    );
    for (const sched of ordered) {
      const st = statusOf(snap, sched);
      const { produces, deps } = depsFor(snap, sched);
      const agent = snap.agents.find((a) => a.agent_id === sched.owner_agent_id);
      const agentTag = agent
        ? `<span class="s-owner"><span class="av">${agent.avatar || "🤖"}</span>${escapeHtml(agent.name)}</span>`
        : sched.owner_agent_id
        ? `<span class="s-owner"><span class="av">🤖</span>${escapeHtml(sched.owner_agent_id)}</span>`
        : "";

      const producesHtml = produces.length
        ? produces
            .map(
              (p) =>
                `<span class="dep-produces">↗ produces ${escapeHtml(p.schema_id)} (v${p.min_version}+)</span>`
            )
            .join("")
        : "";

      const depsHtml = deps.length
        ? deps
            .map((d) => {
              const cls = `dep-${d.state}`;
              const mark =
                d.state === "ready" ? "✅" : d.state === "missing" ? "❌" : "⚠️";
              let detail = "";
              if (d.state === "ready") detail = `v${d.detail.detail.version}`;
              else if (d.state === "version_mismatch")
                detail = `v${d.detail.detail.found} / need v${d.detail.detail.need}+`;
              else detail = "no manifest in window";
              return `<div class="dep ${cls}"><span class="d-mark">${mark}</span><span class="d-schema">${escapeHtml(d.schema_id)}</span><span class="d-detail">${escapeHtml(detail)}</span></div>`;
            })
            .join("")
        : producesHtml
        ? '<span class="no-deps">source — no inputs</span>'
        : "";

      const row = document.createElement("div");
      row.className = "dag-row";
      row.innerHTML = `
        <div class="dag-sched">
          <div class="s-head">
            <span class="emoji">${st.emoji}</span>
            <span class="s-name">${escapeHtml(sched.summary)}</span>
            <span class="s-status st-${st.ical}">${st.ical}</span>
          </div>
          <div class="s-meta">${escapeHtml(sched.rrule || "webhook")} · ${escapeHtml(sched.calendar_group)}</div>
          ${agentTag}
        </div>
        <div class="dag-deps">${producesHtml}${depsHtml || '<span class="no-deps">no declared contracts</span>'}</div>
      `;
      el.appendChild(row);
    }
  }

  function renderSchedule(snap) {
    const el = $("sched-list");
    el.innerHTML = "";
    if (!snap.schedules.length) {
      el.innerHTML = '<p class="empty">No schedules.</p>';
      return;
    }
    for (const sched of snap.schedules) {
      const st = statusOf(snap, sched);
      const card = document.createElement("div");
      card.className = `sched-card ${st.cls}`;
      card.innerHTML = `
        <div class="sc-emoji">${st.emoji}</div>
        <div class="sc-body">
          <div class="sc-name">${escapeHtml(sched.summary)}</div>
          <div class="sc-meta">${escapeHtml(sched.rrule || "webhook")} · ${escapeHtml(sched.calendar_group)}${sched.owner_agent_id ? " · " + escapeHtml(sched.owner_agent_id) : ""}</div>
        </div>
        <div class="sc-status st-${st.ical}">${st.ical}</div>
      `;
      el.appendChild(card);
    }
  }

  function renderAgents(snap) {
    const el = $("agents-list");
    el.innerHTML = "";
    if (!snap.agents.length) {
      el.innerHTML = '<p class="empty">No agents registered. Use the MCP <code>register_agent</code> tool.</p>';
      return;
    }
    for (const a of snap.agents) {
      const owned = snap.schedules.filter(
        (s) => s.owner_agent_id === a.agent_id
      ).length;
      const card = document.createElement("div");
      card.className = "agent-card";
      card.innerHTML = `
        <div class="agent-head">
          <div class="agent-av">${a.avatar || "🤖"}</div>
          <div>
            <div class="agent-name">${escapeHtml(a.name)}</div>
            <div class="agent-id">@${escapeHtml(a.agent_id)}</div>
            <span class="agent-kind">${escapeHtml(a.kind)}</span>
          </div>
        </div>
        ${a.description ? `<p class="agent-desc">${escapeHtml(a.description)}</p>` : ""}
        <div class="agent-stats"><span><b>${owned}</b> schedule${owned === 1 ? "" : "s"}</span><span><b>${escapeHtml(a.community_id)}</b></span></div>
      `;
      el.appendChild(card);
    }
  }

  async function refresh() {
    try {
      const res = await fetch(STATE_URL, { cache: "no-store" });
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      const snap = await res.json();
      $("community").textContent = snap.community;
      renderDag(snap);
      renderSchedule(snap);
      renderAgents(snap);
      $("poll-status").textContent = "live";
    } catch (e) {
      $("poll-status").textContent = "disconnected";
    }
  }

  refresh();
  setInterval(refresh, 4000);
})();

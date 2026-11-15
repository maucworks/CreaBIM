// CreaBIM interface pages. The app sends each pane its state with
// CB.state({...}); the page draws it and posts what the user does back as
// JSON with window.ipc.postMessage.
(function () {
  "use strict";

  const pane = document.documentElement.dataset.pane;
  const root = document.getElementById("root");
  let S = null; // the last state
  const local = { open: {}, filter: "", stack: [] };

  function post(msg) {
    window.ipc.postMessage(JSON.stringify(msg));
  }

  function icon(name, cls) {
    const body = window.ICONS[name] || "";
    return `<svg class="i ${cls || ""}" viewBox="0 0 24 24">${body}</svg>`;
  }

  // the CreaBIM mark: filled, in the brand colour of the theme
  // (the small cut below 48 px, the full mark from 48 px up)
  function mark(full) {
    const name = full ? "creabim-mark-full" : "creabim-mark";
    return `<svg class="mark" viewBox="0 0 200 200" aria-label="CreaBIM">${window.ICONS[name]}</svg>`;
  }

  // The weaver and its nest, woven once where the logo lives (MU39, the
  // "Woven precise" motion): the branch draws, two strands weave the
  // nest, the nest fills, the bird lands; then a line or two of where it
  // is: a settings slider, a guide's page, a new plan. In the theme's
  // logo colour. `play` false draws it still (a redraw, or reduced motion).
  function woven(kind, play) {
    const ctxArt = {
      settings: `<path class="wm-ctx" d="M222 78 H318"/><circle class="wm-ctx" cx="252" cy="78" r="9"/><path class="wm-ctx" d="M222 126 H318"/><circle class="wm-ctx" cx="292" cy="126" r="9"/>`,
      guide: `<path class="wm-ctx" d="M222 70 H306 M222 100 H318 M222 130 H290 M222 160 H300"/>`,
      new: `<path class="wm-ctx" d="M222 62 H318 V160 H222 Z M270 62 V112 H318"/>`,
    }[kind] || "";
    const w = ctxArt ? 330 : 200;
    return `<svg class="woven ${play ? "play" : "still"}" viewBox="0 0 ${w} 200" aria-label="CreaBIM">` +
      `<g transform="translate(9 4)">` +
      `<path class="wm-branch" d="M26 52 H174"/>` +
      `<path class="wm-nest" d="M130 52 C162 64 172 100 166 128 C160 156 146 166 142 190 L118 190 C114 166 98 156 94 128 C88 100 98 64 130 52 Z"/>` +
      `<path class="wm-strand" d="M98 98 L164 124"/><path class="wm-strand s2" d="M96 132 L160 156"/>` +
      `<g class="wm-bird"><path d="M44.5 8.0 C51.2 2.1 63.8 3.0 67.2 12.2 C69.7 18.9 66.4 24.8 63.0 29.0 C61.3 39.9 52.9 50.0 38.6 50.8 C32.8 51.3 27.7 50.0 23.5 48.3 L8.4 56.7 L16.8 43.3 C14.3 35.7 16.0 27.3 21.8 22.3 C27.7 17.2 37.8 14.7 44.5 8.0 Z"/><path d="M65.5 10.5 L80.6 16.4 L65.5 21.4 Z"/><circle class="wm-eye" cx="61.5" cy="11.4" r="2.8"/></g>` +
      `</g>${ctxArt}</svg>`;
  }

  // A page's major sections (its ## headings) fold, as Properties' do
  // (MU39); what is folded stays folded while the page is redrawn.
  const folded = new Set();
  function foldSections(html, pageId) {
    const parts = html.split(/(?=<h2 )/);
    return parts.map((p, i) => {
      if (i === 0 && !p.startsWith("<h2 ")) return p;
      const end = p.indexOf("</h2>") + 5;
      const id = (p.match(/id="([^"]+)"/) || [])[1] || String(i);
      const key = pageId + "#" + id;
      return `<details class="hsec" data-fold="${esc(key)}"${folded.has(key) ? "" : " open"}><summary>${p.slice(0, end)}</summary>${p.slice(end)}</details>`;
    }).join("");
  }
  // A status message cut short slides to show the rest while the pointer
  // rests on it, and back (MU39).
  document.addEventListener("mouseover", (e) => {
    const t = e.target && e.target.closest && e.target.closest(".status .msg span[title], .status .hint");
    if (!t || t.classList.contains("slide")) return;
    const over = t.scrollWidth - t.clientWidth;
    if (over < 4) return;
    t.classList.add("slide");
    t.innerHTML = `<i>${esc(t.textContent)}</i>`;
    t.style.setProperty("--slide", `-${over + 8}px`);
    t.style.setProperty("--slide-t", `${Math.max(2, over / 40)}s`);
    t.addEventListener("mouseleave", () => {
      t.classList.remove("slide");
      t.textContent = t.textContent;
    }, { once: true });
  });
  document.addEventListener("toggle", (e) => {
    const d = e.target;
    if (d && d.dataset && d.dataset.fold) {
      if (d.open) folded.delete(d.dataset.fold); else folded.add(d.dataset.fold);
    }
  }, true);

  function esc(s) {
    return String(s == null ? "" : s).replace(/[&<>"']/g, (c) => ({
      "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;",
    }[c]));
  }

  function el(html) {
    const t = document.createElement("template");
    t.innerHTML = html.trim();
    return t.content.firstChild;
  }

  // Keep a text box being typed in when the state redraws.
  function keepFocus(draw) {
    const a = document.activeElement;
    const key = a && a.dataset ? a.dataset.key : null;
    const pos = a && a.selectionStart;
    const val = a && a.value;
    draw();
    if (key) {
      const b = root.querySelector(`[data-key="${CSS.escape(key)}"]`);
      if (b) {
        b.focus();
        if (b.value !== val && b.dataset.live !== "1") b.value = val;
        try { b.setSelectionRange(pos, pos); } catch (e) { /* not a text box */ }
      }
    }
  }

  // ---- controls shared by the top bar and the status bar ----

  // A control's tooltip: its name, its key, then what it does.
  function tip(c) {
    let name = c.label || c.value || "";
    if (c.kind === "menu" && c.label && c.value) name = `${c.label}: ${c.value}`;
    return name + (c.shortcut ? `  (${c.shortcut})` : "") + (c.hint ? (name ? "\n" : "") + c.hint : "");
  }

  // `full` draws a compact control with its words too (in the More menu).
  function control(c, full) {
    const compact = c.compact && !full;
    const slot = c.slot ? ` data-slot="${esc(c.slot)}"` : "";
    switch (c.kind) {
      case "number": {
        // a length comes formatted in the project's units, and what is
        // typed in it goes back as text for the app to read
        const text = c.text != null;
        return `<label class="field"${slot} title="${esc(tip(c))}"><span class="k" data-scrub="${esc(c.key)}" data-step="${c.step}" data-min="${c.min}" data-max="${c.max}">${esc(c.label)}</span>` +
          `<input class="${text ? "len" : ""}" data-key="${esc(c.key)}" data-step="${c.step}" data-min="${c.min}" data-max="${c.max}" data-v="${c.value}" data-text="${text ? 1 : 0}" value="${text ? esc(c.text) : fmt(c.value, c.step)}">` +
          (c.unit ? `<span class="u">${esc(c.unit)}</span>` : "") + `</label>`;
      }
      case "menu":
        if (compact) {
          return `<button class="dd icon ${S.popup === c.key ? "open" : ""}"${slot} data-popup="${esc(c.key)}" title="${esc(tip(c))}" ${c.enabled === false ? "disabled" : ""}>` +
            icon(c.icon) + `${icon("chevron-down", "chev")}</button>`;
        }
        return `<button class="dd ${c.big ? "big" : ""} ${S.popup === c.key ? "open" : ""}"${slot} data-popup="${esc(c.key)}" title="${esc(tip(c))}" ${c.enabled === false ? "disabled" : ""}>` +
          (c.icon ? icon(c.icon) : "") + (c.label ? `<span class="k">${esc(c.label)}</span>` : "") +
          `<span class="v">${esc(c.value)}</span>${icon("chevron-down", "chev")}</button>`;
      case "button":
        return `<button class="btn ${compact ? "icon" : ""}"${slot} data-act="${esc(c.id)}" title="${esc(tip(c))}">${c.icon ? icon(c.icon) : ""}${compact ? "" : esc(c.label)}</button>`;
      case "toggle":
        return `<button class="toggle ${compact ? "icon" : ""} ${c.on ? "on" : ""}"${slot} data-act="${esc(c.id)}" data-keep="${full ? 1 : 0}" title="${esc(tip(c))}">${c.icon ? icon(c.icon) : ""}${compact ? "" : esc(c.label)}</button>`;
      case "label":
        return `<span class="ctl-label"${slot}>${esc(c.label)}</span>`;
      case "seg":
        return `<span class="seg"${slot}>` + c.items.map((i) => `<button class="${i.on ? "on" : ""}${i.icon ? " ico" : ""}" data-act="${esc(i.id)}" data-keep="${full ? 1 : 0}" title="${esc(i.hint)}">${i.icon ? icon(i.icon) : esc(i.label)}</button>`).join("") + `</span>`;
      case "sep":
        return `<span class="vsep"${slot}></span>`;
      case "meta":
        return `<span class="meta"${slot}>${esc(c.label)}</span>`;
    }
    return "";
  }

  function fmt(v, step) {
    const d = step >= 1 ? 0 : step >= 0.1 ? 1 : 2;
    return Number(v).toFixed(d);
  }

  function setNumber(input, v) {
    const min = parseFloat(input.dataset.min), max = parseFloat(input.dataset.max);
    if (!isNaN(min)) v = Math.max(min, v);
    if (!isNaN(max)) v = Math.min(max, v);
    input.dataset.v = v;
    if (input.dataset.text !== "1") input.value = fmt(v, parseFloat(input.dataset.step));
    post({ t: "set", key: input.dataset.key, value: v });
  }

  // ---- top pane ----

  function drawTop() {
    const menus = S.menus.map((m) => `<button class="menubtn ${S.popup === m.key ? "open" : ""}" data-popup="${esc(m.key)}" data-menubar="1">${esc(m.name)}</button>`).join("");
    const quick = S.quick.map((q) => q === null ? `<span class="qsep"></span>` :
      `<button class="ibtn" data-act="${esc(q.id)}" title="${esc(q.label)}${q.key ? "  (" + esc(q.key) + ")" : ""}" ${q.enabled ? "" : "disabled"}>${icon(q.icon)}</button>`).join("");
    // the tabs, then Select and Align, then the tab's own buttons: icons
    // only, the name in the tip and, for the tool in use, in the strip below
    const tabs = (S.tabs || []).map((t) => `<button class="ttab ${t.on ? "on" : ""}" data-act="${esc(t.id)}">${esc(t.label)}</button>`).join("");
    const tools = S.tools.map((t) =>
      t.kind === "sep" ? `<span class="vsep"></span>` :
      t.kind === "menu" ? `<button class="tool ${t.open ? "on" : ""}" data-popup="${esc(t.key)}" title="${esc(t.label + ": " + t.hint)}">${icon(t.icon)}${icon("chevron-down", "tchev")}</button>` :
      `<button class="tool ${t.active ? "on" : ""} ${t.off ? "off" : ""}" data-act="${esc(t.id)}" title="${esc(t.label + ": " + t.hint)}">${icon(t.icon)}</button>`).join("");
    const html =
      `<div class="bar1"><button class="brand menubtn ${S.popup === S.app.key ? "open" : ""}" data-popup="${esc(S.app.key)}" data-menubar="1" title="About, updates, settings and exit"><span class="logo">${mark(true)}</span><span class="name">${esc(S.app.label)}</span>${icon("chevron-down", "chev")}</button>${menus}<span class="vsep"></span>` +
      `<div class="quick">${quick}</div><span class="spacer"></span><div class="doc" title="${esc(S.doc.path)}${S.doc.unsaved === false ? "" : " · unsaved changes"}">${S.doc.unsaved === false ? "" : `<span class="dot"></span>`}${esc(S.doc.name)}</div>${S.right.map(control).join("")}</div>` +
      `<div class="bar2"><div class="ttabs">${tabs}</div><div class="tools">${tools}</div><span class="spacer"></span></div>` +
      // the tool options strip: the tool in use and what it can be set to,
      // then the active view's own settings at the right. What doesn't fit
      // goes in a More menu at the end (the app picks what, from the
      // widths measured below).
      `<div class="bar3"><div class="toolname">${S.tool.icon ? icon(S.tool.icon) : ""}<b>${esc(S.tool.label)}</b>` +
      (S.tool.help ? `<button class="qhelp ${S.popup === "toolhelp" ? "open" : ""}" data-popup="toolhelp" title="How to use the ${esc(S.tool.label)} tool (F1 opens its page)">${icon("circle-help")}</button>` : "") + `</div>` +
      (S.tool_options.length ? `<div class="options tool-opts">${shown(S.tool_options).map((c) => control(c)).join("")}</div>` +
        // Select keeps its hint after its tools: what is selected (MU39)
        (S.tool.trail ? `<div class="tool-hint" title="${esc(S.tool.hint)}">${esc(S.tool.hint || "")}</div>` : "") :
        `<div class="tool-hint" title="${esc(S.tool.hint)}">${esc(S.tool.hint || "")}</div>`) +
      `<span class="spacer"></span>` +
      (S.tool_options.length && S.options.length ? `<span class="vsep group-sep"></span>` : "") +
      `<div class="options view-opts">${shown(S.options).map((c) => control(c)).join("")}</div>` +
      (S.more ? `<button class="dd more-btn ${S.popup === "more" ? "open" : ""}" data-popup="more" title="${S.more} more ${S.more === 1 ? "option" : "options"} that don't fit here">${icon("ellipsis")}<span class="v">More</span></button>` : "") +
      `</div>` +
      // every control at its own width, out of sight, to measure
      `<div class="bar3 measure" aria-hidden="true"><div class="m-tool">${S.tool_options.map((c) => control(c)).join("")}</div>` +
      `<div class="m-view">${S.options.map((c) => control(c)).join("")}</div>` +
      `<span class="m-more"><button class="dd more-btn">${icon("ellipsis")}<span class="v">More</span></button></span></div>`;
    const cut = html.indexOf(`<div class="bar3 measure"`);
    // a row scrolled sideways stays where it was
    const scrolled = ROWS.map((sel) => { const r = root.querySelector(sel); return r ? r.scrollLeft : 0; });
    // the measured copies answer no clicks and hold no typing
    root.innerHTML = html.slice(0, cut) + html.slice(cut).replace(/ data-(key|act|popup|scrub)=/g, " data-m$1=");
    ROWS.forEach((sel, i) => {
      const r = root.querySelector(sel);
      r.scrollLeft = scrolled[i];
      r.addEventListener("scroll", () => edges(r));
      edges(r);
    });
    measureBar();
  }

  // The menu and tool rows. In a narrow window they scroll sideways, with
  // the wheel too, and fade at an end that has more past it.
  const ROWS = [".bar1", ".bar2"];
  function edges(row) {
    const more = row.scrollWidth - row.clientWidth;
    row.classList.toggle("more-l", more > 1 && row.scrollLeft > 1);
    row.classList.toggle("more-r", more > 1 && row.scrollLeft < more - 1);
  }
  document.addEventListener("wheel", (e) => {
    const row = pane === "top" && e.target.closest && e.target.closest(ROWS.join(","));
    if (!row || row.scrollWidth <= row.clientWidth) return;
    e.preventDefault();
    row.scrollLeft += 2 * (e.deltaX || e.deltaY);
  }, { passive: false });

  // The controls the More menu doesn't hold.
  function shown(list) {
    return list.filter((c) => !c.more);
  }

  // Tell the app how wide each control is and how much room the strip
  // has, when that changes; it answers with what goes in More.
  function measureBar() {
    const bar = root.querySelector(".bar3:not(.measure)");
    const m = root.querySelector(".bar3.measure");
    if (!bar || !m) return;
    const w = (e) => Math.ceil(e.getBoundingClientRect().width);
    const items = [];
    const take = (sel, view, list) => {
      [...m.querySelectorAll(sel + " > *")].forEach((k, i) => {
        const cs = getComputedStyle(k);
        const out = w(k) + Math.ceil(parseFloat(cs.marginLeft) + parseFloat(cs.marginRight));
        items.push({ k: (list[i] && list[i].slot) || "", w: out, v: view });
      });
    };
    take(".m-tool", false, S.tool_options);
    take(".m-view", true, S.options);
    const name = bar.querySelector(".toolname");
    const hint = bar.querySelector(".tool-hint");
    // the side padding, the gaps round the name and the spacer, the
    // divider between the groups, and room for a little of the hint
    const fixed = 20 + w(name) + 3 * 8 + 13 + (hint ? 120 : 0);
    const msg = { t: "bar", avail: bar.clientWidth - fixed, gap: 6, more: w(m.querySelector(".m-more button")) + 6, items };
    const key = JSON.stringify(msg);
    if (key !== local.bar) {
      local.bar = key;
      post(msg);
    }
  }

  window.addEventListener("resize", () => {
    if (pane !== "top" || !S) return;
    measureBar();
    ROWS.forEach((sel) => { const r = root.querySelector(sel); if (r) edges(r); });
  });

  // ---- left pane: project browser ----

  function drawLeft() {
    const f = local.filter.trim().toLowerCase();
    const anyOpen = S.groups.some((g) => (local.open[g.key] !== undefined ? local.open[g.key] : g.open));
    // folded to a rail (MU27): one icon a group; a click opens the browser
    // on that group
    if (S.folded) {
      // a click lists the group's items beside the rail, to pick one
      // without opening the browser (MU29); the wheel scrolls the rail
      let rail = `<div class="rail">`;
      for (const g of S.groups) rail += `<button class="rail-btn" data-popup="rail:${esc(g.key)}" title="${esc(g.title)} (${g.rows.length})">${icon(g.icon)}</button>`;
      root.innerHTML = rail + `</div>`;
      return;
    }
    let html = `<div class="browser"><div class="browser-head"><div class="title">${icon("layout-dashboard")}Project<span class="count">${esc(S.count)}</span>` +
      `<button class="gact fold" data-fold="${anyOpen ? 0 : 1}" title="${anyOpen ? "Collapse every group" : "Expand every group"}">${icon(anyOpen ? "chevrons-down-up" : "chevrons-up-down")}</button>` +
      `</div>` +
      `<label class="search">${icon("search")}<input data-key="filter" data-live="1" placeholder="Find views and elements" value="${esc(local.filter)}"></label></div><div class="tree">`;
    let any = false;
    for (const g of S.groups) {
      const rows = f ? g.rows.filter((r) => r.name.toLowerCase().includes(f) || (r.mark && r.mark.toLowerCase().includes(f))) : g.rows;
      if (f && rows.length === 0) continue;
      any = true;
      const open = f ? true : (local.open[g.key] !== undefined ? local.open[g.key] : g.open);
      const acts = (g.acts || []).map((a) => `<button class="gact" data-act="${esc(a.id)}" title="${esc(a.hint)}">${icon(a.icon)}</button>`).join("");
      const note = g.note ? `<span class="gnote">${esc(g.note)}</span>` : "";
      html += `<div class="group ${open ? "open" : ""}" data-group="${esc(g.key)}">${icon("chevron-right", "chev")}${icon(g.icon)}<span>${esc(g.title)}</span>${note}${acts}<span class="n">${g.rows.length}</span></div>`;
      if (open) {
        // rows of a sortable group can be dragged to another place in it
        const sort = g.sortable && !f ? ` data-sort="${esc(g.key)}"` : "";
        for (const r of rows) {
          html += `<div class="row ${r.sel ? "sel" : ""}" data-id="${r.id}"${sort} title="${esc(r.hint || r.name)}${sort ? " · drag to reorder" : ""}">${icon(r.icon)}<span>${esc(r.name)}</span>${r.active ? `<span class="dot"></span>` : ""}</div>`;
        }
      }
    }
    if (!any) html += `<div class="empty">Nothing matches “${esc(local.filter)}”.</div>`;
    html += `</div></div>`;
    const tree = root.querySelector(".tree");
    const scroll = tree ? tree.scrollTop : 0;
    keepFocus(() => { root.innerHTML = html; });
    const t2 = root.querySelector(".tree");
    if (t2) t2.scrollTop = scroll;
    showDrag();
  }

  // ---- dragging a view to another place in its group ----

  // The row being dragged and where it would land: {id, group, y, on, over, after}.
  function showDrag() {
    const d = local.drag;
    root.querySelectorAll(".row.dragging, .row.drop-before, .row.drop-after").forEach((r) => r.classList.remove("dragging", "drop-before", "drop-after"));
    if (!d || !d.on) return;
    const me = root.querySelector(`.row[data-id="${CSS.escape(d.id)}"]`);
    if (me) me.classList.add("dragging");
    const over = d.over && root.querySelector(`.row[data-id="${CSS.escape(d.over)}"]`);
    if (over) over.classList.add(d.after ? "drop-after" : "drop-before");
  }

  function dragMove(e) {
    const d = local.drag;
    if (!d) return;
    if (!d.on && Math.abs(e.clientY - d.y) < 5) return;
    d.on = true;
    const el = document.elementFromPoint(e.clientX, e.clientY);
    const row = el && el.closest(".row");
    if (row && row.dataset.sort === d.group) {
      const r = row.getBoundingClientRect();
      d.over = row.dataset.id;
      d.after = e.clientY > r.top + r.height / 2;
    }
    showDrag();
  }

  function dragEnd() {
    const d = local.drag;
    local.drag = null;
    document.removeEventListener("mousemove", dragMove);
    document.removeEventListener("mouseup", dragEnd);
    if (!d || !d.on) return showDrag();
    // a click follows the mouse-up on the same row: it isn't a pick
    local.dragged = true;
    setTimeout(() => { local.dragged = false; }, 0);
    showDrag();
    if (!d.over || d.over === d.id) return;
    const rows = [...root.querySelectorAll(`.row[data-sort="${CSS.escape(d.group)}"]`)].map((r) => r.dataset.id);
    let i = rows.indexOf(d.over) + (d.after ? 1 : 0);
    if (rows[i] === d.id) i += 1;
    post({ t: "reorder", id: d.id, before: rows[i] || null });
  }

  // ---- status pane ----

  function drawStatus() {
    const mouse = (S.mouse || []).map((m) => `<span class="mb"><svg viewBox="0 0 12 16"><rect x="1" y="1" width="10" height="14" rx="5"/><path class="${m.b}" d="${m.b === "l" ? "M1 6 V6 a5 5 0 0 1 5 -5 V6 Z" : m.b === "r" ? "M11 6 a5 5 0 0 0 -5 -5 V6 Z" : "M5 2.5 h2 v3 h-2 Z"}"/></svg>${esc(m.label)}</span>`).join("");
    root.innerHTML = `<div class="status"><span class="mice">${mouse}</span><div class="msg"><span class="pulse"></span><span title="${esc(S.message)}">${esc(S.message)}</span>${S.hint ? `<span class="hint" title="${esc(S.hint)}">${esc(S.hint)}</span>` : ""}</div>` +
      S.controls.map(control).join("") + `</div>`;
  }

  // ---- the 3D view's navigation buttons ----

  function drawNav() {
    let html = `<div class="nav">`;
    for (const b of S.buttons) {
      if (!b) continue;
      html += `<button class="ibtn ${b.on ? "on" : ""}" data-act="${esc(b.id)}" title="${esc(b.hint)}" ${b.off ? "disabled" : ""}>${icon(b.icon)}</button>`;
    }
    // dropdowns: what the 3D view shows
    for (const m of S.menus || []) {
      const label = m.label ? `<span class="v">${esc(m.label)}</span>` : "";
      html += `<span class="qsep"></span><button class="dd ${m.on ? "on" : ""} ${S.popup === m.key ? "open" : ""}" data-popup="${esc(m.key)}" title="${esc(m.hint)}">${icon(m.icon)}${label}${icon("chevron-down", "chev")}</button>`;
    }
    html += `<span class="qsep"></span><button class="dd ${S.popup === S.look.key ? "open" : ""}" data-popup="${esc(S.look.key)}" title="${esc(S.look.hint)}">${icon("axis-3d")}<span class="v">View</span>${icon("chevron-down", "chev")}</button></div>`;
    root.innerHTML = html;
  }

  // ---- command palette ----

  function drawPalette() {
    if (local.key !== S.key) {
      local.key = S.key;
      local.query = "";
      local.hot = 0;
    }
    const words = (local.query || "").toLowerCase().split(/\s+/).filter(Boolean);
    const found = S.items.filter((i) => i.id && words.every((w) => i.label.toLowerCase().includes(w)));
    local.found = found;
    local.hot = Math.max(0, Math.min(local.hot || 0, found.length - 1));
    let html = `<div class="palette"><label class="pq">${icon("search")}<input data-key="palette" data-live="1" placeholder="Type a command, a tool or a view" value="${esc(local.query || "")}"></label><div class="pl">`;
    found.slice(0, 60).forEach((i, n) => {
      html += `<button class="mi ${n === local.hot ? "hot" : ""}" data-act="${esc(i.id)}" data-n="${n}" title="${esc(i.hint)}"><span class="ck">${i.icon ? icon(i.icon) : ""}</span><span class="label">${esc(i.label)}</span>${i.key ? `<span class="key">${esc(i.key)}</span>` : ""}</button>`;
    });
    if (found.length === 0) html += `<div class="empty">Nothing matches.</div>`;
    html += `</div><div class="pfoot"><span>↑↓ choose</span><span>Enter run</span><span>Esc close</span></div></div>`;
    keepFocus(() => { root.innerHTML = html; });
    const input = root.querySelector("input[data-key=palette]");
    if (input && document.activeElement !== input) { input.focus(); input.setSelectionRange(input.value.length, input.value.length); }
    const hot = root.querySelector(".mi.hot");
    if (hot) hot.scrollIntoView({ block: "nearest" });
    // the height it wants, measured without the window's cap, so a window
    // made taller gives the list its room back
    const pal = root.querySelector(".palette");
    pal.style.maxHeight = "none";
    const want = Math.ceil(pal.scrollHeight) + 2;
    pal.style.maxHeight = "";
    post({ t: "size", w: window.innerWidth, h: want });
  }

  // ---- popup menus ----

  function drawPopup() {
    if (S.close) return drawClose(S.close);
    if (S.dialog) return drawDialog(S.dialog);
    if (S.key === "palette" && S.items && S.items.length) return drawPalette();
    if (S.controls && S.controls.length) return drawPanel(controlsHtml());
    if (S.help) return drawPanel(helpHtml());
    if (!S.items || S.items.length === 0) {
      root.innerHTML = "";
      local.stack = [];
      local.key = null;
      return;
    }
    if (local.key !== S.key) {
      local.key = S.key;
      local.stack = [];
      local.hot = -1;
    }
    // Follow the open submenu into fresh items, by label.
    let items = S.items;
    let title = null;
    for (const label of local.stack) {
      const sub = items.find((i) => i.items && i.label === label);
      if (!sub) { local.stack = []; items = S.items; title = null; break; }
      items = sub.items;
      title = label;
    }
    const row = (i) => {
      if (i.sep) return `<div class="msep"></div>`;
      if (i.header) return `<div class="mhead">${esc(i.header)}</div>`;
      if (i.items) return `<button class="mi" data-sub="${esc(i.label)}" ${i.enabled ? "" : "disabled"}><span class="ck">${i.icon ? icon(i.icon) : ""}</span><span class="label">${esc(i.label)}</span>${icon("chevron-right", "sub")}</button>`;
      // one icon column for every row; a toggle's state at the row's end,
      // before its shortcut (A7-6). A toggle without an icon shows its
      // tick in the icon column, as a plain check item.
      const toggle = i.checked != null;
      const lead = i.icon ? icon(i.icon) : (i.checked ? icon("check") : "");
      const state = toggle && i.icon ? `<span class="state">${i.checked ? icon("check") : ""}</span>` : "";
      return `<button class="mi ${toggle && i.checked ? "on" : ""}" data-act="${esc(i.id)}" data-keep="${i.keep ? 1 : 0}" title="${esc(i.hint)}" ${i.enabled ? "" : "disabled"}>` +
        `<span class="ck">${lead}</span><span class="label">${esc(i.label)}</span>${state}${i.key ? `<span class="key">${esc(i.key)}</span>` : ""}</button>`;
    };
    const cols = S.columns > 1 && title === null ? columns(items, S.columns) : null;
    let html = `<div class="mwrap"><div class="menu ${cols ? "cols" : ""}" ${cols ? `style="--cols:${cols.length}"` : ""}>`;
    if (title !== null) html += `<button class="head" data-back="1">${icon("arrow-left")}<span>${esc(title)}</span></button><div class="msep"></div>`;
    if (cols) {
      // a long menu with sections reads as a panel of columns
      for (const col of cols) html += `<div class="col">${col.map(row).join("")}</div>`;
    } else {
      html += items.map(row).join("");
    }
    html += `</div><div class="more">${icon("chevron-down")}</div></div>`;
    root.innerHTML = html;
    if (local.hot >= 0) {
      const b = root.querySelectorAll(".menu .mi:not(:disabled), .menu .head")[local.hot];
      if (b) b.classList.add("hot");
    }
    const m = root.querySelector(".menu");
    m.addEventListener("scroll", showMore);
    local.sentH = local.sentW = null;
    reportSize();
    showMore();
  }

  // ---- New project: two large choices and the samples to learn from ----

  function drawDialog(d) {
    const fresh = local.key !== S.key;
    if (fresh) { local.key = S.key; local.pick = d.starts[0].id; }
    const all = d.starts.concat(d.samples);
    if (!all.some((c) => c.id === local.pick)) local.pick = d.starts[0].id;
    const card = (c, big) => `<button class="${big ? "np-big" : "np-sample"} ${local.pick === c.id ? "on" : ""}" data-pick="${esc(c.id)}" title="${esc(c.line)}">` +
      (big ? `<span class="np-pic">${icon(c.icon)}</span><span class="np-text"><b>${esc(c.title)}</b><span>${esc(c.line)}</span></span>` :
        `<span class="np-icon">${icon(c.icon)}</span><span class="np-text"><b>${esc(c.title)}</b><span>${esc(c.line)}</span></span>`) + `</button>`;
    // the choices scroll between the title and the buttons when the window
    // is too short for them (a small window, or Windows' larger text):
    // Create and Cancel were out of reach below the edge (issue 24)
    const was = root.querySelector(".np-body");
    const top = was && !fresh ? was.scrollTop : 0;
    root.innerHTML = `<div class="np"><div class="np-head"><span class="logo np-logo">${woven("new", fresh)}</span><b>${esc(d.title)}</b><span class="np-esc">Esc to close</span></div>` +
      `<div class="np-body"><div class="np-starts">${d.starts.map((c) => card(c, true)).join("")}</div>` +
      `<div class="np-sub">${esc(d.samples_title)}</div><div class="np-samples">${d.samples.map((c) => card(c, false)).join("")}</div></div>` +
      `<div class="np-foot"><span>${esc(d.note)}</span><span class="spacer"></span><button class="np-btn" data-np="cancel">Cancel</button><button class="np-btn primary" data-np="create">Create</button></div></div>`;
    const body = root.querySelector(".np-body");
    if (body) {
      body.scrollTop = top;
      const on = body.querySelector(".on");
      if (on && on.scrollIntoView) on.scrollIntoView({ block: "nearest" });
    }
  }

  // ---- Save the changes (REV02 compact): the unsaved nest asks ----

  // The woven mark waiting to be saved (animation bank 1 + 2 + 5): the
  // bird hovers over its shadow, the nest is pale and breathes, its
  // outline marches, two strands weave, an orange dot pulses. Saved, the
  // weave closes, the nest fills, the dot turns green and the bird lands.
  function unsavedNest() {
    const nest = "M130 52 C162 64 172 100 166 128 C160 156 146 166 142 190 L118 190 C114 166 98 156 94 128 C88 100 98 64 130 52 Z";
    return `<svg class="nest" width="46" height="49" viewBox="0 -14 212 224" aria-hidden="true"><g transform="translate(9 4)">` +
      `<path class="wm-branch" d="M26 52 H174"/><ellipse class="wm-shadow" cx="40" cy="46" rx="22" ry="3.5"/>` +
      `<path class="wm-nest" d="${nest}"/><path class="wm-strand" d="M98 98 L164 124"/><path class="wm-strand s2" d="M96 132 L160 156"/>` +
      `<path class="wm-rim" d="${nest}"/><circle class="wm-dot" cx="184" cy="20" r="12"/>` +
      `<g class="wm-bird"><path d="M44.5 8.0 C51.2 2.1 63.8 3.0 67.2 12.2 C69.7 18.9 66.4 24.8 63.0 29.0 C61.3 39.9 52.9 50.0 38.6 50.8 C32.8 51.3 27.7 50.0 23.5 48.3 L8.4 56.7 L16.8 43.3 C14.3 35.7 16.0 27.3 21.8 22.3 C27.7 17.2 37.8 14.7 44.5 8.0 Z"/><path d="M65.5 10.5 L80.6 16.4 L65.5 21.4 Z"/><circle class="wm-eye" cx="61.5" cy="11.4" r="2.8"/></g></g></svg>`;
  }

  // Drawn once for a question; Saving… and Saved change it in place, so
  // the nest's motion carries on into its settling.
  function drawClose(c) {
    const failed = c.phase === "failed" || (c.phase === "saving" && c.error);
    const key = [c.file, c.fresh, c.line, failed ? c.error : ""].join("|");
    const asking = c.phase === "ask" || c.phase === "failed";
    let dlg = root.querySelector(".cs");
    // (a Save as… gives the file its name while it saves: the window
    // drawn for the question stays, so the nest settles where it is)
    if (!dlg || (asking && (dlg.dataset.k !== key || dlg.classList.contains("saved")))) {
      const fn = `<span class="fn">${esc(c.file)}</span>`;
      const title = failed ? `Couldn't save ${fn}` : c.fresh ? "Save your new project before closing?" : `Save ${fn} before closing?`;
      const line = failed ? "Your changes are still open. Nothing was lost." : esc(c.line);
      root.innerHTML = `<div class="cs unsaved" role="dialog" aria-modal="true" aria-labelledby="cs-t">` +
        `<div class="cs-body"><div class="cs-tile unsaved">${unsavedNest()}</div>` +
        `<div><h2 id="cs-t">${title}</h2><div class="cs-sub">${line}</div></div>` +
        (failed ? `<div class="cs-err">${esc(c.error)}</div>` : "") + `</div>` +
        `<div class="cs-foot"><span class="cs-hints"><kbd>Enter</kbd> save <kbd>Esc</kbd> back</span><span class="cs-sp"></span>` +
        // (icons stand in for the words when the window is narrow)
        `<button class="cs-btn danger" data-cs="discard" title="Don't save: close and let the changes go">${icon("trash-2", "ic")}<span class="tx">Don't save</span></button>` +
        `<button class="cs-btn" data-cs="cancel" title="Cancel: back to the project (Esc)">${icon("undo-2", "ic")}<span class="tx">Cancel</span></button>` +
        `<button class="cs-btn primary focus" data-cs="save" title="Save, then close (Enter)"><span class="spin"></span>${icon("save", "ic")}<span class="lbl tx"></span></button></div></div>`;
      dlg = root.querySelector(".cs");
      dlg.dataset.k = key;
      local.closeH = null;
    }
    const main = c.fresh || failed ? "Save as…" : "Save";
    const lbl = dlg.querySelector(".lbl");
    dlg.classList.toggle("saving", c.phase === "saving");
    if (c.phase === "saved") {
      if (!dlg.classList.contains("saved")) {
        dlg.querySelectorAll(".unsaved").forEach((e) => { e.classList.remove("unsaved"); e.classList.add("saved"); });
        dlg.classList.remove("unsaved");
        dlg.classList.add("saved", "closing");
        dlg.querySelector("h2").textContent = "Saved.";
        dlg.querySelector(".cs-sub").innerHTML = `Everything is in <b>${esc(c.file)}</b>. Closing CreaBIM…`;
        const err = dlg.querySelector(".cs-err");
        if (err) err.remove();
      }
      lbl.textContent = "Saved";
    } else {
      lbl.textContent = c.phase === "saving" ? "Saving…" : main;
    }
    closeSize();
  }

  // Save the changes is as tall as it needs, measured uncut; the window
  // may give less, and then it scrolls. Measured again when the pane is
  // placed or resized (it is drawn first while parked, at another width).
  function closeSize() {
    const dlg = root.querySelector(".cs");
    if (!dlg) return;
    const body = dlg.querySelector(".cs-body");
    const keep = [dlg.style.height, body.style.flex, body.style.overflowY];
    dlg.style.height = "auto"; body.style.flex = "none"; body.style.overflowY = "visible";
    const h = Math.ceil(dlg.getBoundingClientRect().height);
    [dlg.style.height, body.style.flex, body.style.overflowY] = keep;
    // (again when the window was opened anew at another height)
    if (h !== local.closeH || Math.abs(window.innerHeight - h) > 1) {
      local.closeH = h;
      post({ t: "size", w: window.innerWidth, h });
    }
  }
  window.addEventListener("resize", closeSize);

  // Enter saves, Esc goes back; nothing while it saves.
  function closeKey(e) {
    const c = S && S.close;
    if (!c) return false;
    const busy = c.phase === "saving" || c.phase === "saved";
    if (e.key === "Enter" && !busy) closeAct("save");
    else if (e.key === "Escape" && !busy) closeAct("cancel");
    else if (e.key !== "Tab") return false;
    if (e.key !== "Tab") e.preventDefault();
    return true;
  }

  function closeAct(what) {
    const dlg = root.querySelector(".cs");
    if (!dlg || dlg.classList.contains("saving") || dlg.classList.contains("saved")) return;
    // the button spins at once; the app saves on its next frame
    if (what === "save") {
      dlg.classList.add("saving");
      dlg.querySelector(".lbl").textContent = "Saving…";
    }
    post({ t: "closeask", do: what });
  }

  function dialogKey(e) {
    if (closeKey(e)) return true;
    const d = S && S.dialog;
    if (!d) return false;
    const all = d.starts.concat(d.samples).map((c) => c.id);
    let i = Math.max(0, all.indexOf(local.pick));
    if (e.key === "Escape") post({ t: "close" });
    else if (e.key === "Enter") post({ t: "act", id: local.pick });
    else if (e.key === "ArrowRight" || e.key === "ArrowDown" || e.key === "Tab") { local.pick = all[(i + 1) % all.length]; drawDialog(d); }
    else if (e.key === "ArrowLeft" || e.key === "ArrowUp") { local.pick = all[(i + all.length - 1) % all.length]; drawDialog(d); }
    else return false;
    e.preventDefault();
    return true;
  }

  document.addEventListener("click", (e) => {
    const cs = S && S.close && e.target.closest && e.target.closest("[data-cs]");
    if (cs) { closeAct(cs.dataset.cs); return; }
    if (!(S && S.dialog)) return;
    const pick = e.target.closest && e.target.closest("[data-pick]");
    if (pick) { local.pick = pick.dataset.pick; drawDialog(S.dialog); return; }
    const b = e.target.closest && e.target.closest("[data-np]");
    if (b && b.dataset.np === "cancel") post({ t: "close" });
    if (b && b.dataset.np === "create") post({ t: "act", id: local.pick });
  });
  document.addEventListener("dblclick", (e) => {
    const pick = S && S.dialog && e.target.closest && e.target.closest("[data-pick]");
    if (pick) post({ t: "act", id: pick.dataset.pick });
  });

  // A tool's help card closes when the pointer, having been on it, leaves.
  document.documentElement.addEventListener("mouseenter", () => { local.inside = true; });
  document.documentElement.addEventListener("mouseleave", () => {
    if (pane === "popup" && S && S.help && local.inside) post({ t: "close" });
    local.inside = false;
  });

  // A popup that is a small panel rather than a list: the strip's
  // overflow, or a tool's quick help.
  function drawPanel(inner) {
    local.key = S.key;
    local.stack = [];
    local.hot = -1;
    keepFocus(() => { root.innerHTML = `<div class="mwrap"><div class="menu panel">${inner}</div><div class="more">${icon("chevron-down")}</div></div>`; });
    root.querySelector(".menu").addEventListener("scroll", showMore);
    local.sentH = local.sentW = null;
    reportSize();
    showMore();
  }

  // The tool options strip's controls that don't fit there, one a row.
  function controlsHtml() {
    return S.controls.map((c) => {
      if (c.kind === "head") return `<div class="mhead">${esc(c.label)}</div>`;
      if (c.kind === "sep") return `<div class="msep"></div>`;
      return `<div class="prow">${control(c, true)}</div>`;
    }).join("");
  }

  // How to use a tool or an element, in a few lines, and its guide page.
  function helpHtml() {
    const h = S.help;
    return `<div class="hp"><div class="hp-title">${h.icon ? icon(h.icon) : icon("circle-help")}<b>${esc(h.title)}</b>` +
      (h.keys ? `<span class="hp-key">${esc(h.keys)}</span>` : "") + `</div>` +
      h.lines.map((l) => `<p>${inline(l)}</p>`).join("") +
      (h.page ? `<button class="hp-more" data-act="help:${esc(h.page)}">${icon("book-open")}<span>Read more in the guide</span>${icon("arrow-right")}</button>` : "") +
      `</div>`;
  }

  // Split a menu at its headers into `n` columns of about equal height,
  // keeping the sections in order.
  function columns(items, n) {
    const sections = [];
    for (const i of items) {
      if (i.header || sections.length === 0) sections.push([]);
      if (!i.sep) sections[sections.length - 1].push(i);
    }
    n = Math.min(n, sections.length);
    const h = sections.map((s) => s.length);
    let best = null;
    // every way to cut the sections into n runs (menus are short)
    (function cut(from, left, acc) {
      if (left === 1) {
        const runs = acc.concat([[from, sections.length]]);
        const tall = Math.max(...runs.map(([a, b]) => h.slice(a, b).reduce((x, y) => x + y, 0)));
        if (!best || tall < best.tall) best = { tall, runs };
        return;
      }
      for (let k = from + 1; k <= sections.length - left + 1; k++) cut(k, left - 1, acc.concat([[from, k]]));
    })(0, n, []);
    return best.runs.map(([a, b]) => [].concat(...sections.slice(a, b)));
  }

  // A shadow and an arrow at the bottom while more of the menu is below.
  function showMore() {
    const m = root.querySelector(".menu");
    const more = root.querySelector(".more");
    if (!m || !more) return;
    more.classList.toggle("on", m.scrollTop + m.clientHeight < m.scrollHeight - 2);
  }

  // the page's size changes as the app places it: measure again then
  window.addEventListener("resize", () => {
    if (pane !== "popup" || !root.querySelector(".menu")) return;
    reportSize();
    showMore();
  });

  // The size the menu wants: its content's own width and height, measured
  // apart from the page it is shown in. (Measuring it as placed fed the
  // page's width back into the menu's, so a menu with a scrollbar grew
  // wider every time it was placed.)
  function naturalSize(m) {
    const keep = [m.style.minWidth, m.style.width, m.style.maxHeight, m.style.overflowY];
    m.style.minWidth = "0";
    m.style.width = "max-content";
    m.style.maxHeight = "none";
    m.style.overflowY = "hidden";
    const r = m.getBoundingClientRect();
    [m.style.minWidth, m.style.width, m.style.maxHeight, m.style.overflowY] = keep;
    return { w: Math.ceil(r.width), h: Math.ceil(r.height) };
  }

  function reportSize() {
    const m = root.querySelector(".menu");
    if (!m) return;
    const { w, h } = naturalSize(m);
    // taller than the page it was given: it scrolls, and the scrollbar
    // needs room beside the items
    const bar = h > window.innerHeight - 2 + 1 ? 10 : 0;
    const want = { w: w + 2 + bar, h: h + 2 };
    if (want.w === local.sentW && want.h === local.sentH) return;
    local.sentW = want.w;
    local.sentH = want.h;
    post({ t: "size", w: want.w, h: want.h });
  }

  // Arrow keys, Enter and Esc in an open menu, from this page or passed on
  // from the views.
  function menuKey(k) {
    const buttons = [...root.querySelectorAll(".menu .mi:not(:disabled), .menu .head")];
    if (buttons.length === 0) return;
    if (k === "ArrowDown" || k === "ArrowUp") {
      const n = buttons.length;
      local.hot = local.hot < 0 ? (k === "ArrowDown" ? 0 : n - 1) : (local.hot + (k === "ArrowDown" ? 1 : -1) + n) % n;
      buttons.forEach((b, i) => b.classList.toggle("hot", i === local.hot));
      buttons[local.hot].scrollIntoView({ block: "nearest" });
      showMore();
    } else if (k === "Enter" || k === "ArrowRight") {
      const b = buttons[local.hot];
      if (b && (k === "Enter" || b.dataset.sub)) b.click();
    } else if (k === "ArrowLeft" || k === "Backspace") {
      if (local.stack.length) { local.stack.pop(); local.hot = -1; drawPopup(); }
    } else if (k === "Escape") {
      post({ t: "close" });
    }
  }

  // ---- the Help panel: the guide's pages, written in Markdown ----

  // Every page, from the app: {section, id, title, md}. The shortcuts page
  // is made by the app from its menus and arrives with the state.
  function helpPages() {
    const gen = (S && S.generated) || {};
    return (window.HELP || []).map((p) => gen[p.id] ? Object.assign({}, p, { md: gen[p.id] }) : p);
  }

  // A heading's id, as the app's help::slug makes it.
  function slug(s) {
    let out = "";
    for (const c of s.toLowerCase()) {
      if (/[\p{L}\p{N}]/u.test(c)) out += c;
      else if (out && !out.endsWith("-")) out += "-";
    }
    return out.replace(/-+$/, "");
  }

  function mdLink(text, href) {
    // a website opens in the system's browser
    if (/^https?:/.test(href)) return `<a class="ext" data-act="url:${href}" title="${href}">${text}</a>`;
    return `<a data-act="help:${href}">${text}</a>`;
  }

  function mdImage(alt, src) {
    const url = S && S.images && S.images[src];
    // click a picture to see it as wide as the panel lets it, again to go back
    return url ? `<figure class="zoomable" title="Click to enlarge"><img src="${url}" alt="${alt}"><figcaption>${alt}</figcaption></figure>` : "";
  }

  // Inline marks, on text already escaped.
  function inline(s) {
    let out = esc(s);
    out = out.replace(/`([^`]+)`/g, (m, c) => `<kbd>${c}</kbd>`);
    out = out.replace(/!\[([^\]]*)\]\(([^)]+)\)/g, (m, alt, src) => mdImage(alt, src));
    out = out.replace(/\[([^\]]+)\]\(([^)]+)\)/g, (m, t, href) => mdLink(t, href));
    out = out.replace(/\*\*([^*]+)\*\*/g, "<b>$1</b>");
    out = out.replace(/\*([^*]+)\*/g, "<i>$1</i>");
    return out;
  }

  const LI = /^(\s*)([-*]|\d+\.)\s+(.*)$/;

  // A list from line i, nested by indent. Returns [html, next line].
  function mdList(lines, i) {
    const first = lines[i].match(LI);
    const indent = first[1].length;
    const tag = /\d/.test(first[2]) ? "ol" : "ul";
    let html = `<${tag}>`;
    let item = null;
    const close = () => { if (item) html += `<li>${inline(item.text)}${item.sub}</li>`; item = null; };
    while (i < lines.length) {
      const l = lines[i];
      if (/^\s*$/.test(l)) {
        const next = lines[i + 1] && lines[i + 1].match(LI);
        if (next && next[1].length >= indent) { i++; continue; }
        break;
      }
      const m = l.match(LI);
      if (m) {
        if (m[1].length < indent) break;
        if (m[1].length > indent && item) {
          const [sub, j] = mdList(lines, i);
          item.sub += sub;
          i = j;
          continue;
        }
        close();
        item = { text: m[3], sub: "" };
        i++;
        continue;
      }
      if (item && /^\s+\S/.test(l)) { item.text += " " + l.trim(); i++; continue; }
      break;
    }
    close();
    return [html + `</${tag}>`, i];
  }

  function cells(l) {
    return l.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((c) => c.trim());
  }

  function markdown(text) {
    const lines = text.replace(/\r/g, "").split("\n");
    let html = "";
    let para = [];
    const flush = () => { if (para.length) html += `<p>${inline(para.join(" "))}</p>`; para = []; };
    let i = 0;
    while (i < lines.length) {
      const l = lines[i];
      let m;
      if (/^\s*$/.test(l)) { flush(); i++; continue; }
      if (/^<!--\s*upcoming\b.*-->\s*$/.test(l)) { flush(); html += `<div class="soon"><div class="soon-tag">Coming soon</div>`; i++; continue; }
      if (/^<!--\s*\/upcoming\s*-->\s*$/.test(l)) { flush(); html += `</div>`; i++; continue; }
      if (/^<!--.*-->\s*$/.test(l)) { i++; continue; }
      if ((m = l.match(/^(#{1,4})\s+(.*)$/))) {
        flush();
        const n = m[1].length;
        html += `<h${n} id="h-${slug(m[2])}">${inline(m[2])}</h${n}>`;
        i++;
        continue;
      }
      if (/^!\[[^\]]*\]\([^)]+\)\s*$/.test(l)) { flush(); html += inline(l.trim()); i++; continue; }
      if (/^>/.test(l)) {
        flush();
        const inner = [];
        while (i < lines.length && /^>/.test(lines[i])) inner.push(lines[i++].replace(/^>\s?/, ""));
        html += `<div class="tip">${markdown(inner.join("\n"))}</div>`;
        continue;
      }
      if (LI.test(l)) { flush(); const [h, j] = mdList(lines, i); html += h; i = j; continue; }
      if (/^\|/.test(l)) {
        flush();
        const rows = [];
        while (i < lines.length && /^\|/.test(lines[i])) rows.push(lines[i++]);
        const head = rows.length > 1 && /^[\s|:-]+$/.test(rows[1]) ? cells(rows[0]) : null;
        const body = head ? rows.slice(2) : rows;
        html += `<table>` + (head ? `<thead><tr>${head.map((c) => `<th>${inline(c)}</th>`).join("")}</tr></thead>` : "") +
          `<tbody>${body.map((r) => `<tr>${cells(r).map((c) => `<td>${inline(c)}</td>`).join("")}</tr>`).join("")}</tbody></table>`;
        continue;
      }
      para.push(l.trim());
      i++;
    }
    flush();
    return html;
  }

  // A page's words without the marks (or its title), for searching.
  function plain(md) {
    return md.replace(/^# .*$/m, " ").replace(/<!--.*?-->/g, " ").replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
      .replace(/\[([^\]]+)\]\([^)]*\)/g, "$1").replace(/[#*`>|]/g, " ").replace(/\s+/g, " ").trim();
  }

  function search(q) {
    const words = q.toLowerCase().split(/\s+/).filter(Boolean);
    const out = [];
    for (const p of helpPages()) {
      const text = plain(p.md);
      const low = text.toLowerCase();
      const title = p.title.toLowerCase();
      if (!words.every((w) => low.includes(w) || title.includes(w))) continue;
      let score = 0;
      for (const w of words) {
        if (title.includes(w)) score += 10;
        score += Math.min(5, low.split(w).length - 1);
      }
      // a few words round the first hit
      const at = Math.max(0, low.indexOf(words[0]));
      const from = Math.max(0, at - 50);
      let snip = esc((from ? "…" : "") + text.slice(from, at + 110) + "…");
      for (const w of words) snip = snip.replace(new RegExp(w.replace(/[.*+?^${}()|[\]\\]/g, "\\$&"), "gi"), (m) => `<mark>${m}</mark>`);
      out.push({ p, score, snip });
    }
    return out.sort((a, b) => b.score - a.score);
  }

  function drawHelp() {
    // a page opened from elsewhere (a menu, F1) ends the search
    const nav = S.page + "#" + (S.anchor || "");
    if (local.hnav !== undefined && local.hnav !== nav) local.hq = "";
    local.hnav = nav;
    const pages = helpPages();
    const q = local.hq || "";
    const page = pages.find((p) => p.id === S.page);
    let body = "";
    if (q.trim()) {
      const found = search(q);
      local.hfound = found;
      body = `<div class="hres-n">${found.length ? found.length + (found.length === 1 ? " page" : " pages") : "Nothing found"} for “${esc(q)}”</div>` +
        found.map((f) => `<button class="hres" data-act="help:${esc(f.p.id)}"><b>${esc(f.p.title)}</b><span class="hsec">${esc(f.p.section)}</span><span class="snip">${f.snip}</span></button>`).join("");
    } else if (!page) {
      // the contents: every page by section
      body = `<article class="hdoc"><h1>Help</h1><p>Pick a page, or search the guide above. <kbd>F1</kbd> opens the page about the tool you are using.</p>`;
      let section = null;
      for (const p of pages) {
        if (p.section !== section) {
          if (section !== null) body += `</div>`;
          section = p.section;
          body += `<h3>${esc(section)}</h3><div class="toc">`;
        }
        body += `<a data-act="help:${esc(p.id)}">${esc(p.title)}</a>`;
      }
      body += `</div></article>`;
    } else {
      const i = pages.indexOf(page);
      const prev = pages[i - 1], next = pages[i + 1];
      // the woven mark over each page, woven once as the page opens
      const opened = local.hwoven !== page.id;
      local.hwoven = page.id;
      body = `<div class="help-mark logo">${woven("guide", opened)}</div><div class="crumb"><a data-act="help:contents">Contents</a> › ${esc(page.section)}</div><article class="hdoc">${foldSections(markdown(page.md), page.id)}</article>` +
        // two equal cards, a small label over the page's title (MU30)
        `<div class="pn">${prev ? `<a class="pn-card" data-act="help:${esc(prev.id)}"><small>← Previous</small><span>${esc(prev.title)}</span></a>` : "<span></span>"}` +
        `${next ? `<a class="pn-card next" data-act="help:${esc(next.id)}"><small>Next →</small><span>${esc(next.title)}</span></a>` : "<span></span>"}</div>`;
    }
    const where = (q.trim() ? "?" : "") + S.page + "#" + (S.anchor || "");
    const scroller = root.querySelector(".help-body");
    const keep = scroller && local.hwhere === where ? scroller.scrollTop : null;
    root.innerHTML = `<div class="help"><div class="help-head">` +
      `<button class="ibtn" data-act="help.back" title="Back" ${S.back ? "" : "disabled"}>${icon("arrow-left")}</button>` +
      `<button class="ibtn ${page || q ? "" : "on"}" data-act="help:contents" title="Contents">${icon("book-open")}</button>` +
      `<label class="search">${icon("search")}<input data-key="helpq" data-live="1" placeholder="Search the guide" value="${esc(q)}"></label>` +
      `<button class="ibtn" data-act="help.close" title="Close (F1 or Esc)">${icon("x")}</button></div>` +
      `<div class="help-body">${body}</div></div>`;
    const b = root.querySelector(".help-body");
    if (keep !== null) b.scrollTop = keep;
    else {
      const h = S.anchor && !q.trim() && root.querySelector(`#h-${CSS.escape(S.anchor)}`);
      const fold = h && h.closest("details");
      if (fold) fold.open = true;
      if (h) h.scrollIntoView({ block: "start" });
      else b.scrollTop = 0;
    }
    local.hwhere = where;
  }

  // ---- drawing and events ----

  const draw = { top: drawTop, left: drawLeft, status: drawStatus, popup: drawPopup, nav: drawNav, help: drawHelp }[pane];

  // Keys typed in the views while the palette is open.
  function paletteKey(k) {
    const found = local.found || [];
    if (k === "ArrowDown" || k === "ArrowUp") {
      local.hot = (local.hot + (k === "ArrowDown" ? 1 : -1) + found.length) % Math.max(found.length, 1);
    } else if (k === "Enter") {
      const i = found[local.hot];
      post(i ? { t: "act", id: i.id, keep: false } : { t: "close" });
      return false;
    } else if (k === "Escape") {
      post({ t: "close" });
      return false;
    } else if (k === "Backspace") {
      local.query = (local.query || "").slice(0, -1);
      local.hot = 0;
    }
    return true;
  }

  window.CB = {
    state(s) {
      // profile pictures for the lists, drawn as icons (MU37)
      if (s.icons) Object.assign(window.ICONS, s.icons);
      S = s;
      if (s.theme) document.documentElement.dataset.theme = s.theme;
      keepFocus(draw);
    },
    nav(k) {
      if (S && S.key !== "palette") menuKey(k);
    },
    feed(f) {
      if (!S || S.key !== "palette") return;
      if (f.text) { local.query = (local.query || "") + f.text; local.hot = 0; }
      for (const k of f.keys) if (!paletteKey(k)) return;
      drawPalette();
    },
  };

  function focusViews(e) {
    // Give the keyboard back to the views unless a text box was clicked.
    const t = e.target;
    if (!(t instanceof HTMLInputElement)) post({ t: "focus" });
  }

  document.addEventListener("mousedown", (e) => {
    post({ t: "down" });
    const g = e.target.closest("[data-scrub]");
    if (g) startScrub(e, g);
    const row = e.button === 0 && e.target.closest(".row[data-sort]");
    if (row) {
      local.drag = { id: row.dataset.id, group: row.dataset.sort, y: e.clientY, on: false };
      document.addEventListener("mousemove", dragMove);
      document.addEventListener("mouseup", dragEnd);
    }
  });

  document.addEventListener("click", (e) => {
    if (local.dragged) return;
    const fig = e.target.closest("figure.zoomable");
    if (fig) { fig.classList.toggle("big"); return; }
    const rg = e.target.closest("[data-railgroup]");
    if (rg) local.open[rg.dataset.railgroup] = true;
    const fold = e.target.closest("[data-fold]");
    if (fold && S && S.groups) {
      for (const g of S.groups) local.open[g.key] = fold.dataset.fold === "1";
      drawLeft();
      return focusViews(e);
    }
    const t = e.target.closest("button, .row, .group, a[data-act]");
    if (!t || t.disabled) return focusViews(e);
    if (t.dataset.popup) {
      const r = t.getBoundingClientRect();
      if (S.popup === t.dataset.popup) post({ t: "close" });
      else post({ t: "popup", key: t.dataset.popup, x: r.left, y: r.bottom + 4, w: r.width, up: pane === "status" ? r.top - 4 : null, top: r.top - 4 });
    } else if (t.dataset.back) {
      local.stack.pop();
      local.hot = -1;
      drawPopup();
      return;
    } else if (t.dataset.sub) {
      local.stack.push(t.dataset.sub);
      local.hot = -1;
      drawPopup();
      root.querySelector(".menu").scrollTop = 0;
      return;
    } else if (t.dataset.act) {
      // a page picked from the search results ends the search
      if (pane === "help" && t.dataset.act.startsWith("help:") && local.hq) {
        local.hq = "";
        drawHelp();
      }
      post({ t: "act", id: t.dataset.act, keep: t.dataset.keep === "1" });
    } else if (t.dataset.group) {
      const g = S.groups.find((x) => x.key === t.dataset.group);
      const was = local.open[g.key] !== undefined ? local.open[g.key] : g.open;
      local.open[g.key] = !was;
      drawLeft();
    } else if (t.dataset.id) {
      post({ t: "pick", id: t.dataset.id, add: e.shiftKey || e.ctrlKey || e.metaKey });
    }
    focusViews(e);
  });

  // Double-click a row of the project browser: a type opens on its own,
  // an element is framed in the 3D view.
  document.addEventListener("dblclick", (e) => {
    const row = e.target.closest(".row");
    if (row && row.dataset.id && !local.dragged) post({ t: "open", id: row.dataset.id });
  });

  // Slide between menus while one is open, like a menu bar.
  document.addEventListener("mouseover", (e) => {
    const t = e.target.closest("[data-menubar]");
    if (!t || !S || !S.popup || !S.popup.startsWith("menu:") || S.popup === t.dataset.popup) return;
    const r = t.getBoundingClientRect();
    post({ t: "popup", key: t.dataset.popup, x: r.left, y: r.bottom + 4, w: r.width });
  });

  document.addEventListener("input", (e) => {
    const t = e.target;
    if (t.dataset.key === "filter") {
      local.filter = t.value;
      drawLeft();
    } else if (t.dataset.key === "palette") {
      local.query = t.value;
      local.hot = 0;
      drawPalette();
    } else if (t.dataset.key === "helpq") {
      local.hq = t.value;
      keepFocus(drawHelp);
    }
  });

  document.addEventListener("change", (e) => {
    const t = e.target;
    if (t.dataset.text === "1") {
      post({ t: "set", key: t.dataset.key, text: t.value });
    } else if (t.dataset.key && !["filter", "palette", "helpq"].includes(t.dataset.key)) {
      const v = parseFloat(t.value.replace(",", "."));
      if (!isNaN(v)) setNumber(t, v);
    }
  });

  document.addEventListener("keydown", (e) => {
    const t = e.target;
    if (t instanceof HTMLInputElement && t.dataset.key === "palette") {
      if (e.key === "ArrowDown" || e.key === "ArrowUp") {
        e.preventDefault();
        paletteKey(e.key);
        drawPalette();
      } else if (e.key === "Enter" || e.key === "Escape") {
        paletteKey(e.key);
        post({ t: "focus" });
      }
      return;
    }
    if (t instanceof HTMLInputElement && t.dataset.key === "helpq") {
      // Esc clears the search (and then closes), Enter opens the best page
      if (e.key === "Escape") {
        if (t.value) { local.hq = ""; keepFocus(drawHelp); } else post({ t: "act", id: "help.close" });
      } else if (e.key === "Enter" && local.hfound && local.hfound.length) {
        const id = local.hfound[0].p.id;
        local.hq = "";
        post({ t: "act", id: "help:" + id });
      } else if (e.key === "F1") {
        post({ t: "act", id: "help.close" });
      }
      return;
    }
    if (pane === "help" && e.key === "Escape") {
      post({ t: "act", id: "help.close" });
      return;
    }
    if (t instanceof HTMLInputElement) {
      if (e.key === "Enter") { t.blur(); post({ t: "focus" }); }
      else if (e.key === "Escape") { t.value = t.defaultValue; t.blur(); post({ t: "focus" }); }
      else if ((e.key === "ArrowUp" || e.key === "ArrowDown") && t.dataset.step) {
        e.preventDefault();
        const step = parseFloat(t.dataset.step) * (e.shiftKey ? 10 : 1);
        setNumber(t, value(t) + (e.key === "ArrowUp" ? step : -step));
      }
      return;
    }
    if (pane === "popup" && dialogKey(e)) return;
    if (pane === "popup" && e.key === "Escape" && S && S.help) { post({ t: "close" }); return; }
    if (pane === "popup" && ["ArrowDown", "ArrowUp", "ArrowLeft", "ArrowRight", "Enter", "Escape"].includes(e.key)) {
      e.preventDefault();
      menuKey(e.key);
      return;
    }
    // Anything else is a shortcut for the views.
    post({ t: "key", key: e.key, code: e.code, ctrl: e.ctrlKey || e.metaKey, shift: e.shiftKey, alt: e.altKey, repeat: e.repeat });
    if (e.ctrlKey || e.metaKey || e.key === "F1" || e.key === "F3" || e.key === "Tab") e.preventDefault();
  });

  // A number field's value: lengths keep theirs (metres) beside the text.
  function value(input) {
    return input.dataset.text === "1" ? parseFloat(input.dataset.v) : parseFloat(input.value);
  }

  // Drag a number's label sideways to change it.
  function startScrub(e, label) {
    const input = label.parentElement.querySelector("input");
    const step = parseFloat(label.dataset.step);
    const x0 = e.clientX;
    const v0 = value(input);
    let moved = false;
    e.preventDefault();
    function move(ev) {
      const dx = ev.clientX - x0;
      if (Math.abs(dx) > 2) moved = true;
      if (moved) setNumber(input, v0 + Math.round(dx / 4) * step * (ev.shiftKey ? 10 : 1));
    }
    function up() {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
      if (!moved) { input.focus(); input.select(); }
    }
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }

  // Hints shown by the app in its theme (MU26): the system's black
  // tooltip is kept from showing, the text goes to the app instead.
  (function () {
    let timer = null, shown = null;
    function hide() {
      clearTimeout(timer);
      timer = null;
      if (shown) { shown = null; post({ t: "tip", text: "" }); }
    }
    document.addEventListener("mouseover", (e) => {
      const el = e.target.closest && e.target.closest("[title], [data-tip]");
      if (!el) { hide(); return; }
      if (el.hasAttribute("title")) {
        el.dataset.tip = el.getAttribute("title");
        el.removeAttribute("title");
      }
      if (el === shown) return;
      hide();
      const text = el.dataset.tip || "";
      if (!text) return;
      timer = setTimeout(() => {
        const r = el.getBoundingClientRect();
        shown = el;
        post({ t: "tip", text, x: r.left, y: r.top, w: r.width, h: r.height });
      }, 450);
    });
    document.addEventListener("mouseleave", hide);
    // the app's word that the pointer is on the drawing area: this page
    // never hears it leave
    window.CB.away = () => { clearTimeout(timer); timer = null; shown = null; };
    document.addEventListener("mousedown", hide, true);
    document.addEventListener("wheel", hide, { passive: true });
  })();

  document.addEventListener("contextmenu", (e) => e.preventDefault());
  post({ t: "ready" });
})();

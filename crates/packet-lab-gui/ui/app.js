// Packet Lab's page: asks the app how things stand a few times a second, lists the opcodes
// seen (by family, with how well each is understood), shows the stream through the filter,
// and one message's bytes with a reader for the value at any byte. Payloads are shown as
// hex and read as numbers; nothing from them is ever put in the page as HTML.

const tauri = window.__TAURI__;
const invoke = tauri.core.invoke;
const win = tauri.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

const POLL_MS = 300;
const STATS_MS = 1000;
// Rows fetched at a time, and kept on the page at most.
const PAGE = 1000;
const MAX_SHOWN = 4000;

const state = {
  catalog: new Map(), // opcode -> what the notes say
  stats: new Map(), // opcode -> counts this source
  status: "all", // the chip
  search: "",
  picks: new Set(), // opcodes chosen by hand
  find: "",
  rows: [], // shown, oldest first
  more: false, // older matching rows than the first shown
  selected: null, // seq
  message: null,
  offset: 0,
  source: "", // what the stream is from, to notice a new one
  total: 0,
};

// ---------------------------------------------------------------- helpers

function el(tag, className, text) {
  const e = document.createElement(tag);
  if (className) e.className = className;
  if (text !== undefined) e.textContent = text;
  return e;
}
const hex4 = (op) => `0x${op.toString(16).padStart(4, "0")}`;
const statusOf = (op) => (state.catalog.get(op) || {}).status || "unknown";
function label(op) {
  const e = state.catalog.get(op);
  if (!e) return "not in the notes";
  return e.name || e.what || (e.status === "unknown" ? "unknown" : e.other) || "—";
}
const familyOf = (op) => (state.stats.get(op) || state.catalog.get(op) || {}).family || "other";
function clock(s) {
  const m = Math.floor(s / 60);
  return `${m}:${String(Math.floor(s % 60)).padStart(2, "0")}`;
}

// ---------------------------------------------------------------- the filter

// The opcodes the stream shows: those picked by hand, else those the chip and search let
// through (null: all of them).
function filterOpcodes() {
  if (state.picks.size) return [...state.picks];
  if (state.status === "all" && !state.search) return null;
  return [...state.stats.keys()].filter(shownInSide);
}

function shownInSide(op) {
  if (state.status !== "all" && statusOf(op) !== state.status) return false;
  if (!state.search) return true;
  const q = state.search.toLowerCase();
  const e = state.catalog.get(op) || {};
  return [hex4(op), e.name, e.what, e.other, familyOf(op)].some((s) => s && s.toLowerCase().includes(q));
}

let filterKey = "";
function filterChanged() {
  const key = JSON.stringify([filterOpcodes(), state.find]);
  if (key === filterKey) return false;
  filterKey = key;
  return true;
}

function query(extra) {
  return { opcodes: filterOpcodes(), find: state.find || null, ...extra };
}

// ---------------------------------------------------------------- the opcode list

let sideKey = "";
function drawSide() {
  const ops = [...state.stats.values()].filter((s) => shownInSide(s.opcode));
  const key = JSON.stringify([ops.map((s) => [s.opcode, s.count, Math.round(s.rate * 10)]), [...state.picks], state.status, state.search]);
  if (key === sideKey) return;
  sideKey = key;
  const families = new Map();
  for (const s of ops) {
    if (!families.has(s.family)) families.set(s.family, []);
    families.get(s.family).push(s);
  }
  const sum = (list) => list.reduce((n, s) => n + s.count, 0);
  const groups = [...families.entries()].sort((a, b) => sum(b[1]) - sum(a[1]));
  $("opcodes").replaceChildren(
    ...groups.map(([family, list]) => {
      const box = el("div", "family");
      const head = el("button", "family-head");
      head.type = "button";
      head.append(el("span", "", family), el("span", "", `${list.length} · ${sum(list).toLocaleString()}`));
      head.title = `Show only ${family}`;
      head.addEventListener("click", () => pickAll(list.map((s) => s.opcode)));
      box.append(head);
      for (const s of list.sort((a, b) => b.count - a.count)) {
        const b = el("button", `op st-${statusOf(s.opcode)}${state.picks.has(s.opcode) ? " on" : ""}`);
        b.type = "button";
        b.append(el("span", "dot"), el("span", "", hex4(s.opcode)), el("span", "label", label(s.opcode)));
        b.append(el("span", "num", `${s.count.toLocaleString()}${s.rate >= 0.1 ? ` · ${s.rate.toFixed(1)}/s` : ""}`));
        const e = state.catalog.get(s.opcode);
        b.title = [hex4(s.opcode), statusOf(s.opcode), e && e.what, e && e.name, e && e.other, `${s.bytes.toLocaleString()} bytes`, "click: only this · ctrl-click: add or remove"]
          .filter(Boolean)
          .join("\n");
        b.addEventListener("click", (ev) => pick(s.opcode, ev.ctrlKey || ev.metaKey));
        box.append(b);
      }
      return box;
    }),
  );
  const line = $("filter-line");
  line.replaceChildren();
  if (state.picks.size) {
    line.append(`showing ${[...state.picks].map(hex4).join(", ")} `);
    const c = el("button", "link", "[show all]");
    c.type = "button";
    c.addEventListener("click", () => pickAll([]));
    line.append(c);
  } else {
    line.textContent = `${ops.length} opcodes seen${state.status !== "all" || state.search ? " (filtered)" : ""}`;
  }
}

function pick(op, add) {
  if (add) {
    if (state.picks.has(op)) state.picks.delete(op);
    else state.picks.add(op);
  } else {
    state.picks = state.picks.size === 1 && state.picks.has(op) ? new Set() : new Set([op]);
  }
  refilter();
}

function pickAll(ops) {
  const same = ops.length === state.picks.size && ops.every((o) => state.picks.has(o));
  state.picks = same ? new Set() : new Set(ops);
  refilter();
}

function refilter() {
  sideKey = "";
  drawSide();
  if (filterChanged()) reload();
}

for (const b of document.querySelectorAll("[data-status]")) {
  b.addEventListener("click", () => {
    state.status = b.dataset.status;
    for (const c of document.querySelectorAll("[data-status]")) c.setAttribute("aria-pressed", String(c === b));
    refilter();
  });
}
$("search").addEventListener("input", (e) => {
  state.search = e.target.value.trim();
  refilter();
});
let findTimer = 0;
$("find").addEventListener("input", (e) => {
  clearTimeout(findTimer);
  findTimer = setTimeout(() => {
    state.find = e.target.value.trim();
    if (filterChanged()) reload();
    if (state.selected != null) select(state.selected);
  }, 300);
});

// ---------------------------------------------------------------- the stream

function rowEl(r) {
  const tr = el("tr");
  tr.dataset.seq = r.seq;
  tr.style.setProperty("--st", `var(--${statusOf(r.opcode)})`);
  if (r.seq === state.selected) tr.classList.add("sel");
  tr.append(
    el("td", "n", String(r.seq)),
    el("td", "n", r.t.toFixed(3)),
    el("td", "", hex4(r.opcode)),
    el("td", "name", label(r.opcode)),
    el("td", "fam", familyOf(r.opcode)),
    el("td", "n", String(r.len)),
    el("td", "payload", r.head),
  );
  return tr;
}

function drawRows(replace) {
  const body = $("rows");
  if (replace) body.replaceChildren(...state.rows.map(rowEl));
  $("empty").hidden = state.rows.length > 0;
  $("older").hidden = !state.more;
}

function atBottom() {
  const s = $("stream");
  return s.scrollHeight - s.scrollTop - s.clientHeight < 30;
}

function scrollDown() {
  if ($("follow").checked) $("stream").scrollTop = $("stream").scrollHeight;
}

let fetching = false;
async function reload() {
  fetching = true;
  try {
    const res = await invoke("rows", { query: query({ limit: PAGE }) });
    state.rows = res.rows;
    state.more = res.more;
    drawRows(true);
    scrollDown();
  } catch (e) {
    showError(e);
  } finally {
    fetching = false;
  }
}

async function fetchNew() {
  if (fetching || $("freeze").checked) return;
  const last = state.rows.length ? state.rows[state.rows.length - 1].seq : null;
  if (last == null) return reload();
  fetching = true;
  try {
    const res = await invoke("rows", { query: query({ after: last, limit: PAGE }) });
    if (!res.rows.length) return;
    // More came in than one page: start again from the newest.
    if (res.more) {
      state.rows = res.rows;
      state.more = true;
      drawRows(true);
    } else {
      const follow = $("follow").checked && atBottom();
      state.rows.push(...res.rows);
      const body = $("rows");
      body.append(...res.rows.map(rowEl));
      const over = state.rows.length - MAX_SHOWN;
      if (over > 0) {
        state.rows.splice(0, over);
        for (let i = 0; i < over; i++) body.firstChild.remove();
        state.more = true;
      }
      drawRows(false);
      if (follow) scrollDown();
    }
    $("empty").hidden = true;
  } catch (e) {
    showError(e);
  } finally {
    fetching = false;
  }
}

$("older").addEventListener("click", async () => {
  if (!state.rows.length) return;
  const s = $("stream");
  const before = s.scrollHeight;
  const res = await invoke("rows", { query: query({ before: state.rows[0].seq, limit: PAGE }) }).catch(showError);
  if (!res) return;
  state.rows.unshift(...res.rows);
  state.more = res.more;
  $("rows").prepend(...res.rows.map(rowEl));
  drawRows(false);
  s.scrollTop += s.scrollHeight - before;
});

// Scrolling up stops following; back at the bottom, it follows again.
$("stream").addEventListener("scroll", () => {
  $("follow").checked = atBottom();
});

$("rows").addEventListener("click", (e) => {
  const tr = e.target.closest("tr");
  if (tr) select(Number(tr.dataset.seq));
});

// ---------------------------------------------------------------- one message

async function select(seq) {
  state.selected = seq;
  for (const tr of $("rows").querySelectorAll("tr.sel")) tr.classList.remove("sel");
  const tr = $("rows").querySelector(`tr[data-seq="${seq}"]`);
  if (tr) tr.classList.add("sel");
  const m = await invoke("message", { seq, find: state.find || null }).catch(showError);
  if (!m) {
    $("detail-head").replaceChildren(el("span", "dim", "That message is no longer kept."));
    $("detail-body").hidden = true;
    return;
  }
  state.message = m;
  state.offset = m.marks.length ? m.marks[0][0] : 0;
  drawDetail();
}

function drawDetail() {
  const m = state.message;
  const e = state.catalog.get(m.opcode);
  const head = $("detail-head");
  head.replaceChildren();
  const st = statusOf(m.opcode);
  const tag = el("span", `st-${st}`, `● ${st}`);
  tag.style.color = "var(--st)";
  head.append(
    el("span", "op-name", `${hex4(m.opcode)} ${label(m.opcode)}`),
    "  ",
    tag,
    el("span", "dim", `  ${familyOf(m.opcode)} · ${m.payload.length} bytes · #${m.seq} at ${m.t.toFixed(3)} s`),
  );
  if (e && (e.what || e.other)) head.append(el("div", "what", [e.what, e.other].filter(Boolean).join(" · ")));
  $("detail-body").hidden = false;
  drawHex();
  drawValues();
}

function drawHex() {
  const p = state.message.payload;
  const marked = new Set();
  for (const [s, l] of state.message.marks) for (let i = s; i < s + l; i++) marked.add(i);
  const lines = [];
  for (let o = 0; o < p.length; o += 16) {
    const line = el("div", "hex-line");
    line.append(el("span", "off", o.toString(16).padStart(4, "0")));
    const bytes = el("span", "bytes");
    let ascii = "";
    for (let i = o; i < Math.min(o + 16, p.length); i++) {
      const b = el("span", `b${p[i] === 0 ? " zero" : ""}${marked.has(i) ? " mark" : ""}${i === state.offset ? " cur" : ""}`, p[i].toString(16).padStart(2, "0"));
      b.dataset.i = i;
      bytes.append(b);
      ascii += p[i] >= 0x20 && p[i] < 0x7f ? String.fromCharCode(p[i]) : "·";
    }
    line.append(bytes, el("span", "ascii", ascii));
    lines.push(line);
  }
  $("hex").replaceChildren(...lines);
}

$("hex").addEventListener("click", (e) => {
  const b = e.target.closest(".b");
  if (!b) return;
  state.offset = Number(b.dataset.i);
  for (const c of $("hex").querySelectorAll(".b.cur")) c.classList.remove("cur");
  b.classList.add("cur");
  drawValues();
});

// The value at the chosen byte, read every way it might be.
function readings(p, o) {
  const left = p.length - o;
  const view = new DataView(new Uint8Array(p).buffer);
  const out = [];
  const add = (name, len, text) => {
    if (left >= len) out.push([name, len, text]);
  };
  add("u8 / i8", 1, `${view.getUint8(o)} / ${view.getInt8(o)}`);
  if (left >= 2) add("u16 / i16", 2, `${view.getUint16(o, true)} / ${view.getInt16(o, true)}`);
  if (left >= 4) {
    add("u32 / i32", 4, `${view.getUint32(o, true)} / ${view.getInt32(o, true)}`);
    const f = view.getFloat32(o, true);
    add("f32", 4, Number.isFinite(f) ? String(+f.toPrecision(7)) : String(f));
    const secs = view.getUint32(o, true);
    if (secs > 1_600_000_000 && secs < 2_200_000_000) add("as seconds", 4, new Date(secs * 1000).toISOString());
  }
  if (left >= 8) {
    const big = view.getBigUint64(o, true);
    add("u64", 8, big.toString());
    const f = view.getFloat64(o, true);
    if (Number.isFinite(f) && Math.abs(f) > 1e-6 && Math.abs(f) < 1e12) add("f64", 8, String(+f.toPrecision(10)));
    if (big > 1_600_000_000_000n && big < 2_200_000_000_000n) add("as ms", 8, new Date(Number(big)).toISOString());
  }
  // A varint (LEB128), and the same zigzag-signed.
  let v = 0n;
  let shift = 0n;
  let i = o;
  while (i < p.length && i - o < 10) {
    const b = p[i++];
    v |= BigInt(b & 0x7f) << shift;
    shift += 7n;
    if (b < 0x80) {
      add("varint", i - o, `${v} (${i - o} bytes)`);
      add("zigzag", i - o, String((v >> 1n) ^ -(v & 1n)));
      break;
    }
  }
  // A string with its length first (one byte, as names are sent).
  const n = p[o];
  if (n > 0 && left > n) {
    const bytes = new Uint8Array(p.slice(o + 1, o + 1 + n));
    const text = new TextDecoder("utf-8", { fatal: false }).decode(bytes);
    if (!/[\u0000-\u001f�]/.test(text)) add("string (u8 length)", n + 1, JSON.stringify(text));
  }
  return out;
}

function drawValues() {
  const p = state.message.payload;
  $("offset").textContent = `${state.offset} (0x${state.offset.toString(16)})`;
  const rows = readings(p, state.offset).map(([name, len, text]) => {
    const tr = el("tr");
    tr.append(el("td", "", name), el("td", "", text));
    // Over a reading: its bytes underlined.
    tr.addEventListener("mouseenter", () => spanBytes(state.offset, len));
    tr.addEventListener("mouseleave", () => spanBytes(0, 0));
    return tr;
  });
  $("values").replaceChildren(...rows);
}

function spanBytes(start, len) {
  for (const b of $("hex").querySelectorAll(".b.span")) b.classList.remove("span");
  for (let i = start; i < start + len; i++) {
    const b = $("hex").querySelector(`.b[data-i="${i}"]`);
    if (b) b.classList.add("span");
  }
}

$("copy").addEventListener("click", () => {
  if (!state.message) return;
  const text = state.message.payload.map((b) => b.toString(16).padStart(2, "0")).join(" ");
  // The page isn't a secure context, so the clipboard API may be missing.
  const area = el("textarea");
  area.value = text;
  document.body.append(area);
  area.select();
  document.execCommand("copy");
  area.remove();
  $("copy").textContent = "copied";
  setTimeout(() => ($("copy").textContent = "copy hex"), 1200);
});

// ---------------------------------------------------------------- sources

function showError(e) {
  $("message").textContent = String(e);
}

$("live").addEventListener("click", () => invoke("start_live", { process: $("process").value }).catch(showError));
$("open").addEventListener("click", () => invoke("open_recording", { path: $("path").value, speed: Number($("speed").value) }).catch(showError));
$("path").addEventListener("keydown", (e) => {
  if (e.key === "Enter") $("open").click();
});
$("stop").addEventListener("click", () => invoke("stop_source").catch(showError));
$("clear").addEventListener("click", async () => {
  await invoke("clear").catch(showError);
  reset();
});
$("min").addEventListener("click", () => win.minimize());
$("max").addEventListener("click", () => win.toggleMaximize());
$("close").addEventListener("click", () => win.close());

// A new source, or cleared: start the page over.
function reset() {
  state.rows = [];
  state.more = false;
  state.stats = new Map();
  state.selected = null;
  state.message = null;
  sideKey = "";
  $("rows").replaceChildren();
  $("detail-head").replaceChildren(el("span", "dim", "Click a message to see its bytes."));
  $("detail-body").hidden = true;
  drawRows(false);
  drawSide();
}

// ---------------------------------------------------------------- polling

async function poll() {
  try {
    const s = await invoke("status");
    const source = `${s.kind}:${s.label}`;
    if (source !== state.source || s.total < state.total) {
      state.source = source;
      reset();
    }
    state.total = s.total;
    const src = $("source");
    src.className = `source ${s.kind}`;
    src.textContent = s.kind === "live" ? `● live · ${s.label}` : s.kind === "recording" ? `recording · ${s.label}` : "";
    $("message").textContent = s.message;
    $("counts").textContent = `${s.total.toLocaleString()} messages · ${s.kept.toLocaleString()} kept · ${s.rate.toFixed(1)}/s · ${clock(s.span_s)}`;
    await fetchNew();
  } catch (e) {
    showError(e);
  }
  setTimeout(poll, POLL_MS);
}

async function pollStats() {
  try {
    const list = await invoke("stats");
    state.stats = new Map(list.map((s) => [s.opcode, s]));
    drawSide();
    // New opcodes can change which ones a chip or search lets through.
    if (filterChanged()) reload();
  } catch (e) {
    showError(e);
  }
  setTimeout(pollStats, STATS_MS);
}

(async () => {
  const list = await invoke("catalog").catch(() => []);
  state.catalog = new Map(list.map((e) => [e.opcode, e]));
  filterChanged();
  poll();
  pollStats();
})();

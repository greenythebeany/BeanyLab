import "./style.css";
import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import { broadcast, call, closeAppWindow, guardClose, onWindowClose, openFigureWindow, tileWindows, interrupt, isDesktop, onAppEvent, onEvent, onStream, openAppWindow, pauseSim, pickFolder, pickOpenFile, pickSaveFile, setWindowTitle } from "./api";
import type { EvalResult, FileEntry, Figure, VarInfo, VarDetail } from "./api";
import { icon, fileIcon, APP_ICON } from "./icons";
import { registerLanguage, LANG, sectionAt } from "./mlang";
import * as plots from "./plots";
import { ModelEditor, emptyModel } from "./model";
import type { BlockDef, BlockSpec, Model } from "./model";

(self as any).MonacoEnvironment = { getWorker: () => new EditorWorker() };

// In the desktop app each model opens in its own window: index.html?model=<path> or ?newmodel=<name>.
const query = new URLSearchParams(location.search);
const modelWindow = query.has("model") || query.has("newmodel");
// Desktop figures are OS windows too: index.html?figure=<id>.
const figureWindow = query.has("figure");
registerLanguage(monaco);

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;
const store = {
  get(k: string) { try { return localStorage.getItem("beany." + k); } catch { return null; } },
  set(k: string, v: string) { try { localStorage.setItem("beany." + k, v); } catch { /* storage unavailable */ } },
};

// ---------------------------------------------------------------- state
let cwd = "";
let busy = false;
let dark = store.get("theme") === "dark";
const cmdHistory: string[] = [];
let histPos = -1;
let draft = "";
const navBack: string[] = [];
const navFwd: string[] = [];
let wsVars: VarInfo[] = [];
const wsSelected = new Set<string>();

interface Tab {
  id: number;
  kind: "code" | "var" | "model";
  path: string | null;
  name: string;
  model?: monaco.editor.ITextModel;
  saved?: string;
  varName?: string;
  /** Block-diagram editor and its container, for model tabs. */
  me?: ModelEditor;
  host?: HTMLElement;
}
let blockLib: BlockDef[] = [];
let untitledModelSeq = 1;
const tabs: Tab[] = [];
let activeTab: Tab | null = null;
let tabSeq = 1;
let untitledSeq = 1;

// ---------------------------------------------------------------- theme
function applyTheme() {
  document.documentElement.dataset.theme = dark ? "dark" : "light";
  monaco.editor.setTheme(dark ? "beany-dark" : "beany-light");
  store.set("theme", dark ? "dark" : "light");
  for (const f of figures.values()) plots.render(f.body, f.fig, dark);
}

// ---------------------------------------------------------------- toolstrip
type Btn = { label: string; icon: string; run: () => void; small?: boolean; title?: string; id?: string };
type Group = { label: string; items: (Btn | Btn[])[] };
const RIBBON: Record<string, Group[]> = {
  HOME: [
    { label: "File", items: [
      { label: "New Script", icon: "newScript", run: () => newScript(), title: "New script (Ctrl+N)" },
      { label: "New Function", icon: "newFunction", run: () => newFunction() },
      { label: "New Model", icon: "model", run: () => newModel(), title: "New block-diagram model" },
      { label: "Open", icon: "open", run: () => openFileDialog(), title: "Open file (Ctrl+O)" },
    ] },
    { label: "Variable", items: [
      { label: "Import Data", icon: "importData", run: () => importData() },
      [
        { label: "Open Variable", icon: "table", small: true, run: () => { const v = [...wsSelected][0]; if (v) openVariable(v); else toast("Select a variable in the Workspace first"); } },
        { label: "Clear Workspace", icon: "clear", small: true, run: () => execute("clear", true) },
      ],
    ] },
    { label: "Code", items: [
      { label: "Run", icon: "run", run: () => (activeTab?.kind === "code" || activeTab?.kind === "model" ? runActive() : runFileDialog()), title: "Run the open file or model (F5)" },
      [
        { label: "Clear Commands", icon: "terminal", small: true, run: () => clearCommand() },
        { label: "Run File…", icon: "open", small: true, run: () => runFileDialog(), title: "Choose a .m file to run" },
      ],
    ] },
    { label: "Environment", items: [
      { label: "Layout", icon: "layout", run: () => resetLayout(), title: "Reset the desktop layout" },
      [
        { label: "Set Path", icon: "path", small: true, run: () => setPath() },
        { label: dark ? "Light Theme" : "Dark Theme", icon: dark ? "sun" : "moon", small: true, id: "btn-theme", run: () => toggleTheme() },
      ],
    ] },
    { label: "Resources", items: [
      { label: "Help", icon: "help", run: () => showHelp() },
      { label: "Add-Ons", icon: "addons", run: () => toast("Add-on packages are coming in a later version") },
    ] },
  ],
  PLOTS: [
    { label: "Plots (select variables in the Workspace)", items: [
      { label: "plot", icon: "chartLine", run: () => quickPlot("plot") },
      { label: "scatter", icon: "chartScatter", run: () => quickPlot("scatter") },
      { label: "bar", icon: "chartBar", run: () => quickPlot("bar") },
      { label: "histogram", icon: "chartHist", run: () => quickPlot("histogram") },
      { label: "area", icon: "chartArea", run: () => quickPlot("area") },
      { label: "stem", icon: "chartStem", run: () => quickPlot("stem") },
      { label: "stairs", icon: "chartStairs", run: () => quickPlot("stairs") },
      { label: "surf", icon: "chartSurface", run: () => quickPlot("surf") },
      { label: "mesh", icon: "chartSurface", run: () => quickPlot("mesh") },
      { label: "contour", icon: "chartContour", run: () => quickPlot("contour") },
      { label: "imagesc", icon: "chartImage", run: () => quickPlot("imagesc") },
      { label: "pie", icon: "chartPie", run: () => quickPlot("pie") },
    ] },
    { label: "Figures", items: [
      { label: "New Figure", icon: "window", run: () => execute("figure", true) },
      { label: "Tile Figures", icon: "window", run: () => tileFigures(), title: "Arrange all figure windows side by side" },
      { label: "Close All", icon: "close", run: () => execute("close all", true) },
    ] },
  ],
  EDITOR: [
    { label: "File", items: [
      { label: "New", icon: "newScript", run: () => newScript() },
      { label: "New Model", icon: "model", run: () => newModel() },
      { label: "Open", icon: "open", run: () => openFileDialog() },
      { label: "Save", icon: "save", run: () => saveActive(false), title: "Save (Ctrl+S)" },
      [
        { label: "Save As…", icon: "saveAs", small: true, run: () => saveActive(true) },
      ],
    ] },
    { label: "Navigate", items: [
      [
        { label: "Find", icon: "find", small: true, run: () => editorAction("actions.find") },
        { label: "Go To Line", icon: "forward", small: true, run: () => editorAction("editor.action.gotoLine") },
      ],
    ] },
    { label: "Edit", items: [
      [
        { label: "Comment", icon: "comment", small: true, run: () => editorAction("editor.action.addCommentLine"), title: "Ctrl+R" },
        { label: "Uncomment", icon: "uncomment", small: true, run: () => editorAction("editor.action.removeCommentLine"), title: "Ctrl+T" },
        { label: "Indent", icon: "indent", small: true, run: () => editorAction("editor.action.indentLines") },
      ],
    ] },
    { label: "Run", items: [
      { label: "Run", icon: "run", run: () => runActive(), title: "Run (F5)" },
      { label: "Run Section", icon: "runSection", run: () => runSection(false), title: "Run section (Ctrl+Enter)" },
      { label: "Run and Advance", icon: "runAdvance", run: () => runSection(true) },
      { label: "Stop", icon: "stop", run: () => stop(), id: "btn-stop", title: "Stop (Ctrl+C in Command Window)" },
    ] },
  ],
  VIEW: [
    { label: "Panels", items: [
      { label: "Current Folder", icon: "folderTree", run: () => togglePane("pane-files") },
      { label: "Workspace", icon: "variables", run: () => togglePane("pane-workspace") },
      { label: "Command History", icon: "history", run: () => togglePane("pane-history") },
    ] },
    { label: "Appearance", items: [
      { label: "Light", icon: "sun", run: () => { dark = false; applyTheme(); } },
      { label: "Dark", icon: "moon", run: () => { dark = true; applyTheme(); } },
      { label: "Reset Layout", icon: "layout", run: () => resetLayout() },
    ] },
  ],
};
let ribbonTab = "HOME";

function renderRibbon() {
  const tabsEl = $("ribbon-tabs");
  tabsEl.innerHTML = "";
  for (const name of Object.keys(RIBBON)) {
    const b = document.createElement("button");
    b.textContent = name;
    b.className = name === ribbonTab ? "active" : "";
    b.onclick = () => { ribbonTab = name; renderRibbon(); };
    tabsEl.appendChild(b);
  }
  const strip = $("toolstrip");
  strip.innerHTML = "";
  for (const g of RIBBON[ribbonTab]) {
    const ge = document.createElement("div");
    ge.className = "tgroup";
    const body = document.createElement("div");
    body.className = "tgroup-body";
    for (const it of g.items) {
      if (Array.isArray(it)) {
        const col = document.createElement("div");
        col.className = "tcol";
        for (const b of it) col.appendChild(makeBtn({ ...b, small: true }));
        body.appendChild(col);
      } else body.appendChild(makeBtn(it));
    }
    const lab = document.createElement("div");
    lab.className = "tgroup-label";
    lab.textContent = g.label;
    ge.append(body, lab);
    strip.appendChild(ge);
  }
  syncBusyButtons();
}

function makeBtn(b: Btn): HTMLButtonElement {
  const el = document.createElement("button");
  el.className = "tbtn" + (b.small ? " small" : "");
  el.innerHTML = `${icon(b.icon, b.small ? 17 : 26)}<span>${b.label}</span>`;
  if (b.title) el.title = b.title;
  if (b.id) el.id = b.id;
  el.onclick = () => b.run();
  return el;
}

function toggleTheme() {
  dark = !dark;
  applyTheme();
  const h = RIBBON.HOME[3].items[1] as Btn[];
  h[1] = { ...h[1], label: dark ? "Light Theme" : "Dark Theme", icon: dark ? "sun" : "moon" };
  renderRibbon();
}

// ---------------------------------------------------------------- command window
const out = $("cmd-out");
const input = $<HTMLTextAreaElement>("cmd-in");
const cmdBody = $("cmd-body");

function scrollCmd() { cmdBody.scrollTop = cmdBody.scrollHeight; }

function appendText(text: string, cls?: string) {
  if (!text) return;
  // Form feed = clc: clear everything before it.
  const ff = text.lastIndexOf("\f");
  if (ff >= 0) {
    out.innerHTML = "";
    text = text.slice(ff + 1);
    if (!text) return;
  }
  const lines = text.split(/(?<=\n)/);
  for (const line of lines) {
    const span = document.createElement("span");
    const c = cls ?? (line.startsWith("Warning:") ? "warn" : undefined);
    if (c) span.className = c;
    span.textContent = line;
    out.appendChild(span);
  }
  // Keep the DOM bounded for very chatty loops.
  while (out.childNodes.length > 6000) out.removeChild(out.firstChild!);
  scrollCmd();
}

function clearCommand() { out.innerHTML = ""; input.focus(); }

function autosize() {
  input.style.height = "auto";
  input.style.height = input.scrollHeight + "px";
}

function needsMore(code: string): boolean {
  let depth = 0;
  for (const raw of code.split("\n")) {
    const line = raw.replace(/'[^'\n]*'|"[^"\n]*"/g, "").split("%")[0];
    for (const w of line.split(/[^A-Za-z0-9_]+/)) {
      if (["if", "for", "while", "switch", "try", "function", "parfor"].includes(w)) depth++;
      else if (w === "end") depth--;
    }
  }
  const t = code.trimEnd();
  return depth > 0 || t.endsWith("...");
}

input.addEventListener("input", autosize);
input.addEventListener("keydown", async (e) => {
  if (e.key === "c" && e.ctrlKey && busy && input.selectionStart === input.selectionEnd) {
    e.preventDefault();
    stop();
    return;
  }
  if (e.key === "Enter" && !e.shiftKey) {
    const code = input.value;
    if (needsMore(code)) return; // let the newline in
    e.preventDefault();
    input.value = "";
    autosize();
    histPos = -1;
    if (code.trim()) addHistory(code);
    await execute(code, true);
    return;
  }
  const atFirst = input.value.lastIndexOf("\n", input.selectionStart - 1) === -1;
  const atLast = input.value.indexOf("\n", input.selectionEnd) === -1;
  if (e.key === "ArrowUp" && atFirst && cmdHistory.length) {
    e.preventDefault();
    if (histPos === -1) { draft = input.value; histPos = cmdHistory.length; }
    // Prefix search like a shell: match history entries starting with the draft.
    let i = histPos - 1;
    while (i >= 0 && draft && !cmdHistory[i].startsWith(draft)) i--;
    if (i >= 0) { histPos = i; input.value = cmdHistory[i]; autosize(); }
    return;
  }
  if (e.key === "ArrowDown" && atLast && histPos !== -1) {
    e.preventDefault();
    let i = histPos + 1;
    while (i < cmdHistory.length && draft && !cmdHistory[i].startsWith(draft)) i++;
    if (i >= cmdHistory.length) { histPos = -1; input.value = draft; }
    else { histPos = i; input.value = cmdHistory[i]; }
    autosize();
    return;
  }
  if (e.key === "Tab") {
    e.preventDefault();
    await complete();
    return;
  }
  if (e.key === "Escape") { input.value = ""; autosize(); histPos = -1; }
});
cmdBody.addEventListener("mouseup", () => { if (!window.getSelection()?.toString()) input.focus(); });

async function complete() {
  const pos = input.selectionStart;
  const before = input.value.slice(0, pos);
  const m = before.match(/[A-Za-z_]\w*$/);
  if (!m) return;
  const list: string[] = await call("complete", { prefix: m[0] });
  if (list.length === 1) {
    input.setRangeText(list[0].slice(m[0].length), pos, pos, "end");
  } else if (list.length > 1) {
    let common = list[0];
    for (const s of list) while (!s.startsWith(common)) common = common.slice(0, -1);
    if (common.length > m[0].length) input.setRangeText(common.slice(m[0].length), pos, pos, "end");
    else appendText(list.slice(0, 60).join("  ") + (list.length > 60 ? "  …" : "") + "\n\n", "info");
  }
}

function addHistory(code: string) {
  if (cmdHistory[cmdHistory.length - 1] !== code) cmdHistory.push(code);
  renderHistory();
}

function renderHistory() {
  const el = $("history-list");
  el.innerHTML = "";
  const sep = document.createElement("div");
  sep.className = "hist-sep";
  sep.textContent = `%-- ${new Date().toLocaleDateString()} --%`;
  el.appendChild(sep);
  for (const c of cmdHistory.slice(-500)) {
    const d = document.createElement("div");
    d.className = "hist-item";
    d.textContent = c.replace(/\n/g, " ↵ ");
    d.title = c;
    d.onclick = () => { el.querySelectorAll(".sel").forEach((x) => x.classList.remove("sel")); d.classList.add("sel"); };
    d.ondblclick = () => { addHistory(c); execute(c, true); };
    el.appendChild(d);
  }
  el.scrollTop = el.scrollHeight;
}

// ---------------------------------------------------------------- execution
function setBusy(b: boolean) {
  busy = b;
  $("busy-chip").hidden = !b;
  $("cmd-prompt").classList.toggle("busy", b);
  $("status-text").textContent = b ? "Busy" : "Ready";
  syncBusyButtons();
}

function syncBusyButtons() {
  const s = document.getElementById("btn-stop") as HTMLButtonElement | null;
  if (s) s.disabled = !busy;
  document.querySelectorAll<HTMLButtonElement>(".ea.stop-btn").forEach((b) => (b.disabled = !busy));
}

// Commands typed while code is running wait their turn, like a typed-ahead console.
const queue: (() => Promise<void>)[] = [];

function enqueue(job: () => Promise<void>) {
  queue.push(job);
  if (!busy) drain();
}

async function drain() {
  while (queue.length) {
    const job = queue.shift()!;
    setBusy(true);
    try {
      await job();
    } catch (e) {
      appendText(String(e) + "\n", "err");
    } finally {
      setBusy(false);
    }
  }
  input.focus();
}

function execute(code: string, echo: boolean): Promise<void> {
  return new Promise((resolve) => enqueue(async () => {
    if (echo) appendText(">> " + code.replace(/\n/g, "\n   ") + "\n", "echo");
    try {
      handleResult(await call<EvalResult>("eval", { code }));
    } finally {
      resolve();
    }
  }));
}

function runPath(path: string): Promise<void> {
  const name = baseName(path).replace(/\.m$/i, "");
  const label = sameDir(dirName(path), cwd) ? name : `run('${path}')`;
  addHistory(label);
  return new Promise((resolve) => enqueue(async () => {
    appendText(">> " + label + "\n", "echo");
    try {
      handleResult(await call<EvalResult>("run", { path }));
    } finally {
      resolve();
    }
  }));
}

function handleResult(r: EvalResult) {
  if (r.output) appendText(r.output);
  if (r.error) appendText(r.error + "\n\n", "err");
  renderWorkspace(r.workspace);
  applyFigures(r.figures, r.closed);
  if (r.cwd !== cwd) setCwd(r.cwd, true);
  else refreshFiles();
  refreshOpenVariables();
}

async function stop() {
  if (!busy) return;
  await interrupt();
  $("status-text").textContent = "Stopping…";
}

// ---------------------------------------------------------------- workspace
function renderWorkspace(vars: VarInfo[]) {
  wsVars = vars;
  const body = $("ws-body");
  body.innerHTML = "";
  for (const s of [...wsSelected]) if (!vars.some((v) => v.name === s)) wsSelected.delete(s);
  for (const v of vars) {
    const tr = document.createElement("tr");
    if (wsSelected.has(v.name)) tr.classList.add("sel");
    tr.innerHTML = `<td><span class="ws-name">${icon(v.class === "struct" ? "folderTree" : v.class === "cell" ? "variables" : "table", 15)}<span></span></span></td><td></td><td></td><td></td>`;
    (tr.children[0].querySelector("span span") as HTMLElement).textContent = v.name;
    tr.children[1].textContent = v.value;
    (tr.children[1] as HTMLElement).title = v.value;
    tr.children[2].textContent = v.size;
    tr.children[3].textContent = v.class;
    tr.onclick = (e) => {
      if (!e.ctrlKey && !e.metaKey) wsSelected.clear();
      if (wsSelected.has(v.name) && (e.ctrlKey || e.metaKey)) wsSelected.delete(v.name);
      else wsSelected.add(v.name);
      body.querySelectorAll("tr").forEach((row, i) => row.classList.toggle("sel", wsSelected.has(vars[i].name)));
    };
    tr.ondblclick = () => openVariable(v.name);
    tr.oncontextmenu = (e) => {
      e.preventDefault();
      menu(e, [
        { label: "Open", icon: "table", run: () => openVariable(v.name) },
        { label: "Plot", icon: "chartLine", run: () => { wsSelected.clear(); wsSelected.add(v.name); quickPlot("plot"); } },
        { label: "Clear", icon: "clear", run: () => execute(`clear ${v.name}`, true) },
      ]);
    };
    body.appendChild(tr);
  }
}

function quickPlot(kind: string) {
  const names = wsVars.filter((v) => wsSelected.has(v.name)).map((v) => v.name);
  if (!names.length) { toast("Select one or more variables in the Workspace, then choose a plot"); return; }
  const args = kind === "surf" || kind === "mesh" || kind === "contour" || kind === "imagesc" || kind === "pie" || kind === "histogram"
    ? names.slice(0, kind === "surf" || kind === "mesh" || kind === "contour" ? 3 : 1)
    : names.slice(0, 2);
  const code = `figure; ${kind}(${args.join(", ")})`;
  addHistory(code);
  execute(code, true);
}

// ---------------------------------------------------------------- variable editor
async function openVariable(name: string) {
  let tab = tabs.find((t) => t.kind === "var" && t.varName === name);
  if (!tab) {
    tab = { id: tabSeq++, kind: "var", path: null, name, varName: name };
    tabs.push(tab);
  }
  activate(tab);
}

async function renderVariable(tab: Tab) {
  const host = $("var-host");
  const d = await call<VarDetail | null>("var", { name: tab.varName! });
  if (activeTab !== tab) return;
  host.innerHTML = "";
  if (!d) {
    host.innerHTML = `<div class="var-note">Variable <b></b> no longer exists in the workspace.</div>`;
    host.querySelector("b")!.textContent = tab.varName!;
    return;
  }
  const note = document.createElement("div");
  note.className = "var-note";
  note.textContent = `${d.name}  ·  ${d.rows}×${d.cols} ${d.class}` + (d.truncated ? "  ·  showing the first 500 rows and 100 columns" : "");
  const table = document.createElement("table");
  table.className = "var-table";
  const ncols = d.cells[0]?.length ?? 0;
  const head = document.createElement("tr");
  head.innerHTML = `<th class="rh"></th>` + Array.from({ length: ncols }, (_, i) => `<th>${d.class === "struct" ? (i === 0 ? "Field" : "Value") : i + 1}</th>`).join("");
  table.appendChild(head);
  d.cells.forEach((row, r) => {
    const tr = document.createElement("tr");
    const th = document.createElement("td");
    th.className = "rh";
    th.textContent = String(r + 1);
    tr.appendChild(th);
    for (const c of row) {
      const td = document.createElement("td");
      td.textContent = c;
      if (d.class === "struct" || d.class === "cell" || d.class === "char") td.style.textAlign = "left";
      tr.appendChild(td);
    }
    table.appendChild(tr);
  });
  host.append(note, table);
}

function refreshOpenVariables() {
  if (activeTab?.kind === "var") renderVariable(activeTab);
}

// ---------------------------------------------------------------- editor
const editor = monaco.editor.create($("editor-host"), {
  language: LANG,
  theme: dark ? "beany-dark" : "beany-light",
  automaticLayout: true,
  fontFamily: "Cascadia Mono, Cascadia Code, Consolas, monospace",
  fontSize: 13.5,
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  renderLineHighlight: "all",
  tabSize: 4,
  insertSpaces: true,
  detectIndentation: false,
  wordWrap: "off",
  glyphMargin: false,
  folding: true,
  smoothScrolling: true,
  padding: { top: 6 },
  model: null,
});

editor.onDidChangeCursorPosition((e) => {
  if (activeTab?.kind === "code") $("status-pos").textContent = `Ln ${e.position.lineNumber}, Col ${e.position.column}`;
});

editor.addCommand(monaco.KeyCode.F5, () => runActive());
editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.Enter, () => runSection(false));
editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyMod.Shift | monaco.KeyCode.Enter, () => runSection(true));
editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => saveActive(false));
editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyR, () => editorAction("editor.action.addCommentLine"));
editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyT, () => editorAction("editor.action.removeCommentLine"));
editor.addCommand(monaco.KeyCode.F9, () => {
  const sel = editor.getModel()?.getValueInRange(editor.getSelection()!);
  if (sel?.trim()) { addHistory(sel); execute(sel, true); }
});

// Section separators: draw a rule above every %% line.
let sectionDecos: monaco.editor.IEditorDecorationsCollection | null = null;
function decorateSections() {
  const m = editor.getModel();
  if (!m) return;
  const ranges: monaco.editor.IModelDeltaDecoration[] = [];
  for (let i = 1; i <= m.getLineCount(); i++) {
    if (/^\s*%%/.test(m.getLineContent(i))) ranges.push({ range: new monaco.Range(i, 1, i, 1), options: { isWholeLine: true, className: "section-line" } });
  }
  sectionDecos?.clear();
  sectionDecos = editor.createDecorationsCollection(ranges);
}
const sectionStyle = document.createElement("style");
sectionStyle.textContent = `.section-line { border-top: 1px solid #94A3B8; }`;
document.head.appendChild(sectionStyle);

function editorAction(id: string) {
  if (activeTab?.kind !== "code") return;
  editor.focus();
  editor.getAction(id)?.run();
}

function renderTabs() {
  updateModelWindowTitle();
  const el = $("editor-tabs");
  el.innerHTML = "";
  for (const t of tabs) {
    const b = document.createElement("div");
    b.className = "etab" + (t === activeTab ? " active" : "");
    const dirty = isDirty(t);
    const ic = t.kind === "var" ? icon("table", 14) : t.kind === "model" ? icon("model", 14) : icon("script", 14);
    b.innerHTML = `${ic}<span></span>${dirty ? `<span class="dot" title="Unsaved changes"></span>` : ""}<span class="x" title="Close">${icon("close", 13)}</span>`;
    (b.children[1] as HTMLElement).textContent = t.name;
    b.title = t.path ?? t.name;
    b.onclick = (e) => { if ((e.target as HTMLElement).closest(".x")) closeTab(t); else activate(t); };
    b.onauxclick = (e) => { if (e.button === 1) closeTab(t); };
    el.appendChild(b);
  }
  // Quick actions for the open file, always visible next to the tabs.
  if (activeTab?.kind === "code") {
    const bar = document.createElement("div");
    bar.className = "etab-actions";
    const actions: [string, string, string, () => void][] = [
      ["run", "Run", "Run file (F5)", () => runActive()],
      ["runSection", "Section", "Run current section (Ctrl+Enter)", () => runSection(false)],
      ["save", "Save", "Save (Ctrl+S)", () => saveActive(false)],
      ["stop", "Stop", "Stop running code", () => stop()],
    ];
    for (const [ic, label, title, fn] of actions) {
      const btn = document.createElement("button");
      btn.className = "ea" + (ic === "run" ? " primary" : "") + (ic === "stop" ? " stop-btn" : "");
      btn.innerHTML = `${icon(ic, 15)}<span>${label}</span>`;
      btn.title = title;
      btn.disabled = ic === "stop" && !busy;
      btn.onclick = fn;
      bar.appendChild(btn);
    }
    el.appendChild(bar);
  }
  const empty = tabs.length === 0;
  $("editor-empty").hidden = !empty;
  $("editor-host").hidden = empty || activeTab?.kind !== "code";
  $("var-host").hidden = empty || activeTab?.kind !== "var";
  $("model-hosts").hidden = empty || activeTab?.kind !== "model";
  for (const t of tabs) if (t.host) t.host.hidden = t !== activeTab;
  $("title-file").textContent = activeTab ? (activeTab.path ?? activeTab.name) : "Command Window";
}

function isDirty(t: Tab): boolean {
  if (t.kind === "code") return !!t.model && t.model.getValue() !== t.saved;
  if (t.kind === "model") return !!t.me && JSON.stringify(t.me.root) !== t.saved;
  return false;
}

function activate(t: Tab) {
  activeTab = t;
  if (t.kind === "model") {
    renderTabs();
    t.me!.render();
    $("status-pos").textContent = "";
    t.me!.focus();
    return;
  }
  if (t.kind === "code") {
    editor.setModel(t.model!);
    renderTabs();
    editor.focus();
    decorateSections();
  } else {
    renderTabs();
    renderVariable(t);
    $("status-pos").textContent = "";
  }
}

function newTab(path: string | null, name: string, text: string): Tab {
  const model = monaco.editor.createModel(text, LANG);
  const t: Tab = { id: tabSeq++, kind: "code", path, name, model, saved: path ? text : "" };
  model.onDidChangeContent(() => { renderTabsDebounced(); decorateSectionsDebounced(); });
  tabs.push(t);
  // Opening a file brings up the editor's tools, like Run and Run Section.
  if (ribbonTab !== "EDITOR") {
    ribbonTab = "EDITOR";
    renderRibbon();
  }
  activate(t);
  return t;
}

let tabTimer = 0;
function renderTabsDebounced() { clearTimeout(tabTimer); tabTimer = window.setTimeout(renderTabs, 120); }
let secTimer = 0;
function decorateSectionsDebounced() { clearTimeout(secTimer); secTimer = window.setTimeout(decorateSections, 200); }

function closeTab(t: Tab) {
  if (isDirty(t) && !confirm(`${t.name} has unsaved changes. Close it anyway?\n\nOK = close without saving · Cancel = keep open`)) return;
  const i = tabs.indexOf(t);
  tabs.splice(i, 1);
  t.model?.dispose();
  t.host?.remove();
  if (activeTab === t) {
    activeTab = tabs[i] ?? tabs[i - 1] ?? null;
    if (activeTab) activate(activeTab);
    else { editor.setModel(null); renderTabs(); }
  } else renderTabs();
}

function newScript(text = "") {
  newTab(null, `untitled${untitledSeq > 1 ? untitledSeq : ""}.m`, text);
  untitledSeq++;
}

function newFunction() {
  const name = "myFunction";
  newScript(`function out = ${name}(in)\n% ${name.toUpperCase()}  Describe what this function does.\n%   out = ${name}(in) returns ...\n\nout = in;\n\nend\n`);
}

// ---------------------------------------------------------------- models

function newModelTab(path: string | null, name: string, model: Model) {
  const host = document.createElement("div");
  host.className = "model-host";
  $("model-hosts").appendChild(host);
  const t: Tab = { id: tabSeq++, kind: "model", path, name, host, saved: path ? JSON.stringify(model) : "" };
  t.me = new ModelEditor(host, model, {
    library: blockLib,
    ports: (blocks: BlockSpec[]) => call<[number, number][]>("block_ports", { blocks }),
    onChange: () => renderTabsDebounced(),
    run: () => runModel(t),
    stop: () => { if (simPaused) { simPaused = false; t.me!.setPaused(false); } stop(); },
    pause: () => { if (simTab === t) { simPaused = !simPaused; pauseSim(simPaused); } },
    save: () => { activate(t); saveActive(false); },
    toast,
  });
  tabs.push(t);
  activate(t);
  return t;
}

function newModel() {
  const name = `untitled${untitledModelSeq > 1 ? untitledModelSeq : ""}`;
  untitledModelSeq++;
  if (isDesktop && !modelWindow) {
    openAppWindow(`model-new-${Date.now()}`, `newmodel=${encodeURIComponent(name)}`, `${name}.bmdl - BeanyLab Model`);
    return;
  }
  newModelTab(null, `${name}.bmdl`, emptyModel(name));
}

function modelWindowLabel(path: string): string {
  return "model-" + path.toLowerCase().replace(/[^a-z0-9_-]/g, "_");
}

/** Model windows report their simulations so the main window shows output, workspace and figures. */
function wireModelEvents() {
  if (modelWindow) return;
  onAppEvent("model-result", (p: { label: string; result: EvalResult }) => {
    addHistory(`sim('${p.label}')`);
    appendText(`>> sim('${p.label}')\n`, "echo");
    handleResult(p.result);
  });
  onAppEvent("model-saved", () => refreshFiles());
  onAppEvent("figure-closed", (id: number) => { figWins.delete(id); });
}

function updateModelWindowTitle() {
  const t = tabs[0];
  if (!modelWindow || !t) return;
  setWindowTitle(`${isDirty(t) ? "● " : ""}${t.name} - BeanyLab Model`);
}

// The model tab whose simulation is running, for live progress events.
let simTab: Tab | null = null;
let simPaused = false;

function onEngineEvent(name: string, p: any) {
  if (!simTab?.me) return;
  if (name === "sim-progress") {
    simTab.me.setDisplays(p.displays);
    if (!simPaused) $("status-text").textContent = `Running… t = ${+(+p.t).toPrecision(6)}`;
  } else if (name === "sim-state") {
    simPaused = p.paused;
    simTab.me.setPaused(p.paused);
    $("status-text").textContent = p.paused ? `Paused at t = ${+(+p.t).toPrecision(6)}` : "Running…";
  }
}

function runModel(t: Tab): Promise<void> {
  const label = t.name.replace(/\.bmdl$/i, "");
  if (!modelWindow) addHistory(`sim('${label}')`);
  return new Promise((resolve) => enqueue(async () => {
    appendText(`>> sim('${label}')\n`, "echo");
    simTab = t;
    simPaused = false;
    try {
      t.me!.setError(null);
      const r = await call<{ result: EvalResult; report: { displays: { id: string; value: number[] }[]; error_block: string | null; stop_time: number } }>("sim", { model: t.me!.root, label });
      handleResult(r.result);
      if (modelWindow) broadcast("model-result", { label, result: r.result });
      t.me!.setDisplays(r.report.displays);
      if (r.report.error_block) t.me!.setError(r.report.error_block);
      $("status-text").textContent = r.result.error ? "Simulation failed" : `Simulation finished at t = ${+r.report.stop_time.toPrecision(6)}`;
    } finally {
      simTab = null;
      simPaused = false;
      t.me!.setPaused(false);
      resolve();
    }
  }));
}

async function openPath(path: string) {
  const existing = tabs.find((t) => (t.kind === "code" || t.kind === "model") && t.path && samePath(t.path, path));
  if (existing) { activate(existing); return; }
  if (/\.bmdl$/i.test(path)) {
    if (isDesktop && !modelWindow) {
      openAppWindow(modelWindowLabel(path), `model=${encodeURIComponent(path)}`, `${baseName(path)} - BeanyLab Model`);
      return;
    }
    try {
      const text: string = await call("read", { path });
      const model = JSON.parse(text) as Model;
      model.solver = { ...emptyModel("").solver, ...(model.solver ?? {}) };
      model.lines = model.lines ?? [];
      const fill = (m: Model) => { for (const b of m.blocks) { b.w = b.w ?? 70; b.h = b.h ?? 50; b.params = b.params ?? {}; b.name = b.name || b.type; if (b.system) { b.system.lines = b.system.lines ?? []; fill(b.system); } } };
      fill(model);
      const t = newModelTab(path, baseName(path), model);
      t.saved = JSON.stringify(t.me!.root);
      renderTabs();
    } catch (e) {
      toast(`Could not open model: ${e}`);
    }
    return;
  }
  try {
    const text: string = await call("read", { path });
    newTab(path, baseName(path), text.replace(/\r\n/g, "\n"));
  } catch (e) {
    toast(String(e));
  }
}

async function openFileDialog() {
  const p = await pickOpenFile(cwd);
  if (p) openPath(p);
}

async function saveActive(as: boolean): Promise<boolean> {
  const t = activeTab;
  if (!t || (t.kind !== "code" && t.kind !== "model")) return false;
  const ext = t.kind === "model" ? ".bmdl" : ".m";
  let path = t.path;
  if (!path || as) {
    const suggested = joinPath(cwd, t.kind === "model" ? t.name : guessName(t));
    path = await pickSaveFile(suggested);
    if (!path) return false;
    if (!path.toLowerCase().endsWith(ext)) path += ext;
  }
  if (t.kind === "model") t.me!.root.name = baseName(path).replace(/\.bmdl$/i, "");
  const text = t.kind === "model" ? JSON.stringify(t.me!.root, null, 2) + "\n" : t.model!.getValue();
  try {
    await call("write", { path, content: text });
  } catch (e) {
    toast(String(e));
    return false;
  }
  t.path = path;
  t.name = baseName(path);
  t.saved = t.kind === "model" ? JSON.stringify(t.me!.root) : text;
  renderTabs();
  refreshFiles();
  if (modelWindow) broadcast("model-saved", { path });
  $("status-text").textContent = `Saved ${t.name}`;
  return true;
}

/** Use the function name as the file name for new function files. */
function guessName(t: Tab): string {
  const m = t.model!.getValue().match(/^\s*function\s+(?:\[[^\]]*\]\s*=\s*|\w+\s*=\s*)?([A-Za-z]\w*)/m);
  return m ? `${m[1]}.m` : t.name;
}

async function runActive() {
  const t = activeTab;
  if (t?.kind === "model") return runModel(t);
  if (!t || t.kind !== "code") return;
  if (t.model!.getValue() !== t.saved || !t.path) {
    const ok = await saveActive(false);
    if (!ok) return;
  }
  await runPath(t.path!);
}

async function runSection(advance: boolean) {
  const t = activeTab;
  if (!t || t.kind !== "code") return;
  const m = t.model!;
  const line = editor.getPosition()?.lineNumber ?? 1;
  const [s, e] = sectionAt(m.getValue(), line);
  const code = m.getValueInRange(new monaco.Range(s, 1, e, m.getLineMaxColumn(e)));
  if (advance && e < m.getLineCount()) {
    editor.setPosition({ lineNumber: e + 1, column: 1 });
    editor.revealLineInCenter(e + 1);
  }
  await execute(code, false);
}

async function runFileDialog() {
  const p = await pickOpenFile(cwd);
  if (p) runPath(p);
}

// ---------------------------------------------------------------- current folder
async function setCwd(path: string, fromEngine = false, recordNav = true) {
  if (!fromEngine) {
    try {
      path = await call<string>("cd", { path });
    } catch (e) {
      toast(String(e));
      $<HTMLInputElement>("cwd-input").value = cwd;
      return;
    }
  }
  if (recordNav && cwd && path !== cwd) { navBack.push(cwd); navFwd.length = 0; }
  cwd = path;
  $<HTMLInputElement>("cwd-input").value = cwd;
  store.set("cwd", cwd);
  refreshFiles();
}

async function refreshFiles() {
  let entries: FileEntry[] = [];
  try {
    entries = await call<FileEntry[]>("ls", { path: cwd });
  } catch {
    return;
  }
  const el = $("file-list");
  el.innerHTML = "";
  for (const f of entries) {
    const row = document.createElement("div");
    row.className = "frow" + (/\.(m|bmdl)$/i.test(f.name) ? " code" : "");
    row.innerHTML = `${fileIcon(f.name, f.is_dir)}<span class="name"></span>`;
    (row.querySelector(".name") as HTMLElement).textContent = f.name;
    row.title = f.path;
    row.onclick = () => {
      el.querySelectorAll(".sel").forEach((x) => x.classList.remove("sel"));
      row.classList.add("sel");
      showDetails(f);
    };
    row.ondblclick = () => {
      if (f.is_dir) setCwd(f.path);
      else openPath(f.path);
    };
    row.oncontextmenu = (e) => {
      e.preventDefault();
      const items: MenuItem[] = [];
      if (f.is_dir) items.push({ label: "Open folder", icon: "open", run: () => setCwd(f.path) });
      else items.push({ label: "Open", icon: "open", run: () => openPath(f.path) });
      if (/\.m$/i.test(f.name)) items.push({ label: "Run", icon: "run", run: () => runPath(f.path) });
      if (/\.bmdl$/i.test(f.name)) items.push({ label: "Simulate", icon: "run", run: () => { const c = `sim('${f.name.replace(/\.bmdl$/i, "")}')`; addHistory(c); execute(c, true); } });
      if (f.is_dir) items.push({ label: "Add to Path", icon: "path", run: () => execute(`addpath('${f.path}')`, true) });
      menu(e, items);
    };
    el.appendChild(row);
  }
}

async function showDetails(f: FileEntry) {
  const d = $("file-details");
  if (f.is_dir || !/\.m$/i.test(f.name)) {
    d.innerHTML = "";
    const b = document.createElement("b");
    b.textContent = f.name;
    d.append(b, document.createTextNode(f.is_dir ? "\nFolder" : `\n${formatSize(f.size)}`));
    return;
  }
  try {
    const text: string = await call("read", { path: f.path });
    const lines = text.split(/\r?\n/);
    const sig = lines.find((l) => /^\s*function\b/.test(l))?.trim();
    const help: string[] = [];
    let started = false;
    for (const l of lines) {
      const t = l.trim();
      if (t.startsWith("%")) { started = true; help.push(t.replace(/^%+\s?/, "")); }
      else if (started) break;
      else if (t === "" || t.startsWith("function")) continue;
      else break;
    }
    d.innerHTML = "";
    const b = document.createElement("b");
    b.textContent = sig ? sig.replace(/^function\s+/, "") : `${f.name} (script)`;
    d.append(b, document.createTextNode("\n" + (help.join("\n") || "No description")));
  } catch {
    d.textContent = f.name;
  }
}

function formatSize(n: number) {
  if (n < 1024) return `${n} bytes`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / 1024 / 1024).toFixed(1)} MB`;
}

// ---------------------------------------------------------------- figures
interface FigWin { fig: Figure; win: HTMLElement; body: HTMLElement }
const figures = new Map<number, FigWin>();
let figZ = 10;

/** Figure ids that have an OS window (desktop, main window only). */
const figWins = new Set<number>();

function figureTitle(fig: Figure): string {
  return `Figure ${fig.id}${fig.name ? ": " + fig.name : ""}`;
}

function applyFigures(figs: Figure[], closed: number[]) {
  if (isDesktop) {
    // The main window owns figure windows; model windows leave figures to it.
    if (modelWindow) return;
    for (const id of closed) {
      if (figWins.delete(id)) closeAppWindow(`figure-${id}`);
    }
    for (const fig of figs) {
      if (figWins.has(fig.id)) continue;
      figWins.add(fig.id);
      const req = (fig as any).size as [number, number] | null;
      openFigureWindow(fig.id, figureTitle(fig), req ? req[0] : 680, req ? req[1] + 40 : 560, figWins.size - 1);
    }
    if (figs.length || closed.length) broadcast("figure-data", { figures: figs, closed });
    return;
  }
  for (const id of closed) {
    const f = figures.get(id);
    if (f) { plots.purge(f.body); f.win.remove(); figures.delete(id); }
  }
  for (const fig of figs) {
    let f = figures.get(fig.id);
    if (!f) f = createFigureWindow(fig);
    f.fig = fig;
    (f.win.querySelector(".figtitle") as HTMLElement).textContent = `Figure ${fig.id}${fig.name ? ": " + fig.name : ""}`;
    plots.render(f.body, fig, dark);
    focusFigure(f);
  }
}

function createFigureWindow(fig: Figure): FigWin {
  const layer = $("figures-layer");
  const win = document.createElement("div");
  win.className = "figwin";
  const n = figures.size;
  // Honor figure('Position', ...) sizes, scaled down to fit the window.
  const req = (fig as any).size as [number, number] | null;
  const fit = req ? Math.min(1, (window.innerWidth - 80) / req[0], (window.innerHeight - 120) / req[1]) : 1;
  const w = req ? Math.round(req[0] * fit) : Math.min(640, window.innerWidth - 80);
  const h = req ? Math.round(req[1] * fit) : Math.min(500, window.innerHeight - 120);
  win.style.width = w + "px";
  win.style.height = h + "px";
  win.style.left = Math.max(20, Math.min(window.innerWidth - w - 20, window.innerWidth - w - 380 + n * 28)) + "px";
  win.style.top = Math.max(40, Math.min(window.innerHeight - h - 30, 150 + n * 28)) + "px";
  win.innerHTML = `<div class="figbar">${icon("chartLine", 16)}<span class="figtitle"></span><span class="spacer"></span><button class="ib small fig-max" title="Maximize">${icon("window", 14)}</button><button class="ib small fig-close" title="Close">${icon("close", 14)}</button></div><div class="figbody"></div>`;
  layer.appendChild(win);
  const body = win.querySelector(".figbody") as HTMLElement;
  const f: FigWin = { fig, win, body };
  figures.set(fig.id, f);
  const bar = win.querySelector(".figbar") as HTMLElement;
  const toggleMax = () => { win.classList.toggle("max"); plots.resize(body); };
  (bar.querySelector(".fig-max") as HTMLElement).onclick = toggleMax;
  bar.addEventListener("dblclick", (e) => { if (!(e.target as HTMLElement).closest("button")) toggleMax(); });
  (bar.querySelector(".fig-close") as HTMLElement).onclick = async () => {
    plots.purge(body);
    win.remove();
    figures.delete(fig.id);
    await call("close_figure", { id: fig.id });
  };
  win.addEventListener("mousedown", () => focusFigure(f));
  bar.addEventListener("mousedown", (e) => {
    if ((e.target as HTMLElement).closest("button")) return;
    e.preventDefault();
    const sx = e.clientX - win.offsetLeft;
    const sy = e.clientY - win.offsetTop;
    document.body.classList.add("dragging");
    const move = (m: MouseEvent) => {
      win.style.left = Math.min(window.innerWidth - 60, Math.max(-w + 80, m.clientX - sx)) + "px";
      win.style.top = Math.min(window.innerHeight - 40, Math.max(0, m.clientY - sy)) + "px";
    };
    const up = () => { document.removeEventListener("mousemove", move); document.removeEventListener("mouseup", up); document.body.classList.remove("dragging"); };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  });
  new ResizeObserver(() => plots.resize(body)).observe(body);
  return f;
}

async function tileFigures() {
  if (isDesktop) {
    const n = await tileWindows("figure-");
    if (!n) toast("No figures are open.");
    return;
  }
  // Browser: tile the floating figure panels over the desk.
  const list = [...figures.values()];
  if (!list.length) { toast("No figures are open."); return; }
  const desk = $("desk").getBoundingClientRect();
  const cols = Math.ceil(Math.sqrt(list.length));
  const rows = Math.ceil(list.length / cols);
  const w = Math.floor(desk.width / cols);
  const h = Math.floor(desk.height / rows);
  list.forEach((f, i) => {
    f.win.classList.remove("max");
    Object.assign(f.win.style, { left: desk.left + (i % cols) * w + "px", top: desk.top + Math.floor(i / cols) * h + "px", width: w - 4 + "px", height: h - 4 + "px" });
  });
}

/** A desktop figure window: one figure filling the window, updated from the main window. */
async function bootFigure() {
  const id = Number(query.get("figure"));
  document.body.classList.add("figure-window");
  const host = document.createElement("div");
  host.className = "figure-solo";
  document.body.appendChild(host);
  applyTheme();
  const draw = (fig: Figure) => {
    setWindowTitle(figureTitle(fig));
    plots.render(host, fig, dark);
  };
  new ResizeObserver(() => plots.resize(host)).observe(host);
  await onAppEvent("figure-data", (p: { figures: Figure[]; closed: number[] }) => {
    const fig = p.figures.find((f) => f.id === id);
    if (fig) draw(fig);
  });
  await onWindowClose(async () => {
    await broadcast("figure-closed", id);
    await call("close_figure", { id }).catch(() => undefined);
  });
  try {
    const st: any = await call("state");
    const fig = (st.figures as Figure[]).find((f) => f.id === id);
    if (fig) draw(fig);
  } catch { /* the engine may be busy; updates arrive as events */ }
}

function focusFigure(f: FigWin) {
  f.win.style.zIndex = String(++figZ);
  for (const o of figures.values()) o.win.classList.toggle("focused", o === f);
}

// ---------------------------------------------------------------- layout
function initSplitters() {
  const root = document.documentElement;
  const saved = store.get("layout");
  if (saved) {
    try { for (const [k, v] of Object.entries(JSON.parse(saved))) root.style.setProperty(k, String(v)); } catch { /* ignore */ }
  }
  document.querySelectorAll<HTMLElement>(".splitter").forEach((sp) => {
    sp.addEventListener("mousedown", (e) => {
      e.preventDefault();
      const pane = $(sp.dataset.pane!);
      const vertical = sp.classList.contains("v");
      const side = sp.dataset.side;
      const start = vertical ? e.clientX : e.clientY;
      const startSize = vertical ? pane.offsetWidth : pane.offsetHeight;
      const parentSize = vertical ? 0 : (pane.parentElement as HTMLElement).offsetHeight;
      sp.classList.add("drag");
      document.body.classList.add("dragging");
      const move = (m: MouseEvent) => {
        const delta = (vertical ? m.clientX : m.clientY) - start;
        if (vertical) {
          const wpx = Math.max(160, Math.min(700, side === "right" ? startSize - delta : startSize + delta));
          root.style.setProperty(sp.dataset.pane === "pane-files" ? "--left-w" : "--right-w", wpx + "px");
        } else {
          const hpx = Math.max(80, Math.min(parentSize - 80, startSize + delta));
          root.style.setProperty(sp.dataset.pane === "pane-editor" ? "--editor-h" : "--ws-h", (100 * hpx / parentSize).toFixed(2) + "%");
        }
      };
      const up = () => {
        sp.classList.remove("drag");
        document.body.classList.remove("dragging");
        document.removeEventListener("mousemove", move);
        document.removeEventListener("mouseup", up);
        const vars = ["--left-w", "--right-w", "--editor-h", "--ws-h"];
        store.set("layout", JSON.stringify(Object.fromEntries(vars.map((v) => [v, root.style.getPropertyValue(v)]).filter(([, v]) => v))));
      };
      document.addEventListener("mousemove", move);
      document.addEventListener("mouseup", up);
    });
  });
}

function resetLayout() {
  const root = document.documentElement;
  for (const v of ["--left-w", "--right-w", "--editor-h", "--ws-h"]) root.style.removeProperty(v);
  store.set("layout", "{}");
  for (const id of ["pane-files", "pane-workspace", "pane-history"]) $(id).hidden = false;
  syncPaneVisibility();
}

function togglePane(id: string) {
  const el = $(id);
  el.hidden = !el.hidden;
  syncPaneVisibility();
}

function syncPaneVisibility() {
  const files = $("pane-files").hidden;
  (document.querySelector('.splitter[data-pane="pane-files"]') as HTMLElement).hidden = files;
  const ws = $("pane-workspace").hidden;
  const hist = $("pane-history").hidden;
  $("pane-right").hidden = ws && hist;
  (document.querySelector('.splitter[data-pane="pane-right"]') as HTMLElement).hidden = ws && hist;
  (document.querySelector('.splitter[data-pane="pane-workspace"]') as HTMLElement).hidden = ws || hist;
  $("pane-workspace").style.flex = hist ? "1" : "";
}

// ---------------------------------------------------------------- misc actions
async function setPath() {
  const p = await pickFolder(cwd);
  if (p) execute(`addpath('${p.replace(/'/g, "''")}')`, true);
}

async function importData() {
  const p = await pickOpenFile(cwd);
  if (!p) return;
  const base = baseName(p).replace(/\.[^.]+$/, "").replace(/[^A-Za-z0-9_]/g, "_");
  const name = /^[A-Za-z]/.test(base) ? base : "data_" + base;
  execute(`${name} = readmatrix('${p.replace(/'/g, "''")}')`, true);
}

function showHelp() {
  appendText(
    [
      "BeanyLab — quick help",
      "  Command Window:  Enter runs · Shift+Enter new line · ↑/↓ history · Tab completes · Ctrl+C stops",
      "  Editor:          F5 run file · Ctrl+Enter run section · F9 run selection · Ctrl+S save · Ctrl+R/T comment",
      "  Sections:        start a line with %% to split a script into runnable sections",
      "  help name        shows the leading comment block of name.m",
      "  which name / exist name / who / whos / clc / clear / format long|short",
      "",
    ].join("\n") + "\n",
    "info",
  );
}

type MenuItem = { label: string; icon: string; run: () => void };
function menu(e: MouseEvent, items: MenuItem[]) {
  document.querySelectorAll(".menu").forEach((m) => m.remove());
  const m = document.createElement("div");
  m.className = "menu";
  for (const it of items) {
    const b = document.createElement("button");
    b.innerHTML = `${icon(it.icon, 15)}<span></span>`;
    (b.querySelector("span") as HTMLElement).textContent = it.label;
    b.onclick = () => { m.remove(); it.run(); };
    m.appendChild(b);
  }
  m.style.left = e.clientX + "px";
  m.style.top = e.clientY + "px";
  document.body.appendChild(m);
  setTimeout(() => document.addEventListener("mousedown", function off(ev) {
    if (!m.contains(ev.target as Node)) { m.remove(); document.removeEventListener("mousedown", off); }
  }), 0);
}

let toastTimer = 0;
function toast(msg: string) {
  document.querySelectorAll(".toast").forEach((t) => t.remove());
  const t = document.createElement("div");
  t.className = "toast";
  t.textContent = msg;
  document.body.appendChild(t);
  clearTimeout(toastTimer);
  toastTimer = window.setTimeout(() => t.remove(), 3200);
}

// ---------------------------------------------------------------- paths
const SEP = /[\\/]/;
function baseName(p: string) { const parts = p.split(SEP); return parts[parts.length - 1]; }
function dirName(p: string) { const i = Math.max(p.lastIndexOf("\\"), p.lastIndexOf("/")); return i > 0 ? p.slice(0, i) : p; }
function joinPath(dir: string, name: string) { return dir.replace(/[\\/]+$/, "") + (dir.includes("\\") ? "\\" : "/") + name; }
function norm(p: string) { return p.replace(/[\\/]+$/, "").replace(/\//g, "\\").toLowerCase(); }
function samePath(a: string, b: string) { return norm(a) === norm(b); }
function sameDir(a: string, b: string) { return norm(a) === norm(b); }

// ---------------------------------------------------------------- global keys + wiring
document.addEventListener("keydown", (e) => {
  if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "n") { e.preventDefault(); newScript(); }
  else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "o") { e.preventDefault(); openFileDialog(); }
  else if (e.key === "F5" && (activeTab?.kind === "code" || activeTab?.kind === "model")) { e.preventDefault(); runActive(); }
  else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s" && activeTab?.kind === "model") { e.preventDefault(); saveActive(false); }
});
window.addEventListener("beforeunload", (e) => {
  if (tabs.some(isDirty)) { e.preventDefault(); }
});

function wireChrome() {
  $("app-icon").innerHTML = APP_ICON;
  $("empty-icon").innerHTML = icon("script", 56);
  $("nav-back").innerHTML = icon("back", 17);
  $("nav-forward").innerHTML = icon("forward", 17);
  $("nav-up").innerHTML = icon("folderUp", 17);
  $("nav-browse").innerHTML = icon("open", 17);
  $("files-refresh").innerHTML = icon("refresh", 14);
  $("cmd-clear").innerHTML = icon("clear", 14);
  $("nav-back").onclick = () => { const p = navBack.pop(); if (p) { navFwd.push(cwd); setCwd(p, false, false); } };
  $("nav-forward").onclick = () => { const p = navFwd.pop(); if (p) { navBack.push(cwd); setCwd(p, false, false); } };
  $("nav-up").onclick = () => { const up = dirName(cwd); if (up && up !== cwd) setCwd(up.endsWith(":") ? up + "\\" : up); };
  $("nav-browse").onclick = async () => { const p = await pickFolder(cwd); if (p) setCwd(p); };
  $("files-refresh").onclick = () => refreshFiles();
  $("cmd-clear").onclick = () => clearCommand();
  const cwdInput = $<HTMLInputElement>("cwd-input");
  cwdInput.addEventListener("keydown", (e) => { if (e.key === "Enter") { setCwd(cwdInput.value.trim()); input.focus(); } if (e.key === "Escape") cwdInput.value = cwd; });
}

async function boot() {
  if (figureWindow) { await bootFigure(); return; }
  wireChrome();
  renderRibbon();
  initSplitters();
  syncPaneVisibility();
  applyTheme();
  renderTabs();
  await onStream(
    (s) => appendText(s),
    (f) => applyFigures(f.figures, f.closed),
  );
  await onEvent(onEngineEvent);
  wireModelEvents();
  if (modelWindow) document.body.classList.add("model-window");
  try {
    blockLib.push(...(await call<BlockDef[]>("block_library")));
    const st: any = await call("state");
    cwd = st.cwd;
    $<HTMLInputElement>("cwd-input").value = cwd;
    const last = store.get("cwd");
    if (last && last !== cwd) await setCwd(last, false, false);
    renderWorkspace(st.workspace);
    applyFigures(st.figures, []);
    for (const h of st.history) cmdHistory.push(h);
    renderHistory();
    refreshFiles();
  } catch (e) {
    appendText(`Could not reach the BeanyLab engine: ${e}\n`, "err");
  }
  if (modelWindow) {
    const path = query.get("model");
    if (path) await openPath(path);
    else {
      const name = query.get("newmodel") || "untitled";
      newModelTab(null, `${name}.bmdl`, emptyModel(name));
    }
    updateModelWindowTitle();
    guardClose(() => !tabs[0] || !isDirty(tabs[0]) || confirm(`${tabs[0].name} has unsaved changes. Close it anyway?`));
    return;
  }
  appendText(`BeanyLab 0.1 — clean-room numeric computing environment.\nType commands at >> or open a .m file. Type "help" for keyboard shortcuts.\n\n`, "info");
  input.focus();
}

boot();

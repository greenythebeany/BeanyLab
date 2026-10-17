// Block-diagram model editor: palette, canvas, lines, parameter dialogs, subsystems.

export interface ParamDef { name: string; label: string; default: string; kind: string }
export interface BlockDef { type: string; category: string; description: string; params: ParamDef[] }
export interface PortRef { block: string; port: number }
export interface Line { src: PortRef; dst: PortRef; points?: [number, number][] }
export interface BlockSpec { id: string; type: string; name: string; x: number; y: number; w: number; h: number; params: Record<string, string>; system?: Model }
export interface Solver { solver: string; stop_time: string; fixed_step: string; max_step: string; rel_tol: string; abs_tol: string }
export interface Model { version: number; name: string; solver: Solver; blocks: BlockSpec[]; lines: Line[] }

export interface EditorHost {
  library: BlockDef[];
  ports(blocks: BlockSpec[]): Promise<[number, number][]>;
  onChange(): void;
  run(): void;
  stop(): void;
  save(): void;
  toast(msg: string): void;
}

export function emptyModel(name: string): Model {
  return { version: 1, name, solver: defaultSolver(), blocks: [], lines: [] };
}

function defaultSolver(): Solver {
  return { solver: "auto", stop_time: "10", fixed_step: "auto", max_step: "auto", rel_tol: "1e-3", abs_tol: "1e-6" };
}

const SVGNS = "http://www.w3.org/2000/svg";
const GRID = 10;
const snap = (v: number) => Math.round(v / GRID) * GRID;

function el<K extends keyof SVGElementTagNameMap>(tag: K, attrs: Record<string, string | number> = {}, parent?: Element): SVGElementTagNameMap[K] {
  const e = document.createElementNS(SVGNS, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  if (parent) parent.appendChild(e);
  return e;
}

function esc(s: string) {
  return s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** "[1 2 3]" -> "s^2 + 2s + 3" for transfer-function glyphs; anything else stays as typed. */
function polyText(src: string, v: string): string {
  const nums = src.replace(/[[\],]/g, " ").trim().split(/\s+/).map(Number);
  if (!nums.length || nums.some((x) => !Number.isFinite(x))) return src;
  const n = nums.length - 1;
  const parts: string[] = [];
  nums.forEach((c, i) => {
    if (c === 0) return;
    const p = n - i;
    const mag = Math.abs(c);
    const coef = mag === 1 && p > 0 ? "" : String(+mag.toPrecision(4));
    const term = p === 0 ? coef : p === 1 ? `${coef}${v}` : `${coef}${v}^${p}`;
    parts.push(parts.length === 0 ? (c < 0 ? "-" : "") + term : (c < 0 ? " - " : " + ") + term);
  });
  return parts.join("") || "0";
}

/** Inner drawing of a block (SVG markup in a w×h box). */
function glyph(b: BlockSpec, display?: number[]): string {
  const { w, h } = b;
  const cx = w / 2;
  const cy = h / 2;
  const P = b.params;
  const txt = (s: string, size = 12, y = cy) => `<text x="${cx}" y="${y}" class="bt" font-size="${size}" dominant-baseline="middle" text-anchor="middle">${esc(s)}</text>`;
  const frac = (n: string, d: string, v: string) =>
    `${txt(polyText(n, v), 11, cy - 9)}<line x1="${w * 0.15}" y1="${cy}" x2="${w * 0.85}" y2="${cy}" class="bl"/>${txt(polyText(d, v), 11, cy + 10)}`;
  const path = (d: string) => `<path d="${d}" class="bl" fill="none"/>`;
  const sx = (f: number) => w * 0.15 + f * w * 0.7;
  const sy = (f: number) => h * 0.8 - f * h * 0.6;
  switch (b.type) {
    case "Constant": return txt(P.value ?? "1");
    case "Step": return path(`M${sx(0)},${sy(0)} H${sx(0.45)} V${sy(1)} H${sx(1)}`);
    case "Ramp": return path(`M${sx(0)},${sy(0)} H${sx(0.3)} L${sx(1)},${sy(1)}`);
    case "SineWave": case "SignalGenerator": {
      let d = "";
      for (let i = 0; i <= 24; i++) d += `${i ? "L" : "M"}${sx(i / 24)},${sy(0.5 + 0.45 * Math.sin((i / 24) * 2 * Math.PI))}`;
      return path(d);
    }
    case "PulseGenerator": return path(`M${sx(0)},${sy(0)} H${sx(0.15)} V${sy(1)} H${sx(0.4)} V${sy(0)} H${sx(0.6)} V${sy(1)} H${sx(0.85)} V${sy(0)} H${sx(1)}`);
    case "Clock": return `<circle cx="${cx}" cy="${cy}" r="${Math.min(w, h) * 0.3}" class="bl" fill="none"/>` + path(`M${cx},${cy} V${cy - h * 0.18} M${cx},${cy} H${cx + w * 0.12}`);
    case "FromWorkspace": return txt("[t, y]", 11);
    case "RandomNumber": return path(`M${sx(0)},${sy(0.5)} L${sx(0.1)},${sy(0.9)} L${sx(0.2)},${sy(0.2)} L${sx(0.32)},${sy(0.7)} L${sx(0.45)},${sy(0.1)} L${sx(0.58)},${sy(0.8)} L${sx(0.7)},${sy(0.35)} L${sx(0.85)},${sy(0.95)} L${sx(1)},${sy(0.4)}`);
    case "Scope": return `<rect x="${w * 0.15}" y="${h * 0.18}" width="${w * 0.7}" height="${h * 0.55}" rx="3" class="scr"/>` + path(`M${w * 0.2},${h * 0.55} Q${w * 0.35},${h * 0.2} ${w * 0.5},${h * 0.45} T${w * 0.8},${h * 0.35}`);
    case "ToWorkspace": return txt(P.variable ?? "simout", 11);
    case "Display": return txt(display ? display.map((v) => +v.toPrecision(5)).join(", ") : "—", 12);
    case "Inport": case "Outport": return txt(P.port ?? "1", 13);
    case "Terminator": return path(`M${w * 0.3},${cy} H${w * 0.6} M${w * 0.6},${h * 0.25} V${h * 0.75}`);
    case "StopSimulation": return txt("STOP", 11);
    case "Gain": return txt(P.gain ?? "1", 12, cy);
    case "Sum": return txt(P.signs ?? "+-", 12, cy);
    case "Product": return txt((P.ops ?? "**").replace(/\*/g, "×").replace(/\//g, "÷"), 14);
    case "Bias": return txt(`u + ${P.bias ?? "0"}`, 11);
    case "Abs": return txt("|u|", 13);
    case "Sign": return txt("sign", 12);
    case "MathFunction": case "TrigFunction": return txt(P.function ?? (b.type === "MathFunction" ? "exp" : "sin"), 12);
    case "MinMax": return txt(P.function ?? "min", 12);
    case "DotProduct": return txt("u1·u2", 11);
    case "Fcn": return txt(`f(u) = ${P.expression ?? "sin(u(1))"}`, 10);
    case "UserFunction": return txt(`${P.function ?? "myfun"}(u)`, 11);
    case "Integrator": return txt("1/s", 14);
    case "Derivative": return txt("du/dt", 12);
    case "TransferFcn": return frac(P.num ?? "[1]", P.den ?? "[1 1]", "s");
    case "DiscreteTransferFcn": return frac(P.num ?? "[1]", P.den ?? "[1 0.5]", "z");
    case "StateSpace": return txt("x' = Ax+Bu", 10, cy - 7) + txt("y = Cx+Du", 10, cy + 8);
    case "DiscreteStateSpace": return txt("x⁺ = Ax+Bu", 10, cy - 7) + txt("y = Cx+Du", 10, cy + 8);
    case "ZeroPole": return txt("(s-z)/(s-p)", 10);
    case "TransportDelay": return txt("e^(-sT)", 11);
    case "PID": return txt("PID(s)", 12);
    case "DiscretePID": return txt("PID(z)", 12);
    case "UnitDelay": return txt("1/z", 14);
    case "DiscreteIntegrator": return txt("KTs/(z-1)", 10);
    case "ZeroOrderHold": return path(`M${sx(0)},${sy(0.1)} H${sx(0.25)} V${sy(0.45)} H${sx(0.5)} V${sy(0.8)} H${sx(0.75)} V${sy(0.6)} H${sx(1)}`);
    case "Saturation": return path(`M${sx(0)},${sy(0.15)} H${sx(0.3)} L${sx(0.7)},${sy(0.85)} H${sx(1)}`);
    case "DeadZone": return path(`M${sx(0)},${sy(0.1)} L${sx(0.35)},${sy(0.5)} H${sx(0.65)} L${sx(1)},${sy(0.9)}`);
    case "Relay": return path(`M${sx(0)},${sy(0.15)} H${sx(0.6)} V${sy(0.85)} H${sx(1)} M${sx(0.4)},${sy(0.15)} V${sy(0.85)} H${sx(0.6)}`);
    case "RateLimiter": return path(`M${sx(0)},${sy(0.1)} L${sx(0.6)},${sy(0.9)} H${sx(1)}`);
    case "Quantizer": return path(`M${sx(0)},${sy(0)} H${sx(0.2)} V${sy(0.25)} H${sx(0.4)} V${sy(0.5)} H${sx(0.6)} V${sy(0.75)} H${sx(0.8)} V${sy(1)} H${sx(1)}`);
    case "Lookup1D": return path(`M${sx(0)},${sy(0.1)} C${sx(0.4)},${sy(0.1)} ${sx(0.6)},${sy(0.9)} ${sx(1)},${sy(0.9)}`);
    case "Mux": case "Demux": return "";
    case "Switch": return path(`M${w * 0.15},${h * 0.25} H${w * 0.45} L${w * 0.75},${cy} H${w * 0.9} M${w * 0.15},${h * 0.75} H${w * 0.45}`);
    case "RelationalOperator": return txt(P.op ?? "<=", 14);
    case "LogicalOperator": return txt(P.op ?? "AND", 11);
    case "Subsystem": return txt("Subsystem", 11);
    default: return txt(b.type, 10);
  }
}

let uid = 0;
const newId = (prefix: string) => `${prefix}${Date.now().toString(36)}${(uid++).toString(36)}`;

export class ModelEditor {
  root: Model;
  /** Path of subsystem ids from the root to the open level. */
  stack: string[] = [];
  private ports = new Map<string, [number, number]>();
  private selection = new Set<string>();
  private selLine: number | null = null;
  private view = { x: 0, y: 0, k: 1 };
  private svg!: SVGSVGElement;
  private layer!: SVGGElement;
  private host: EditorHost;
  private wrap: HTMLElement;
  private undo: string[] = [];
  private redo: string[] = [];
  private displays = new Map<string, number[]>();
  private errorBlock: string | null = null;
  private clipboard: { blocks: BlockSpec[]; lines: Line[] } | null = null;
  private crumbs!: HTMLElement;

  constructor(wrap: HTMLElement, model: Model, host: EditorHost) {
    this.wrap = wrap;
    this.root = model;
    this.host = host;
    this.build();
    this.refreshPorts().then(() => this.fit());
  }

  /** The model level currently shown. */
  get level(): Model {
    let m = this.root;
    for (const id of this.stack) {
      const b = m.blocks.find((x) => x.id === id);
      if (!b || !b.system) break;
      m = b.system;
    }
    return m;
  }

  private snapshot() {
    this.undo.push(JSON.stringify(this.root));
    if (this.undo.length > 200) this.undo.shift();
    this.redo.length = 0;
  }

  private changed() {
    this.host.onChange();
    this.render();
  }

  // ------------------------------------------------------------- layout

  private build() {
    this.wrap.innerHTML = `
      <div class="me-toolbar">
        <button class="ea primary" data-a="run" title="Run simulation (Ctrl+T)"><span>▶ Run</span></button>
        <button class="ea stop-btn" data-a="stop" title="Stop simulation"><span>■ Stop</span></button>
        <label class="me-field">Stop time <input data-s="stop_time" size="6"></label>
        <label class="me-field">Solver
          <select data-s="solver">
            <option value="auto">auto</option><option value="ode45">ode45 (variable)</option><option value="ode23">ode23 (variable)</option>
            <option value="ode4">ode4 (RK4, fixed)</option><option value="ode2">ode2 (Heun, fixed)</option><option value="ode1">ode1 (Euler, fixed)</option><option value="discrete">discrete</option>
          </select></label>
        <button class="ea" data-a="settings" title="Solver settings">Settings…</button>
        <span class="me-sep"></span>
        <button class="ea" data-a="save" title="Save (Ctrl+S)">Save</button>
        <button class="ea" data-a="fit" title="Fit to view">Fit</button>
        <button class="ea" data-a="undo" title="Undo (Ctrl+Z)">Undo</button>
        <button class="ea" data-a="redo" title="Redo (Ctrl+Y)">Redo</button>
        <span class="me-crumbs"></span>
      </div>
      <div class="me-body">
        <div class="me-palette">
          <input class="me-search" placeholder="Search blocks">
          <div class="me-lib"></div>
        </div>
        <div class="me-canvas" tabindex="0"></div>
      </div>`;
    this.crumbs = this.wrap.querySelector(".me-crumbs") as HTMLElement;
    this.wrap.querySelectorAll<HTMLButtonElement>("[data-a]").forEach((b) => {
      b.onclick = () => {
        const a = b.dataset.a;
        if (a === "run") this.host.run();
        else if (a === "stop") this.host.stop();
        else if (a === "save") this.host.save();
        else if (a === "fit") this.fit();
        else if (a === "undo") this.doUndo();
        else if (a === "redo") this.doRedo();
        else if (a === "settings") this.solverDialog();
      };
    });
    const st = this.wrap.querySelector<HTMLInputElement>('[data-s="stop_time"]')!;
    st.value = this.root.solver.stop_time;
    st.onchange = () => { this.snapshot(); this.root.solver.stop_time = st.value.trim() || "10"; this.host.onChange(); };
    const so = this.wrap.querySelector<HTMLSelectElement>('[data-s="solver"]')!;
    so.value = this.root.solver.solver;
    so.onchange = () => { this.snapshot(); this.root.solver.solver = so.value; this.host.onChange(); };
    this.buildPalette("");
    const search = this.wrap.querySelector<HTMLInputElement>(".me-search")!;
    search.oninput = () => this.buildPalette(search.value.trim().toLowerCase());
    this.buildCanvas();
  }

  private buildPalette(filter: string) {
    const lib = this.wrap.querySelector(".me-lib") as HTMLElement;
    lib.innerHTML = "";
    const cats = new Map<string, BlockDef[]>();
    for (const d of this.host.library) {
      if (filter && !d.type.toLowerCase().includes(filter) && !d.description.toLowerCase().includes(filter)) continue;
      if (!cats.has(d.category)) cats.set(d.category, []);
      cats.get(d.category)!.push(d);
    }
    for (const [cat, defs] of cats) {
      const sec = document.createElement("details");
      sec.open = true;
      sec.innerHTML = `<summary>${esc(cat)}</summary>`;
      const grid = document.createElement("div");
      grid.className = "me-items";
      for (const d of defs) {
        const item = document.createElement("div");
        item.className = "me-item";
        item.draggable = true;
        item.title = d.description;
        const sample: BlockSpec = { id: "", type: d.type, name: d.type, x: 0, y: 0, w: 46, h: 30, params: Object.fromEntries(d.params.map((p) => [p.name, p.default])) };
        item.innerHTML = `<svg width="46" height="30" class="me-thumb">${shapeMarkup(sample)}${glyph(sample)}</svg><span>${esc(label(d.type))}</span>`;
        item.ondragstart = (e) => { e.dataTransfer!.setData("text/beany-block", d.type); e.dataTransfer!.effectAllowed = "copy"; };
        item.ondblclick = () => {
          const c = this.toModel(this.svg.clientWidth / 2, this.svg.clientHeight / 2);
          this.addBlock(d.type, c.x, c.y);
        };
        grid.appendChild(item);
      }
      sec.appendChild(grid);
      lib.appendChild(sec);
    }
  }

  private buildCanvas() {
    const box = this.wrap.querySelector(".me-canvas") as HTMLElement;
    this.svg = el("svg", { class: "me-svg" }, box);
    const defs = el("defs", {}, this.svg);
    defs.innerHTML = `
      <pattern id="me-grid" width="${GRID}" height="${GRID}" patternUnits="userSpaceOnUse"><circle cx="1" cy="1" r="0.8" class="me-dot"/></pattern>
      <marker id="me-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" class="me-arrowhead"/></marker>`;
    this.layer = el("g", {}, this.svg);
    box.addEventListener("dragover", (e) => { e.preventDefault(); e.dataTransfer!.dropEffect = "copy"; });
    box.addEventListener("drop", (e) => {
      e.preventDefault();
      const type = e.dataTransfer!.getData("text/beany-block");
      if (!type) return;
      const r = this.svg.getBoundingClientRect();
      const p = this.toModel(e.clientX - r.left, e.clientY - r.top);
      this.addBlock(type, p.x, p.y);
    });
    this.svg.addEventListener("wheel", (e) => {
      e.preventDefault();
      const r = this.svg.getBoundingClientRect();
      const mx = e.clientX - r.left;
      const my = e.clientY - r.top;
      const k = Math.min(4, Math.max(0.2, this.view.k * (e.deltaY < 0 ? 1.1 : 1 / 1.1)));
      this.view.x = mx - ((mx - this.view.x) * k) / this.view.k;
      this.view.y = my - ((my - this.view.y) * k) / this.view.k;
      this.view.k = k;
      this.applyView();
    }, { passive: false });
    this.svg.addEventListener("mousedown", (e) => this.onBackgroundDown(e));
    box.addEventListener("keydown", (e) => this.onKey(e));
    new ResizeObserver(() => this.applyView()).observe(box);
  }

  private applyView() {
    this.layer.setAttribute("transform", `translate(${this.view.x},${this.view.y}) scale(${this.view.k})`);
  }

  toModel(px: number, py: number) {
    return { x: (px - this.view.x) / this.view.k, y: (py - this.view.y) / this.view.k };
  }

  fit() {
    const bl = this.level.blocks;
    const W = this.svg.clientWidth || 800;
    const H = this.svg.clientHeight || 500;
    if (!bl.length) {
      this.view = { x: 40, y: 40, k: 1 };
    } else {
      const x0 = Math.min(...bl.map((b) => b.x)) - 60;
      const y0 = Math.min(...bl.map((b) => b.y)) - 50;
      const x1 = Math.max(...bl.map((b) => b.x + b.w)) + 60;
      const y1 = Math.max(...bl.map((b) => b.y + b.h)) + 70;
      const k = Math.min(1.6, Math.max(0.3, Math.min(W / (x1 - x0), H / (y1 - y0))));
      this.view = { k, x: (W - (x1 - x0) * k) / 2 - x0 * k, y: (H - (y1 - y0) * k) / 2 - y0 * k };
    }
    this.applyView();
    this.render();
  }

  // ------------------------------------------------------------- ports

  async refreshPorts() {
    const all: BlockSpec[] = [];
    const walk = (m: Model) => { for (const b of m.blocks) { all.push(b); if (b.system) walk(b.system); } };
    walk(this.root);
    const res = await this.host.ports(all);
    all.forEach((b, i) => this.ports.set(b.id, res[i] ?? [1, 1]));
    this.render();
  }

  private portsOf(b: BlockSpec): [number, number] {
    return this.ports.get(b.id) ?? [1, 1];
  }

  private portPos(b: BlockSpec, out: boolean, k: number): [number, number] {
    const [ni, no] = this.portsOf(b);
    const n = out ? no : ni;
    return [out ? b.x + b.w : b.x, b.y + (b.h * (k + 0.5)) / Math.max(1, n)];
  }

  // ------------------------------------------------------------- render

  setDisplays(values: { id: string; value: number[] }[]) {
    this.displays = new Map(values.map((v) => [v.id, v.value]));
    this.render();
  }

  setError(blockPath: string | null) {
    this.errorBlock = blockPath;
    if (blockPath) {
      // Open the subsystem that holds the failing block.
      const parts = blockPath.split("/");
      this.stack = parts.slice(0, -1);
    }
    this.render();
  }

  render() {
    const g = this.layer;
    g.innerHTML = "";
    el("rect", { x: -5000, y: -5000, width: 10000, height: 10000, fill: "url(#me-grid)", class: "me-bg" }, g);
    const m = this.level;
    const byId = new Map(m.blocks.map((b) => [b.id, b]));
    const prefix = this.stack.length ? this.stack.join("/") + "/" : "";
    // Lines under blocks.
    m.lines.forEach((l, i) => {
      const s = byId.get(l.src.block);
      const d = byId.get(l.dst.block);
      if (!s || !d) return;
      const [x1, y1] = this.portPos(s, true, l.src.port - 1);
      const [x2, y2] = this.portPos(d, false, l.dst.port - 1);
      const path = route(x1, y1, x2, y2, s, d);
      const hit = el("path", { d: path, class: "me-hit" }, g);
      el("path", { d: path, class: "me-line" + (this.selLine === i ? " sel" : ""), "marker-end": "url(#me-arrow)" }, g);
      hit.addEventListener("mousedown", (e) => {
        e.stopPropagation();
        this.selection.clear();
        this.selLine = i;
        this.render();
        this.focus();
      });
    });
    for (const b of m.blocks) {
      const grp = el("g", { transform: `translate(${b.x},${b.y})`, class: "me-block" + (this.selection.has(b.id) ? " sel" : "") + (this.errorBlock && (this.errorBlock === prefix + b.id || this.errorBlock.startsWith(prefix + b.id + "/")) ? " err" : "") }, g);
      grp.innerHTML = shapeMarkup(b) + glyph(b, this.displays.get(prefix + b.id));
      const name = el("text", { x: b.w / 2, y: b.h + 14, class: "me-name", "text-anchor": "middle" }, grp);
      name.textContent = b.name;
      const [ni, no] = this.portsOf(b);
      for (let k = 0; k < ni; k++) {
        const y = (b.h * (k + 0.5)) / ni;
        const p = el("path", { d: `M-1,${y - 4} L5,${y} L-1,${y + 4}`, class: "me-port in" }, grp);
        p.dataset.port = String(k + 1);
        const hit = el("circle", { cx: 0, cy: y, r: 8, class: "me-port-hit" }, grp);
        hit.dataset.inport = `${b.id}:${k + 1}`;
      }
      for (let k = 0; k < no; k++) {
        const y = (b.h * (k + 0.5)) / no;
        el("path", { d: `M${b.w},${y - 4} L${b.w + 6},${y} L${b.w},${y + 4}`, class: "me-port out" }, grp);
        const hit = el("circle", { cx: b.w + 3, cy: y, r: 8, class: "me-port-hit out" }, grp);
        hit.addEventListener("mousedown", (e) => { e.stopPropagation(); this.startWire(b, k + 1, e); });
      }
      grp.addEventListener("mousedown", (e) => this.onBlockDown(b, e));
      grp.addEventListener("dblclick", (e) => { e.stopPropagation(); this.openBlock(b); });
      grp.addEventListener("contextmenu", (e) => { e.preventDefault(); this.blockMenu(b, e); });
    }
    this.renderCrumbs();
  }

  private renderCrumbs() {
    const names: string[] = [this.root.name || "model"];
    let m = this.root;
    for (const id of this.stack) {
      const b = m.blocks.find((x) => x.id === id);
      if (!b) break;
      names.push(b.name);
      m = b.system!;
    }
    this.crumbs.innerHTML = "";
    names.forEach((n, i) => {
      const a = document.createElement("button");
      a.className = "crumb" + (i === names.length - 1 ? " cur" : "");
      a.textContent = n;
      a.onclick = () => { this.stack = this.stack.slice(0, i); this.selection.clear(); this.fit(); };
      this.crumbs.appendChild(a);
      if (i < names.length - 1) this.crumbs.appendChild(document.createTextNode(" › "));
    });
  }

  // ------------------------------------------------------------- editing

  focus() {
    (this.wrap.querySelector(".me-canvas") as HTMLElement).focus();
  }

  private uniqueName(base: string) {
    const names = new Set(this.level.blocks.map((b) => b.name));
    if (!names.has(base)) return base;
    let i = 1;
    while (names.has(`${base}${i}`)) i++;
    return `${base}${i}`;
  }

  async addBlock(type: string, x: number, y: number) {
    const def = this.host.library.find((d) => d.type === type);
    if (!def) return;
    this.snapshot();
    const size: Record<string, [number, number]> = {
      Sum: [40, 40], Gain: [60, 44], Mux: [10, 60], Demux: [10, 60], Scope: [56, 50], Inport: [40, 26], Outport: [40, 26],
      TransferFcn: [100, 54], DiscreteTransferFcn: [110, 54], Fcn: [160, 44], Subsystem: [110, 70], Display: [90, 34], Terminator: [30, 30],
    };
    const [w, h] = size[type] ?? [70, 50];
    const m = this.level;
    const b: BlockSpec = {
      id: newId("b"), type, name: this.uniqueName(label(type)), x: snap(x - w / 2), y: snap(y - h / 2), w, h,
      params: Object.fromEntries(def.params.map((p) => [p.name, p.default])),
    };
    if (type === "Inport" || type === "Outport") b.params.port = String(m.blocks.filter((o) => o.type === type).length + 1);
    if (type === "Subsystem") {
      b.system = {
        version: 1, name: b.name, solver: defaultSolver(),
        blocks: [
          { id: newId("b"), type: "Inport", name: "In1", x: 40, y: 100, w: 40, h: 26, params: { port: "1" } },
          { id: newId("b"), type: "Outport", name: "Out1", x: 300, y: 100, w: 40, h: 26, params: { port: "1" } },
        ],
        lines: [],
      };
      b.system.lines.push({ src: { block: b.system.blocks[0].id, port: 1 }, dst: { block: b.system.blocks[1].id, port: 1 } });
    }
    m.blocks.push(b);
    this.selection = new Set([b.id]);
    this.selLine = null;
    await this.refreshPorts();
    this.changed();
    this.focus();
  }

  private onBlockDown(b: BlockSpec, e: MouseEvent) {
    if (e.button !== 0) return;
    e.stopPropagation();
    this.selLine = null;
    if (e.shiftKey || e.ctrlKey) {
      if (this.selection.has(b.id)) this.selection.delete(b.id);
      else this.selection.add(b.id);
    } else if (!this.selection.has(b.id)) {
      this.selection = new Set([b.id]);
    }
    this.render();
    this.focus();
    const start = { x: e.clientX, y: e.clientY };
    const moving = this.level.blocks.filter((o) => this.selection.has(o.id));
    const orig = moving.map((o) => [o.x, o.y]);
    let moved = false;
    const move = (m: MouseEvent) => {
      const dx = (m.clientX - start.x) / this.view.k;
      const dy = (m.clientY - start.y) / this.view.k;
      if (!moved && Math.abs(dx) + Math.abs(dy) > 2) {
        moved = true;
        this.snapshot();
      }
      if (!moved) return;
      moving.forEach((o, i) => { o.x = snap(orig[i][0] + dx); o.y = snap(orig[i][1] + dy); });
      this.render();
    };
    const up = () => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
      if (moved) this.host.onChange();
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }

  private onBackgroundDown(e: MouseEvent) {
    if (e.button === 2) return;
    this.focus();
    const start = { x: e.clientX, y: e.clientY, vx: this.view.x, vy: this.view.y };
    let moved = false;
    const move = (m: MouseEvent) => {
      if (Math.abs(m.clientX - start.x) + Math.abs(m.clientY - start.y) > 3) moved = true;
      this.view.x = start.vx + m.clientX - start.x;
      this.view.y = start.vy + m.clientY - start.y;
      this.applyView();
    };
    const up = () => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
      if (!moved) {
        this.selection.clear();
        this.selLine = null;
        this.render();
      }
    };
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }

  private startWire(b: BlockSpec, port: number, e: MouseEvent) {
    const [x1, y1] = this.portPos(b, true, port - 1);
    const temp = el("path", { class: "me-line temp" }, this.layer);
    const r = this.svg.getBoundingClientRect();
    const move = (m: MouseEvent) => {
      const p = this.toModel(m.clientX - r.left, m.clientY - r.top);
      temp.setAttribute("d", `M${x1},${y1} L${p.x},${p.y}`);
    };
    const up = async (m: MouseEvent) => {
      document.removeEventListener("mousemove", move);
      document.removeEventListener("mouseup", up);
      temp.remove();
      // Find an input port near the release point.
      const p = this.toModel(m.clientX - r.left, m.clientY - r.top);
      let best: { b: BlockSpec; k: number; d: number } | null = null;
      for (const o of this.level.blocks) {
        const [ni] = this.portsOf(o);
        for (let k = 0; k < ni; k++) {
          const [px, py] = this.portPos(o, false, k);
          const d = Math.hypot(px - p.x, py - p.y);
          if (d < 18 && (!best || d < best.d)) best = { b: o, k: k + 1, d };
        }
        // Dropping on a block body picks its first free input.
        if (!best && p.x >= o.x && p.x <= o.x + o.w && p.y >= o.y && p.y <= o.y + o.h && ni > 0) {
          const used = new Set(this.level.lines.filter((l) => l.dst.block === o.id).map((l) => l.dst.port));
          for (let k = 1; k <= ni; k++) if (!used.has(k)) { best = { b: o, k, d: 0 }; break; }
        }
      }
      if (!best) return;
      this.snapshot();
      const m2 = this.level;
      m2.lines = m2.lines.filter((l) => !(l.dst.block === best!.b.id && l.dst.port === best!.k));
      m2.lines.push({ src: { block: b.id, port }, dst: { block: best.b.id, port: best.k } });
      this.changed();
    };
    move(e);
    document.addEventListener("mousemove", move);
    document.addEventListener("mouseup", up);
  }

  private deleteSelection() {
    const m = this.level;
    if (this.selLine !== null) {
      this.snapshot();
      m.lines.splice(this.selLine, 1);
      this.selLine = null;
      this.changed();
      return;
    }
    if (!this.selection.size) return;
    this.snapshot();
    m.blocks = m.blocks.filter((b) => !this.selection.has(b.id));
    m.lines = m.lines.filter((l) => !this.selection.has(l.src.block) && !this.selection.has(l.dst.block));
    this.selection.clear();
    this.changed();
  }

  private copy() {
    const m = this.level;
    const blocks = m.blocks.filter((b) => this.selection.has(b.id));
    const lines = m.lines.filter((l) => this.selection.has(l.src.block) && this.selection.has(l.dst.block));
    if (blocks.length) this.clipboard = JSON.parse(JSON.stringify({ blocks, lines }));
  }

  private async paste() {
    if (!this.clipboard) return;
    this.snapshot();
    const m = this.level;
    const map = new Map<string, string>();
    const clone = JSON.parse(JSON.stringify(this.clipboard)) as { blocks: BlockSpec[]; lines: Line[] };
    this.selection.clear();
    for (const b of clone.blocks) {
      const id = newId("b");
      map.set(b.id, id);
      b.id = id;
      b.x += 30;
      b.y += 30;
      b.name = this.uniqueName(b.name);
      m.blocks.push(b);
      this.selection.add(id);
    }
    for (const l of clone.lines) m.lines.push({ src: { block: map.get(l.src.block)!, port: l.src.port }, dst: { block: map.get(l.dst.block)!, port: l.dst.port } });
    this.clipboard = { blocks: clone.blocks.map((b) => ({ ...b })), lines: clone.lines };
    await this.refreshPorts();
    this.changed();
  }

  private doUndo() {
    const s = this.undo.pop();
    if (!s) return;
    this.redo.push(JSON.stringify(this.root));
    this.root = JSON.parse(s);
    this.refreshPorts();
    this.changed();
  }

  private doRedo() {
    const s = this.redo.pop();
    if (!s) return;
    this.undo.push(JSON.stringify(this.root));
    this.root = JSON.parse(s);
    this.refreshPorts();
    this.changed();
  }

  private onKey(e: KeyboardEvent) {
    const ctrl = e.ctrlKey || e.metaKey;
    if (e.key === "Delete" || e.key === "Backspace") { e.preventDefault(); this.deleteSelection(); }
    else if (ctrl && e.key.toLowerCase() === "z") { e.preventDefault(); this.doUndo(); }
    else if (ctrl && e.key.toLowerCase() === "y") { e.preventDefault(); this.doRedo(); }
    else if (ctrl && e.key.toLowerCase() === "c") { e.preventDefault(); this.copy(); }
    else if (ctrl && e.key.toLowerCase() === "v") { e.preventDefault(); this.paste(); }
    else if (ctrl && e.key.toLowerCase() === "a") { e.preventDefault(); this.selection = new Set(this.level.blocks.map((b) => b.id)); this.render(); }
    else if (ctrl && e.key.toLowerCase() === "t") { e.preventDefault(); this.host.run(); }
    else if (e.key === "Escape" && this.stack.length) { this.stack.pop(); this.selection.clear(); this.fit(); }
  }

  private blockMenu(b: BlockSpec, e: MouseEvent) {
    document.querySelectorAll(".menu").forEach((m) => m.remove());
    const menu = document.createElement("div");
    menu.className = "menu";
    const items: [string, () => void][] = [
      [b.type === "Subsystem" ? "Open subsystem" : "Parameters…", () => this.openBlock(b)],
      ["Rename…", () => { const n = prompt("Block name", b.name); if (n && n.trim()) { this.snapshot(); b.name = n.trim(); this.changed(); } }],
      ["Delete", () => { this.selection = new Set([b.id]); this.deleteSelection(); }],
    ];
    for (const [text, fn] of items) {
      const btn = document.createElement("button");
      btn.textContent = text;
      btn.onclick = () => { menu.remove(); fn(); };
      menu.appendChild(btn);
    }
    menu.style.left = e.clientX + "px";
    menu.style.top = e.clientY + "px";
    document.body.appendChild(menu);
    setTimeout(() => document.addEventListener("mousedown", function off(ev) {
      if (!menu.contains(ev.target as Node)) { menu.remove(); document.removeEventListener("mousedown", off); }
    }), 0);
  }

  private openBlock(b: BlockSpec) {
    if (b.type === "Subsystem") {
      this.stack.push(b.id);
      this.selection.clear();
      this.selLine = null;
      this.fit();
      return;
    }
    const def = this.host.library.find((d) => d.type === b.type);
    if (!def) return;
    const back = document.createElement("div");
    back.className = "modal-back";
    const fields = def.params.map((p) => {
      const val = esc(b.params[p.name] ?? p.default);
      if (p.kind.startsWith("choice:")) {
        const opts = p.kind.slice(7).split("|").map((o) => `<option ${o === (b.params[p.name] ?? p.default) ? "selected" : ""}>${esc(o)}</option>`).join("");
        return `<label><span>${esc(p.label)}</span><select data-p="${p.name}">${opts}</select></label>`;
      }
      return `<label><span>${esc(p.label)}</span><input data-p="${p.name}" value="${val}" spellcheck="false"></label>`;
    }).join("");
    back.innerHTML = `<div class="modal" role="dialog">
      <div class="modal-head">Block Parameters: ${esc(b.name)}</div>
      <p class="modal-desc">${esc(def.type)} — ${esc(def.description)}</p>
      <label><span>Name</span><input data-name value="${esc(b.name)}"></label>
      ${fields || '<p class="modal-desc">This block has no parameters.</p>'}
      <p class="modal-hint">Numeric fields accept expressions and workspace variables, e.g. <code>2*pi</code> or <code>K</code>.</p>
      <div class="modal-buttons"><button class="ea" data-c>Cancel</button><button class="ea primary" data-ok>OK</button></div>
    </div>`;
    document.body.appendChild(back);
    const close = () => back.remove();
    const ok = async () => {
      this.snapshot();
      const nameInput = back.querySelector<HTMLInputElement>("[data-name]")!;
      if (nameInput.value.trim()) b.name = nameInput.value.trim();
      back.querySelectorAll<HTMLInputElement | HTMLSelectElement>("[data-p]").forEach((f) => { b.params[f.dataset.p!] = f.value; });
      close();
      await this.refreshPorts();
      // Drop lines that point at ports which no longer exist.
      const m = this.level;
      m.lines = m.lines.filter((l) => {
        const s = m.blocks.find((x) => x.id === l.src.block);
        const d = m.blocks.find((x) => x.id === l.dst.block);
        return !!s && !!d && l.src.port <= this.portsOf(s)[1] && l.dst.port <= this.portsOf(d)[0];
      });
      this.changed();
    };
    back.querySelector<HTMLButtonElement>("[data-c]")!.onclick = close;
    back.querySelector<HTMLButtonElement>("[data-ok]")!.onclick = ok;
    back.addEventListener("keydown", (e) => { if (e.key === "Enter") ok(); if (e.key === "Escape") close(); });
    back.addEventListener("mousedown", (e) => { if (e.target === back) close(); });
    (back.querySelector("input[data-p], select[data-p], input[data-name]") as HTMLElement | null)?.focus();
  }

  private solverDialog() {
    const s = this.root.solver;
    const back = document.createElement("div");
    back.className = "modal-back";
    const f = (k: keyof Solver, labelText: string) => `<label><span>${labelText}</span><input data-k="${k}" value="${esc(s[k])}"></label>`;
    back.innerHTML = `<div class="modal"><div class="modal-head">Solver settings</div>
      ${f("stop_time", "Stop time")}${f("max_step", "Max step size (variable step)")}${f("fixed_step", "Fixed step size")}${f("rel_tol", "Relative tolerance")}${f("abs_tol", "Absolute tolerance")}
      <p class="modal-hint">"auto" picks a value from the stop time and sample times.</p>
      <div class="modal-buttons"><button class="ea" data-c>Cancel</button><button class="ea primary" data-ok>OK</button></div></div>`;
    document.body.appendChild(back);
    back.querySelector<HTMLButtonElement>("[data-c]")!.onclick = () => back.remove();
    back.querySelector<HTMLButtonElement>("[data-ok]")!.onclick = () => {
      this.snapshot();
      back.querySelectorAll<HTMLInputElement>("[data-k]").forEach((i) => { (s as any)[i.dataset.k!] = i.value.trim() || "auto"; });
      (this.wrap.querySelector('[data-s="stop_time"]') as HTMLInputElement).value = s.stop_time;
      back.remove();
      this.host.onChange();
    };
  }
}

/** "TransferFcn" -> "Transfer Fcn". */
function label(type: string) {
  return type.replace(/([a-z])([A-Z0-9])/g, "$1 $2");
}

/** Outline shape of a block. */
function shapeMarkup(b: BlockSpec): string {
  const { w, h } = b;
  switch (b.type) {
    case "Gain": return `<path d="M0,0 L${w},${h / 2} L0,${h} z" class="bs"/>`;
    case "Sum": return `<ellipse cx="${w / 2}" cy="${h / 2}" rx="${w / 2}" ry="${h / 2}" class="bs"/>`;
    case "Mux": case "Demux": return `<rect width="${w}" height="${h}" class="bs bar"/>`;
    case "Inport": case "Outport": return `<rect width="${w}" height="${h}" rx="${h / 2}" class="bs port"/>`;
    case "Subsystem": return `<rect width="${w}" height="${h}" rx="4" class="bs sub"/>`;
    default: return `<rect width="${w}" height="${h}" rx="4" class="bs"/>`;
  }
}

/** Orthogonal route from an output port to an input port; feedback lines go around below. */
function route(x1: number, y1: number, x2: number, y2: number, s: BlockSpec, d: BlockSpec): string {
  if (x2 - x1 >= 20) {
    const mx = Math.round((x1 + x2) / 2);
    return `M${x1 + 6},${y1} H${mx} V${y2} H${x2 - 1}`;
  }
  const below = Math.max(s.y + s.h, d.y + d.h) + 30;
  return `M${x1 + 6},${y1} H${x1 + 20} V${below} H${x2 - 20} V${y2} H${x2 - 1}`;
}

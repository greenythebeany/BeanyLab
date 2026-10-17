// Render engine figures with Plotly.
import Plotly from "plotly.js-dist-min";
import type { Figure } from "./api";

const DASH: Record<string, string> = { "-": "solid", "--": "dash", ":": "dot", "-.": "dashdot" };
const MARKER: Record<string, string> = {
  o: "circle-open", "+": "cross-thin-open", "*": "asterisk-open", ".": "circle", x: "x-thin-open",
  s: "square-open", d: "diamond-open", "^": "triangle-up-open", v: "triangle-down-open",
  ">": "triangle-right-open", "<": "triangle-left-open", p: "star-open", h: "hexagram-open",
};
const CMAP: Record<string, string> = {
  parula: "Viridis", viridis: "Viridis", jet: "Jet", hot: "Hot", cool: "Bluered", gray: "Greys", grey: "Greys",
  turbo: "Turbo", hsv: "Rainbow", bone: "Greys", copper: "YlOrBr", spring: "Picnic", summer: "YlGn",
  autumn: "YlOrRd", winter: "Blues", default: "Viridis",
};

function lineTrace(s: any, axis: { x: string; y: string }, is3d: boolean, scene: string) {
  const st = s.style ?? {};
  const line = st.line ?? "-";
  const hasMarker = !!st.marker;
  const mode = line === "none" ? "markers" : hasMarker ? "lines+markers" : "lines";
  const t: any = {
    type: is3d ? "scatter3d" : "scatter",
    mode,
    x: s.x,
    y: s.y,
    name: s.name ?? undefined,
    showlegend: true,
    line: { color: st.color, width: st.width ?? 1.5, dash: DASH[line] ?? "solid" },
    marker: hasMarker ? { symbol: MARKER[st.marker] ?? "circle", size: st.marker_size ?? (st.marker === "." ? 4 : 7), color: st.color, line: { color: st.color, width: 1.3 } } : undefined,
  };
  if (is3d) {
    t.z = s.z ?? s.y.map(() => 0);
    t.scene = scene;
    if (t.marker) t.marker.symbol = "circle";
  } else {
    t.xaxis = axis.x;
    t.yaxis = axis.y;
  }
  return t;
}

function traces(ax: any, axis: { x: string; y: string }, scene: string, domain: { x: number[]; y: number[] }): any[] {
  const out: any[] = [];
  const cs = CMAP[ax.colormap ?? "default"] ?? "Viridis";
  for (const s of ax.series) {
    switch (s.type) {
      case "line":
        out.push(lineTrace(s, axis, ax.view3d && !!s.z, scene));
        break;
      case "scatter": {
        const is3d = !!s.z;
        out.push({
          type: is3d ? "scatter3d" : "scatter", mode: "markers", x: s.x, y: s.y, z: s.z ?? undefined,
          name: s.name ?? undefined,
          marker: {
            color: s.color, size: s.size ? s.size.map((v: number) => Math.max(3, Math.sqrt(v))) : 7,
            symbol: is3d ? "circle" : s.filled ? "circle" : "circle-open", line: { color: s.color, width: 1.2 },
          },
          ...(is3d ? { scene } : { xaxis: axis.x, yaxis: axis.y }),
        });
        break;
      }
      case "bar":
        out.push({
          type: "bar", orientation: s.horizontal ? "h" : "v",
          x: s.horizontal ? s.y : s.x, y: s.horizontal ? s.x : s.y,
          marker: { color: s.color }, width: s.width, name: s.name ?? undefined, xaxis: axis.x, yaxis: axis.y,
        });
        break;
      case "histogram":
        out.push({ type: "histogram", x: s.data, nbinsx: s.bins ?? undefined, marker: { color: s.color, line: { color: "#1E293B", width: 0.6 } }, xaxis: axis.x, yaxis: axis.y, name: s.name ?? undefined });
        break;
      case "stem": {
        const xs: (number | null)[] = [];
        const ys: (number | null)[] = [];
        s.x.forEach((x: number, i: number) => { xs.push(x, x, null); ys.push(0, s.y[i], null); });
        const color = s.style?.color;
        out.push({ type: "scatter", mode: "lines", x: xs, y: ys, line: { color, width: s.style?.width ?? 1.2 }, showlegend: false, hoverinfo: "skip", xaxis: axis.x, yaxis: axis.y });
        out.push({ type: "scatter", mode: "markers", x: s.x, y: s.y, marker: { color, symbol: "circle-open", size: 7 }, name: s.name ?? undefined, xaxis: axis.x, yaxis: axis.y });
        break;
      }
      case "stairs":
        out.push({ type: "scatter", mode: "lines", x: s.x, y: s.y, line: { shape: "hv", color: s.style?.color, width: s.style?.width ?? 1.5 }, name: s.name ?? undefined, xaxis: axis.x, yaxis: axis.y });
        break;
      case "area":
        out.push({ type: "scatter", mode: "lines", x: s.x, y: s.y, fill: "tozeroy", line: { color: s.color }, name: s.name ?? undefined, xaxis: axis.x, yaxis: axis.y });
        break;
      case "errorbar":
        out.push({ type: "scatter", mode: "lines+markers", x: s.x, y: s.y, error_y: { type: "data", array: s.err, visible: true, color: s.style?.color }, line: { color: s.style?.color }, name: s.name ?? undefined, xaxis: axis.x, yaxis: axis.y });
        break;
      case "surface":
        out.push({
          type: "surface", x: s.x, y: s.y, z: s.z, scene, colorscale: cs, showscale: !!ax.colorbar,
          ...(s.mesh
            ? { opacity: 0.95, hidesurface: false, surfacecolor: s.z, contours: { x: { show: true, color: "#1E293B", width: 1 }, y: { show: true, color: "#1E293B", width: 1 } } }
            : { contours: { x: { show: false }, y: { show: false } } }),
        });
        break;
      case "contour":
        out.push({ type: "contour", x: s.x, y: s.y, z: s.z, ncontours: s.levels ?? 10, colorscale: cs, showscale: !!ax.colorbar, contours: { coloring: s.filled ? "fill" : "lines" }, line: { width: 1.4 }, xaxis: axis.x, yaxis: axis.y });
        break;
      case "image":
        out.push({ type: "heatmap", z: s.z, colorscale: ax.colormap === "gray" ? "Greys" : cs, reversescale: ax.colormap === "gray", showscale: !!ax.colorbar, xaxis: axis.x, yaxis: axis.y });
        break;
      case "pie":
        out.push({ type: "pie", values: s.values, labels: s.labels, domain: { x: domain.x, y: domain.y }, sort: false, textinfo: "percent" });
        break;
    }
  }
  return out;
}

export function render(el: HTMLElement, fig: Figure, dark: boolean) {
  const data: any[] = [];
  const layout: any = {
    margin: { l: 56, r: 24, t: 40, b: 48 },
    paper_bgcolor: dark ? "#1B1B30" : "#FFFFFF",
    plot_bgcolor: dark ? "#1B1B30" : "#FFFFFF",
    font: { family: "Segoe UI, system-ui, sans-serif", size: 12, color: dark ? "#E2E8F0" : "#1E293B" },
    showlegend: false,
    annotations: [] as any[],
  };
  const axesList = fig.axes ?? [];
  axesList.forEach((ax: any, i: number) => {
    const [rows, cols] = ax.grid ?? [1, 1];
    const cells: number[] = ax.cells ?? [1];
    const rIdx = cells.map((c) => Math.floor((c - 1) / cols));
    const cIdx = cells.map((c) => (c - 1) % cols);
    // Leave room between rows for x-axis labels and titles of the tile below.
    const gap = 0.08;
    const vgap = rows > 1 ? 0.14 : 0;
    // Grid cell in unit coordinates, then mapped into the figure's axes area (tiled layouts
    // may reserve part of the window for tables).
    const [ax0, ay0, aw, ah] = (fig as any).area ?? [0, 0, 1, 1];
    const x0 = ax0 + aw * (Math.min(...cIdx) / cols + (cols > 1 ? gap / 2 : 0));
    const x1 = ax0 + aw * ((Math.max(...cIdx) + 1) / cols - (cols > 1 ? gap / 2 : 0));
    const y1 = ay0 + ah * (1 - Math.min(...rIdx) / rows - (rows > 1 ? vgap * 0.4 : 0));
    const y0 = ay0 + ah * (1 - (Math.max(...rIdx) + 1) / rows + (rows > 1 ? vgap * 0.6 : 0));
    const n = i === 0 ? "" : String(i + 1);
    const axis = { x: `x${n}`, y: `y${n}` };
    const scene = `scene${n}`;
    const domain = { x: [x0, x1], y: [y0, y1] };
    const tr = traces(ax, axis, scene, domain);
    data.push(...tr);
    const grid = dark ? "#33335A" : "#E2E8F0";
    const line = dark ? "#94A3B8" : "#334155";
    const axisStyle = (title: string | null, range: [number, number] | null, log: boolean) => ({
      title: title ? { text: escapeHtml(title) } : undefined,
      range: range ? (log ? range.map((v) => Math.log10(v)) : range) : undefined,
      autorange: range ? false : true,
      type: log ? "log" : "linear",
      showgrid: !!ax.grid_on,
      gridcolor: grid,
      zeroline: false,
      showline: true,
      linecolor: line,
      mirror: ax.box_on ? "ticks" : false,
      ticks: "inside",
      tickcolor: line,
      visible: !ax.axis_off,
    });
    const legendId = i === 0 ? "legend" : `legend${i + 1}`;
    if (ax.legend_visible || (ax.legend && ax.legend.length)) {
      layout.showlegend = true;
      let k = 0;
      for (const t of tr) {
        t.legend = legendId;
        if (t.showlegend === false) continue;
        if (ax.legend) {
          if (k < ax.legend.length) t.name = ax.legend[k];
          else t.showlegend = false;
        }
        k++;
      }
      // Each axes gets its own legend placed inside its plot area.
      const loc: string = ax.legend_location ?? "northeast";
      const east = loc.includes("east") || loc === "best";
      const west = loc.includes("west");
      const south = loc.startsWith("south");
      const north = loc.startsWith("north") || (!south && !loc.includes("east") && !west) || loc === "best";
      layout[legendId] = {
        bgcolor: dark ? "rgba(27,27,48,.85)" : "rgba(255,255,255,.85)", bordercolor: line, borderwidth: 1, font: { size: 11 },
        x: west ? x0 + 0.01 : east ? x1 - 0.01 : (x0 + x1) / 2, xanchor: west ? "left" : east ? "right" : "center",
        y: south ? y0 + 0.01 : north ? y1 - 0.01 : (y0 + y1) / 2, yanchor: south ? "bottom" : north ? "top" : "middle",
      };
    } else {
      for (const t of tr) t.showlegend = false;
    }
    const hasPie = ax.series.some((s: any) => s.type === "pie");
    if (ax.view3d && ax.series.some((s: any) => s.type === "surface" || ((s.type === "line" || s.type === "scatter") && s.z))) {
      layout[scene] = {
        domain,
        xaxis: { title: { text: escapeHtml(ax.xlabel ?? "") }, gridcolor: grid, showbackground: false, range: ax.xlim ?? undefined },
        yaxis: { title: { text: escapeHtml(ax.ylabel ?? "") }, gridcolor: grid, showbackground: false, range: ax.ylim ?? undefined },
        zaxis: { title: { text: escapeHtml(ax.zlabel ?? "") }, gridcolor: grid, showbackground: false, range: ax.zlim ?? undefined },
        aspectmode: ax.axis_equal ? "data" : "cube",
        camera: { eye: { x: -1.6, y: -1.6, z: 1.1 } },
      };
    } else if (!hasPie) {
      layout[`xaxis${n}`] = { ...axisStyle(ax.xlabel, ax.xlim, ax.xscale_log), domain: [x0, x1], anchor: axis.y };
      layout[`yaxis${n}`] = { ...axisStyle(ax.ylabel, ax.ylim, ax.yscale_log), domain: [y0, y1], anchor: axis.x };
      if (ax.axis_equal) layout[`yaxis${n}`].scaleanchor = axis.x;
      // Axes holding only text (e.g. `axis off` + `text`) use a fixed unit square.
      if (ax.series.length && ax.series.every((s: any) => s.type === "text")) {
        layout[`xaxis${n}`].range = ax.xlim ?? [0, 1];
        layout[`yaxis${n}`].range = ax.ylim ?? [0, 1];
        layout[`xaxis${n}`].autorange = false;
        layout[`yaxis${n}`].autorange = false;
      }
      if (ax.series.some((s: any) => s.type === "image")) layout[`yaxis${n}`].autorange = "reversed";
    }
    if (ax.title) {
      layout.annotations.push({ text: `<b>${escapeHtml(ax.title)}</b>`, x: (x0 + x1) / 2, y: y1, xref: "paper", yref: "paper", xanchor: "center", yanchor: "bottom", showarrow: false, font: { size: 13 }, yshift: 6 });
    }
    for (const s of ax.series) {
      if (s.type === "text") {
        let html = s.latex ? latexToHtml(s.text) : escapeHtml(s.text);
        if (s.bold) html = `<b>${html}</b>`;
        layout.annotations.push({ text: html.replace(/\n/g, "<br>"), x: s.x, y: s.y, xref: axis.x, yref: axis.y, showarrow: false, xanchor: "left", font: { size: s.size ?? 12 } });
      }
    }
  });
  // uitable: drawn as table traces in their normalized figure positions.
  for (const t of ((fig as any).uitables ?? []) as any[]) {
    const [px, py, pw, ph] = t.position;
    const hasRows = t.rows && t.rows.length > 0;
    const headers = [...(hasRows ? [""] : []), ...t.columns];
    const ncols = t.cells[0]?.length ?? 0;
    const columns: string[][] = [];
    if (hasRows) columns.push(t.rows);
    for (let c = 0; c < ncols; c++) columns.push(t.cells.map((row: string[]) => row[c]));
    data.push({
      type: "table",
      domain: { x: [Math.max(0, px), Math.min(1, px + pw)], y: [Math.max(0, py), Math.min(1, py + ph)] },
      header: { values: headers.map((h) => `<b>${escapeHtml(h)}</b>`), fill: { color: dark ? "#23234A" : "#EEF0FA" }, line: { color: dark ? "#3A3A60" : "#CBD2E0" }, align: "center", font: { size: 11 } },
      cells: { values: columns, fill: { color: dark ? "#16162A" : "#FFFFFF" }, line: { color: dark ? "#2A2A48" : "#E2E5EE" }, align: hasRows ? ["left", "right"] : "right", font: { family: "Cascadia Mono, Consolas, monospace", size: 11 }, height: 22 },
    });
  }
  if (axesList.length === 0) {
    layout.xaxis = { visible: false };
    layout.yaxis = { visible: false };
  }
  // Scope figures: a vertical time cursor that reads out every signal.
  if ((fig as any).scope) {
    layout.hovermode = "x unified";
    for (const k of Object.keys(layout)) {
      if (k.startsWith("xaxis")) Object.assign(layout[k], { showspikes: true, spikemode: "across", spikesnap: "cursor", spikethickness: 1, spikedash: "dot", spikecolor: dark ? "#94A3B8" : "#64748B" });
    }
  }
  // Plotly treats a present-but-undefined key (e.g. `marker: undefined`) as an object; drop them.
  Plotly.react(el, clean(data), clean(layout), {
    responsive: true,
    displaylogo: false,
    toImageButtonOptions: { format: "png", filename: `figure${fig.id}`, scale: 2 },
    modeBarButtonsToRemove: ["select2d", "lasso2d"],
  });
}

export function purge(el: HTMLElement) {
  Plotly.purge(el);
}

export function resize(el: HTMLElement) {
  Plotly.Plots.resize(el);
}

function clean<T>(v: T): T {
  if (Array.isArray(v)) return v.map((x) => (x && typeof x === "object" ? clean(x) : x)) as T;
  if (v && typeof v === "object") {
    const o: any = {};
    for (const [k, x] of Object.entries(v)) {
      if (x === undefined) continue;
      // Numeric data arrays are left untouched for speed.
      o[k] = x && typeof x === "object" && !(Array.isArray(x) && typeof x[0] === "number") ? clean(x) : x;
    }
    return o;
  }
  return v;
}

const GREEK: Record<string, string> = {
  alpha: "α", beta: "β", gamma: "γ", delta: "δ", epsilon: "ε", zeta: "ζ", eta: "η", theta: "θ", iota: "ι", kappa: "κ",
  lambda: "λ", mu: "μ", nu: "ν", xi: "ξ", pi: "π", rho: "ρ", sigma: "σ", tau: "τ", phi: "φ", chi: "χ", psi: "ψ", omega: "ω",
  Gamma: "Γ", Delta: "Δ", Theta: "Θ", Lambda: "Λ", Xi: "Ξ", Pi: "Π", Sigma: "Σ", Phi: "Φ", Psi: "Ψ", Omega: "Ω",
  infty: "∞", pm: "±", circ: "°", cdot: "·", times: "×", leq: "≤", geq: "≥", neq: "≠", approx: "≈", partial: "∂", nabla: "∇",
  rightarrow: "→", leftarrow: "←", sqrt: "√", int: "∫", sum: "∑",
};

/** Readable rendering of simple LaTeX strings ($...$ with \frac, \displaystyle, spacing). */
function latexToHtml(src: string): string {
  let s = src.trim().replace(/^\$+|\$+$/g, "");
  s = s.replace(/\\(displaystyle|textstyle|left|right)\b/g, "").replace(/\\[,;:!]|~/g, " ").replace(/\\quad|\\qquad/g, "  ");
  s = s.replace(/\\cdot/g, "·").replace(/\\times/g, "×");
  // Turn ^{..}/_{..} into brace-free markers first so \frac's own braces can be matched.
  for (let i = 0; i < 4; i++) {
    s = s.replace(/\^\{([^{}]*)\}/g, "\u0001$1\u0002").replace(/_\{([^{}]*)\}/g, "\u0003$1\u0004");
  }
  // \frac{a}{b} -> (a) / (b), innermost first.
  for (let i = 0; i < 6 && /\\frac/.test(s); i++) {
    s = s.replace(/\\frac\{([^{}]*)\}\{([^{}]*)\}/g, (_, a, b) => `(${a.trim()}) / (${b.trim()})`);
  }
  s = escapeHtml(s.replace(/\s+/g, " "));
  return s.replace(/\u0001/g, "<sup>").replace(/\u0002/g, "</sup>").replace(/\u0003/g, "<sub>").replace(/\u0004/g, "</sub>");
}

/** Minimal TeX for labels: Greek/symbol commands, ^{..}/_{..} and single-char ^x/_x. */
function escapeHtml(s: string) {
  let t = s.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;");
  t = t.replace(/\\([A-Za-z]+)/g, (m, name) => GREEK[name] ?? m);
  t = t.replace(/\^\{([^}]*)\}/g, "<sup>$1</sup>").replace(/_\{([^}]*)\}/g, "<sub>$1</sub>");
  t = t.replace(/\^([^\s<{])/g, "<sup>$1</sup>").replace(/_([^\s<{])/g, "<sub>$1</sub>");
  return t;
}

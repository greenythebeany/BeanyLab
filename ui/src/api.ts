// Engine bridge: Tauri IPC inside the desktop app, HTTP to `beany serve` in a browser.

export interface VarInfo { name: string; size: string; class: string; value: string; bytes: number }
export interface Figure { id: number; name: string | null; axes: any[]; current_axes: number; visible: boolean }
export interface EvalResult {
  output: string;
  error: string | null;
  workspace: VarInfo[];
  figures: Figure[];
  closed: number[];
  cwd: string;
}
export interface FileEntry { name: string; path: string; is_dir: boolean; size: number; modified: number | null }
export interface VarDetail { name: string; class: string; rows: number; cols: number; cells: string[][]; truncated: boolean }

const tauri = (window as any).__TAURI_INTERNALS__ !== undefined;
export const isDesktop = tauri;

let invoke: ((cmd: string, args?: any) => Promise<any>) | null = null;
let listen: ((ev: string, cb: (e: { payload: any }) => void) => Promise<() => void>) | null = null;

async function ensureTauri() {
  if (!tauri || invoke) return;
  const core = await import("@tauri-apps/api/core");
  const event = await import("@tauri-apps/api/event");
  invoke = core.invoke;
  listen = event.listen as any;
}

export async function call<T = any>(method: string, args: Record<string, unknown> = {}): Promise<T> {
  if (tauri) {
    await ensureTauri();
    return invoke!("api", { method, args }) as Promise<T>;
  }
  const res = await fetch(`/api/${method}`, { method: "POST", body: JSON.stringify(args) });
  const data = await res.json();
  if (!res.ok) throw new Error(data?.error ?? res.statusText);
  return data as T;
}

export async function interrupt(): Promise<void> {
  if (tauri) {
    await ensureTauri();
    await invoke!("interrupt");
    return;
  }
  await fetch("/api/interrupt", { method: "POST", body: "{}" });
}

/** Hold (true) or resume (false) a running simulation. */
export async function pauseSim(on: boolean): Promise<void> {
  if (tauri) {
    await ensureTauri();
    await invoke!("pause", { on });
    return;
  }
  await fetch("/api/pause", { method: "POST", body: JSON.stringify({ on }) });
}

/** Named engine events such as simulation progress (desktop only). */
export async function onEvent(handler: (name: string, payload: any) => void) {
  if (!tauri) return;
  await ensureTauri();
  await listen!("beany-event", (e) => {
    const p = e.payload as { name: string; payload: any };
    handler(p.name, p.payload);
  });
}

/** Open (or focus) another app window showing this UI with a query string. */
export async function openAppWindow(label: string, query: string, title: string): Promise<void> {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const existing = await WebviewWindow.getByLabel(label);
  if (existing) {
    await existing.unminimize();
    await existing.setFocus();
    return;
  }
  // The app's own drag-and-drop (palette to canvas) needs the OS file-drop handler off.
  new WebviewWindow(label, { url: `index.html?${query}`, title, width: 1280, height: 820, minWidth: 700, minHeight: 450, dragDropEnabled: false });
}

/** Open a figure in its own OS window (desktop). */
export async function openFigureWindow(id: number, title: string, width: number, height: number, offset: number): Promise<void> {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  if (await WebviewWindow.getByLabel(`figure-${id}`)) return;
  new WebviewWindow(`figure-${id}`, {
    url: `index.html?figure=${id}`, title, width, height, minWidth: 320, minHeight: 240,
    x: 120 + offset * 30, y: 90 + offset * 30, dragDropEnabled: false,
  });
}

export async function closeAppWindow(label: string): Promise<void> {
  const { WebviewWindow } = await import("@tauri-apps/api/webviewWindow");
  const w = await WebviewWindow.getByLabel(label);
  if (w) await w.destroy();
}

/** Arrange every window whose label starts with `prefix` in a grid on the current screen. */
export async function tileWindows(prefix: string): Promise<number> {
  const { getAllWebviewWindows } = await import("@tauri-apps/api/webviewWindow");
  const { currentMonitor, PhysicalPosition, PhysicalSize } = await import("@tauri-apps/api/window");
  const wins = (await getAllWebviewWindows()).filter((w) => w.label.startsWith(prefix)).sort((a, b) => a.label.localeCompare(b.label, undefined, { numeric: true }));
  const mon = await currentMonitor();
  if (!wins.length || !mon) return wins.length;
  const { width, height } = mon.workArea?.size ?? mon.size;
  const { x: ox, y: oy } = mon.workArea?.position ?? mon.position;
  const cols = Math.ceil(Math.sqrt(wins.length));
  const rows = Math.ceil(wins.length / cols);
  const w = Math.floor(width / cols);
  const h = Math.floor(height / rows);
  for (let i = 0; i < wins.length; i++) {
    const win = wins[i];
    await win.unmaximize();
    await win.setSize(new PhysicalSize(w, h));
    await win.setPosition(new PhysicalPosition(ox + (i % cols) * w, oy + Math.floor(i / cols) * h));
  }
  return wins.length;
}

/** Run `beforeClose` when the user closes this window, then close it. */
export async function onWindowClose(beforeClose: () => Promise<void>): Promise<void> {
  if (!tauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  const w = getCurrentWindow();
  await w.onCloseRequested(async (e) => {
    e.preventDefault();
    try { await beforeClose(); } finally { await w.destroy(); }
  });
}

/** Broadcast an event to every app window (desktop only). */
export async function broadcast(name: string, payload: unknown): Promise<void> {
  if (!tauri) return;
  const event = await import("@tauri-apps/api/event");
  await event.emit(name, payload);
}

export async function onAppEvent(name: string, handler: (payload: any) => void): Promise<void> {
  if (!tauri) return;
  await ensureTauri();
  await listen!(name, (e) => handler(e.payload));
}

export async function setWindowTitle(title: string): Promise<void> {
  document.title = title;
  if (!tauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().setTitle(title);
}

/** Ask before closing this window; `allow` returns false to keep it open. */
export async function guardClose(allow: () => boolean): Promise<void> {
  if (!tauri) return;
  const { getCurrentWindow } = await import("@tauri-apps/api/window");
  await getCurrentWindow().onCloseRequested((e) => { if (!allow()) e.preventDefault(); });
}

/** Live output/figure streams (desktop only; the browser gets everything in the result). */
export async function onStream(onOutput: (s: string) => void, onFigures: (f: { figures: Figure[]; closed: number[] }) => void) {
  if (!tauri) return;
  await ensureTauri();
  await listen!("beany-output", (e) => onOutput(e.payload as string));
  await listen!("beany-figures", (e) => onFigures(e.payload));
}

export async function pickFolder(current: string): Promise<string | null> {
  if (tauri) {
    const d = await import("@tauri-apps/plugin-dialog");
    const r = await d.open({ directory: true, defaultPath: current });
    return typeof r === "string" ? r : null;
  }
  return window.prompt("Folder path", current);
}

export async function pickOpenFile(current: string): Promise<string | null> {
  if (tauri) {
    const d = await import("@tauri-apps/plugin-dialog");
    const r = await d.open({ defaultPath: current, filters: [{ name: "Code and model files", extensions: ["m", "slx", "mdl", "bmdl"] }, { name: "All files", extensions: ["*"] }] });
    return typeof r === "string" ? r : null;
  }
  return window.prompt("File path", current);
}

export async function pickSaveFile(suggested: string): Promise<string | null> {
  if (tauri) {
    const d = await import("@tauri-apps/plugin-dialog");
    const r = await d.save({ defaultPath: suggested, filters: /\.(slx|bmdl|mdl)$/i.test(suggested) ? [{ name: "Model (.slx)", extensions: ["slx"] }, { name: "BeanyLab model (.bmdl)", extensions: ["bmdl"] }] : [{ name: "Code files", extensions: ["m"] }] });
    return r ?? null;
  }
  return window.prompt("Save as", suggested);
}

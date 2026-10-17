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
    const r = await d.open({ defaultPath: current, filters: [{ name: "Code files", extensions: ["m"] }, { name: "All files", extensions: ["*"] }] });
    return typeof r === "string" ? r : null;
  }
  return window.prompt("File path", current);
}

export async function pickSaveFile(suggested: string): Promise<string | null> {
  if (tauri) {
    const d = await import("@tauri-apps/plugin-dialog");
    const r = await d.save({ defaultPath: suggested, filters: [{ name: "Code files", extensions: ["m"] }] });
    return r ?? null;
  }
  return window.prompt("Save as", suggested);
}

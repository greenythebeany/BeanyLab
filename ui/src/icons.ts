// BeanyLab line icon set: 24×24 grid, 1.75 stroke, rounded caps.
// Drawn after the generated icon sheet; `accent` paths use the brand teal.

const P: Record<string, string> = {
  newScript: `<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path class="a" d="M12 11v6M9 14h6"/>`,
  newFunction: `<path d="M14 3H7a2 2 0 0 0-2 2v14a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2V8z"/><path d="M14 3v5h5"/><path class="a" d="M10 11c-1 0-1.5.6-1.5 1.5S8 14 7.5 14c.5 0 1 .5 1 1.5S9 17 10 17M14 11c1 0 1.5.6 1.5 1.5s.5 1.5 1 1.5c-.5 0-1 .5-1 1.5S15 17 14 17"/>`,
  open: `<path d="M3 7a2 2 0 0 1 2-2h4l2 2h7a2 2 0 0 1 2 2v1"/><path d="M3 7v11a1 1 0 0 0 1 1h13.5a1 1 0 0 0 1-.7L21 11H7.5a1 1 0 0 0-1 .7L4 19"/>`,
  save: `<path d="M5 3h11l4 4v12a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V5a2 2 0 0 1 1-2z"/><path d="M8 3v5h7V3"/><path d="M7 21v-7h10v7"/>`,
  saveAs: `<path d="M5 3h11l4 4v5"/><path d="M4 5v14a2 2 0 0 0 2 2h6"/><path d="M8 3v5h7V3"/><path class="a" d="M18 15v6M15 18h6"/>`,
  run: `<path class="a" d="M7 4.5v15a1 1 0 0 0 1.5.9l12-7.5a1 1 0 0 0 0-1.8l-12-7.5A1 1 0 0 0 7 4.5z"/>`,
  runSection: `<path d="M4 4v16"/><path class="a" d="M9 5.5v13a1 1 0 0 0 1.5.9l9.5-6.5a1 1 0 0 0 0-1.8L10.5 4.6A1 1 0 0 0 9 5.5z"/>`,
  runAdvance: `<path class="a" d="M5 5.5v13a1 1 0 0 0 1.5.9l9-6.5a1 1 0 0 0 0-1.8l-9-6.5A1 1 0 0 0 5 5.5z"/><path d="M20 5v14"/>`,
  stop: `<rect x="4" y="4" width="16" height="16" rx="3"/><rect class="a" x="9" y="9" width="6" height="6" rx="1"/>`,
  debug: `<path d="M9 7.5V6a3 3 0 0 1 6 0v1.5"/><path d="M8 9h8a2 2 0 0 1 2 2v3a6 6 0 0 1-12 0v-3a2 2 0 0 1 2-2z"/><path d="M12 9v11M3 13h3M18 13h3M4 8l2.5 2M20 8l-2.5 2M4 19l2.5-2M20 19l-2.5-2"/>`,
  stepOver: `<path d="M4 12a8 8 0 0 1 14.9-4"/><path d="M20 4v4h-4"/><circle class="a" cx="12" cy="18" r="1.6"/>`,
  stepIn: `<path d="M12 3v11"/><path d="M8 10l4 4 4-4"/><path d="M5 20h14"/>`,
  stepOut: `<path d="M12 15V4"/><path d="M8 8l4-4 4 4"/><path d="M5 20h14"/>`,
  breakpoint: `<circle class="f" cx="12" cy="12" r="6"/>`,
  terminal: `<path d="M5 7l5 5-5 5"/><path class="a" d="M12 18h7"/>`,
  variables: `<rect x="4" y="5" width="3" height="3" rx=".6"/><rect x="4" y="10.5" width="3" height="3" rx=".6"/><rect x="4" y="16" width="3" height="3" rx=".6"/><path d="M10 6.5h10M10 12h10M10 17.5h10"/>`,
  table: `<rect x="3.5" y="4.5" width="17" height="15" rx="2"/><path d="M3.5 9.5h17M3.5 14.5h17M9.2 4.5v15M14.8 4.5v15"/>`,
  folder: `<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/>`,
  folderTree: `<rect x="3" y="3" width="7" height="5" rx="1"/><rect x="13" y="10" width="8" height="5" rx="1"/><rect x="13" y="17" width="8" height="5" rx="1"/><path d="M6.5 8v11.5H13M6.5 12.5H13"/>`,
  folderUp: `<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/><path class="a" d="M12 16v-6M9.5 12.5L12 10l2.5 2.5"/>`,
  back: `<path d="M15 6l-6 6 6 6"/>`,
  forward: `<path d="M9 6l6 6-6 6"/>`,
  refresh: `<path d="M20 11a8 8 0 1 0-2.3 5.7"/><path d="M20 4v7h-7"/>`,
  chartLine: `<path d="M4 4v16h16"/><path class="a" d="M7 15l3.5-5 3 3.5L19 6"/>`,
  chartScatter: `<path d="M4 4v16h16"/><circle class="a" cx="8.5" cy="15" r="1.4"/><circle class="a" cx="12" cy="10.5" r="1.4"/><circle class="a" cx="15" cy="13.5" r="1.4"/><circle class="a" cx="17.5" cy="7.5" r="1.4"/>`,
  chartBar: `<path d="M4 20h16"/><rect class="a" x="6" y="11" width="3" height="9" rx=".6"/><rect class="a" x="10.5" y="6" width="3" height="14" rx=".6"/><rect class="a" x="15" y="13" width="3" height="7" rx=".6"/>`,
  chartHist: `<path d="M4 20h16"/><path class="a" d="M5 20v-5h3v-6h3v-4h3v7h3v4h2v4"/>`,
  chartArea: `<path d="M4 4v16h16"/><path class="a" d="M4 17l4-5 4 3 4-6 4 3v8H4z"/>`,
  chartStem: `<path d="M4 20h16"/><path class="a" d="M7 20v-8M11 20V7M15 20v-5M19 20v-9"/><circle class="a" cx="7" cy="11" r="1.3"/><circle class="a" cx="11" cy="6" r="1.3"/><circle class="a" cx="15" cy="14" r="1.3"/><circle class="a" cx="19" cy="10" r="1.3"/>`,
  chartStairs: `<path d="M4 4v16h16"/><path class="a" d="M6 17h3v-4h4v-3h3V6h3"/>`,
  chartSurface: `<path d="M12 3l9 5-9 5-9-5z"/><path d="M3 8v8l9 5 9-5V8"/><path class="a" d="M7.5 10.5v8M12 13v8M16.5 10.5v8"/>`,
  chartContour: `<ellipse cx="12" cy="12" rx="9" ry="7"/><ellipse class="a" cx="12" cy="12" rx="5.5" ry="4"/><ellipse cx="12" cy="12" rx="2" ry="1.4"/>`,
  chartPie: `<path d="M12 3a9 9 0 1 0 9 9h-9z"/><path class="a" d="M15 3.5A9 9 0 0 1 20.5 9H15z"/>`,
  chartImage: `<rect x="3.5" y="3.5" width="17" height="17" rx="2"/><path class="a" d="M3.5 9.2h17M3.5 14.8h17M9.2 3.5v17M14.8 3.5v17"/>`,
  window: `<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M12 4v16"/>`,
  settings: `<circle cx="12" cy="12" r="3"/><path d="M12 2.5v2.2M12 19.3v2.2M4.6 4.6l1.6 1.6M17.8 17.8l1.6 1.6M2.5 12h2.2M19.3 12h2.2M4.6 19.4l1.6-1.6M17.8 6.2l1.6-1.6"/>`,
  help: `<circle cx="12" cy="12" r="9"/><path d="M9.5 9.5a2.5 2.5 0 1 1 3.5 2.3c-.6.3-1 .9-1 1.6v.6"/><circle class="f" cx="12" cy="17.2" r=".9"/>`,
  addons: `<path d="M9 4h3v2a2 2 0 1 0 4 0V4h3a1 1 0 0 1 1 1v3h-2a2 2 0 1 0 0 4h2v3"/><path d="M4 9v10a1 1 0 0 0 1 1h10v-3a2 2 0 1 1 4 0"/><path d="M4 9h3"/>`,
  clear: `<path d="M4 7h16M10 11v6M14 11v6"/><path d="M6 7l1 12a2 2 0 0 0 2 2h6a2 2 0 0 0 2-2l1-12"/><path d="M9 7V4h6v3"/>`,
  layout: `<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 4v16M9 12h12"/>`,
  find: `<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.3-4.3"/>`,
  comment: `<path d="M5 6h4M5 12h4M5 18h4"/><path class="a" d="M14 6l-2 12M18 6l-2 12"/>`,
  uncomment: `<path d="M5 6h14M5 12h14M5 18h9"/>`,
  indent: `<path d="M10 6h10M10 12h10M10 18h10"/><path class="a" d="M3.5 9L7 12l-3.5 3"/>`,
  close: `<path d="M6 6l12 12M18 6L6 18"/>`,
  minus: `<path d="M5 12h14"/>`,
  maximize: `<rect x="5" y="5" width="14" height="14" rx="1.5"/>`,
  dock: `<rect x="3" y="4" width="18" height="16" rx="2"/><path d="M3 14h18"/>`,
  history: `<path d="M3.5 12a8.5 8.5 0 1 0 2.5-6"/><path d="M3 4v4h4"/><path class="a" d="M12 8v4l3 2"/>`,
  moon: `<path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5z"/>`,
  sun: `<circle cx="12" cy="12" r="4"/><path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4"/>`,
  importData: `<path d="M12 3v12"/><path d="M8 11l4 4 4-4"/><path d="M4 17v2a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2v-2"/>`,
  path: `<path d="M4 6h16M4 12h10M4 18h7"/><circle class="a" cx="18" cy="16" r="3"/>`,
  copy: `<rect x="8" y="8" width="12" height="12" rx="2"/><path d="M16 8V6a2 2 0 0 0-2-2H6a2 2 0 0 0-2 2v8a2 2 0 0 0 2 2h2"/>`,
  download: `<path d="M12 4v11"/><path d="M8 11l4 4 4-4"/><path d="M5 20h14"/>`,
  zoom: `<circle cx="11" cy="11" r="6.5"/><path d="M20 20l-4.3-4.3M11 8v6M8 11h6"/>`,
};

// File-type icons drawn after the generated file icon sheet.
const FILES: Record<string, string> = {
  script: `<path d="M14 2.5H6.5A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7.5z"/><path d="M14 2.5V7a.5.5 0 0 0 .5.5H19"/><path class="a" d="M10.5 11v6l4.5-3z"/>`,
  function: `<path d="M14 2.5H6.5A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7.5z"/><path d="M14 2.5V7a.5.5 0 0 0 .5.5H19"/><path class="a" d="M10.5 11c-.9 0-1.3.5-1.3 1.3s-.4 1.2-.9 1.2c.5 0 .9.4.9 1.2s.4 1.3 1.3 1.3M13.5 11c.9 0 1.3.5 1.3 1.3s.4 1.2.9 1.2c-.5 0-.9.4-.9 1.2s-.4 1.3-1.3 1.3"/>`,
  data: `<path d="M14 2.5H6.5A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7.5z"/><path d="M14 2.5V7a.5.5 0 0 0 .5.5H19"/><path class="a" d="M9 11h.01M12 11h.01M15 11h.01M9 14h.01M12 14h.01M15 14h.01M9 17h.01M12 17h.01M15 17h.01" stroke-width="2.4"/>`,
  model: `<path d="M14 2.5H6.5A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7.5z"/><path d="M14 2.5V7a.5.5 0 0 0 .5.5H19"/><rect class="a" x="7.5" y="12" width="3.5" height="3.5" rx=".5"/><rect class="a" x="13" y="12" width="3.5" height="3.5" rx=".5"/><path class="a" d="M11 13.75h2"/>`,
  file: `<path d="M14 2.5H6.5A1.5 1.5 0 0 0 5 4v16a1.5 1.5 0 0 0 1.5 1.5h11A1.5 1.5 0 0 0 19 20V7.5z"/><path d="M14 2.5V7a.5.5 0 0 0 .5.5H19"/>`,
  folder: `<path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" class="fd"/>`,
};

export function icon(name: string, size = 20): string {
  const body = P[name] ?? FILES[name] ?? P.help;
  return `<svg class="ico" width="${size}" height="${size}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;
}

export function fileIcon(name: string, isDir: boolean): string {
  if (isDir) return icon("folder", 16);
  const lower = name.toLowerCase();
  if (lower.endsWith(".m")) return icon("script", 16);
  if (lower.endsWith(".mat") || lower.endsWith(".csv") || lower.endsWith(".txt") || lower.endsWith(".dat")) return icon("data", 16);
  if (lower.endsWith(".bmdl") || lower.endsWith(".slx") || lower.endsWith(".mdl")) return icon("model", 16);
  return icon("file", 16);
}

export const APP_ICON = `<svg viewBox="0 0 1024 1024" width="22" height="22" aria-hidden="true"><rect x="32" y="32" width="960" height="960" rx="220" fill="#241E5C"/><g fill="#2DD4BF"><circle cx="290" cy="290" r="58"/><circle cx="512" cy="290" r="58"/><circle cx="290" cy="512" r="58"/><circle cx="734" cy="512" r="58"/><circle cx="512" cy="734" r="58"/><circle cx="734" cy="734" r="58"/></g><path d="M250 790 C340 790 360 420 430 420 C500 420 520 650 590 650 C680 650 690 270 790 250" fill="none" stroke="#2DD4BF" stroke-width="58" stroke-linecap="round"/></svg>`;

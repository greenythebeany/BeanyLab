# BeanyLab Roadmap

The plan for building BeanyLab into a working, open-source numeric computing environment that runs existing `.m` code on Windows and Linux. We follow it milestone by milestone. Changes to this plan happen by editing this file, never silently.

## Goal

**Version 1.0:** a student or engineer can install BeanyLab, open their existing `.m` files, and run them unmodified. That covers scripts, functions, classes, plots, `.mat` data and the common toolbox functions used in university engineering courses.

**Measured by:** BeanyLab runs at least **90% of the compatibility corpus** (see M0) with correct output.

After 1.0, the block-diagram simulator (M8) and wider toolbox coverage follow.

## Ground rules

1. **Clean room.**
   - Never read, copy or decompile MathWorks code, documentation text, examples, test suites or artwork.
   - Behavior comes from public, observable language semantics and from our own tests.
   - No "MATLAB" or "Simulink" in product names or branding.
   - The rules are written out in `CLEAN_ROOM.md`, and every contributor agrees to them.
2. **Permissive dependencies only.** Use MIT, BSD or Apache-2.0 crates and packages. GPL code (Octave, FFTW, GSL) is never copied in or linked.
3. **One milestone at a time.**
   - A milestone is done only when every exit criterion passes.
   - Work outside the current milestone goes into the backlog, not the code.
   - The one exception is bugs that block a real user's script; those are fixed right away.
4. **No feature without tests.**
   - Each language or library feature gets conformance tests, written in `.m` with expected output.
   - Rust unit tests cover internals.
5. **CI must be green** on Windows and Linux before anything merges.
6. **Real code drives priorities.** When a corpus script fails, the missing feature goes to the top of the current milestone's list, or into the backlog if it belongs to a later milestone.
7. **Weekly check-in.** Each week, update the milestone checklist and the compatibility score in this file.

## Where we are (v0.1, October 2026)

**Done:**
- **Engine:** lexer, parser and tree-walking interpreter.
- **Language:** matrices, indexing, control flow, functions, cells, structs, strings, integer types, anonymous functions, `try/catch`.
- **Library:** about 430 built-in functions.
- **Files and models:** `.mat` load/save, `tf`/`step`, `table`/`writetable`.
- **Plots:** about 25 plot types, `tiledlayout`, `uitable`.
- **Desktop app (Tauri):** editor, command window, workspace, current folder, figures.
- **CLI:** REPL and script runner.
- **Tests:** 44 engine tests.
- **Real course files:** `cv2.m`, `cv3.m` and `cv4.m` run correctly.

**Missing:**
- Complex numbers, N-D arrays, `classdef`, debugger, sparse matrices, FFT, ODE solvers.
- Graphics handle objects (`get`/`set`).
- Installers, CI and a git repository.

---

## M0 — Open-source foundation (1–2 weeks)

Set the project up so others can see, build, test and contribute.

- [x] Git repository and first commit.
- [ ] Public GitHub repository (waiting for go-ahead to publish).
- [x] License chosen and `LICENSE` file added (decision D1).
- [x] Project name checked (decision D2).
- [x] `CLEAN_ROOM.md`, `CONTRIBUTING.md`, `CODE_OF_CONDUCT.md`, issue and PR templates.
- [x] CI workflow written (`.github/workflows/ci.yml`) for Windows and Linux:
  - `cargo fmt --check`, `cargo clippy -D warnings`, `cargo test`
  - `tsc`, `vite build`
  - Passes locally on Windows; first run on GitHub still pending.
- [x] **Conformance harness:**
  - `tests/conformance/**/*.m` files, each with an expected-output file next to it.
  - A runner: `cargo run -p beany-cli -- conformance`.
  - A per-area pass/fail report, with optional JSON output.
- [ ] **Compatibility corpus:**
  - [x] Scaffold in `tests/corpus`, scored in CI.
  - [ ] Add real programs: course files (with permission) and permissively licensed `.m` repositories. Target at least 30 programs before M1.
- [x] Nightly installer workflow written (`.github/workflows/nightly.yml`); first run on GitHub still pending.

**Exit:**
- A fresh clone builds on both platforms with one command.
- CI publishes the test results and the corpus score.

## M1 — Language completeness (6–8 weeks)

Everything a typical script or function file can contain, except `classdef`.

- [ ] Complex numbers end to end: literals `1i`/`2j`, arithmetic, `abs`/`angle`/`conj`/`real`/`imag`, complex `eig`/`roots`/`pole`, display.
- [ ] N-D arrays: `zeros(2,3,4)`, N-D indexing, `cat(3, ...)`, `size`/`squeeze`/`permute`/`ndims`.
- [ ] Nested functions with shared workspace.
- [ ] `private/` folders, `+package` folders and `import`.
- [ ] `onCleanup`, `inputname`, `inputParser`, `validateattributes`, `arguments` blocks.
- [ ] Full cs-list semantics, including `[s.f] = deal(...)` and `c{:} = ...` on the left side.
- [ ] `input()`, `keyboard` and `pause` with user interaction, through a UI round trip.
- [ ] `containers.Map`, `datetime`/`duration` basics, `string` arrays in full.
- [ ] Warning and error IDs, `MException` methods, `warning('off', id)`, `lasterr`.
- [ ] `evalin`/`assignin` with correct caller semantics.
- [ ] Error messages match the reference software's wording where tests check them.

**Exit:** language conformance suite at least 95% passing, and corpus at least 60%.

## M2 — Classes (`classdef`) (4–6 weeks)

- [ ] Value and handle classes, properties (with defaults and validation), methods, constructors.
- [ ] Static methods, `Access`/`SetAccess`, `Dependent` properties with get/set methods.
- [ ] Inheritance, `super` calls, abstract classes, `isa`.
- [ ] Operator overloading (`plus`, `mtimes`, `subsref`/`subsasgn`, `disp`).
- [ ] Enumerations and events/listeners (basic).
- [ ] `@folder` class directories.

**Exit:** OOP conformance suite at least 95%, and corpus at least 70%.

## M3 — Performance (4 weeks)

- [ ] Copy-on-write arrays (shared storage), so passing and assigning large matrices no longer copies them.
- [ ] A bytecode compiler and register VM that replaces the tree walker on hot paths.
- [ ] Scalar fast paths and type specialization in loops.
- [ ] Fast linear algebra (`faer` for dense) and multithreaded elementwise ops for large arrays.
- [ ] Benchmark suite in CI, which fails on more than a 10% regression.

**Exit:**
- Scalar loops run at least 20 million iterations per second (today about 1.4 million).
- Vectorized math is within 2× of NumPy on the benchmark suite.

## M4 — Core numerical library (6 weeks, then ongoing)

- [ ] Linear algebra: complex `eig`/`svd`/`schur`/`hess`, `expm`/`logm`/`sqrtm`, `lsqminnorm`, `orth`/`null`, condition estimates.
- [ ] Sparse matrices: `sparse`/`full`/`spdiags`/`speye`, sparse `\`, `eigs`.
- [ ] FFT family (`rustfft`): `fft`/`ifft`/`fft2`/`fftshift`, `conv` via FFT.
- [ ] ODE solvers: `ode45`, `ode23`, `ode15s`, `ode113`, with `odeset` options and events.
- [ ] Integration and roots: `integral`/`integral2`, `quadgk`, `fzero`, `fminbnd`, `fminsearch`.
- [ ] Interpolation: `interp1` (all methods), `interp2`, `spline`, `pchip`, `griddedInterpolant`.
- [ ] File I/O:
  - `textscan`, `fscanf`, `readtable`/`readmatrix`/`readcell` (CSV and XLSX)
  - `jsonencode`/`jsondecode`
  - `imread`/`imwrite`
  - `.mat` v7.3 (HDF5)
- [ ] Data tools: `accumarray`, `histcounts2`, `movmean`/`movsum`, `smoothdata`, `cumtrapz`, `rescale`, `normalize`.

**Exit:** numerics conformance suite at least 95%, and corpus at least 80%.

## M5 — Graphics system (6 weeks)

- [ ] Handle graphics object tree: figure → axes → line/surface/patch/text/image objects, plus `get`/`set`/`gcf`/`gca`/`gco`/`findobj`/`delete`.
- [ ] Complete axes properties: ticks, labels, limits, scales, colors, fonts, `yyaxis`, `polaraxes`.
- [ ] `patch`, `fill`, `quiver`, `contour3`, `waterfall`, `bar3`, `pcolor`, `polarplot`, `histogram2`.
- [ ] Interactive zoom/pan/rotate, data tips, docked or undocked figures, figure toolbar.
- [ ] Export: `saveas`/`exportgraphics`/`print` to PNG, SVG and PDF.
- [ ] **Decision point:** keep Plotly or move to our own WebGL/wgpu renderer (decision D3), based on fidelity and animation speed measured here.

**Exit:** graphics conformance suite (image comparison) at least 90%, and corpus at least 85%.

## M6 — Desktop IDE (6 weeks)

- [ ] **Debugger:** breakpoints (including conditional), step in/over/out, call-stack panel, inspect variables in any frame, `dbstop if error`.
- [ ] **Editor:** signature help, go to definition, find references, rename, code analyzer warnings, auto-formatting.
- [ ] **Variable editor:** edit cells, plots from a selection.
- [ ] **Files:** file search, recent files, workspace save/load (`.mat`), command history search.
- [ ] **Code tools:** profiler (time per line), and a help browser with our own written documentation.
- [ ] **App:** preferences dialog (fonts, theme, keyboard shortcuts), saved layouts.

**Exit:** a usability test of 5 people each completing a set of debugging and editing tasks, with no blocking issues.

## Release 1.0

- [ ] Corpus at least 90%, all conformance suites at least 95%.
- [ ] Signed installers for Windows and Linux, auto-update, documentation website.
- [ ] No open crash or data-loss bugs.

---

## M7 — Toolboxes (after 1.0, ongoing)

Ordered by how often course and engineering code uses them:

1. **Control systems:** `tf`/`ss`/`zpk` algebra, `feedback`, `series`, `bode`, `nyquist`, `rlocus`, `margin`, `impulse`, `lsim`, `c2d`/`d2c`, `pid`.
2. **Signal processing:** `filter`/`filtfilt`, `butter`/`cheby1`/`fir1`, `freqz`, `pwelch`, `spectrogram`, `resample`, `xcorr`.
3. **Statistics:** `fitlm`, `anova1`, `ttest`/`ttest2`, `vartest2`, `regress`, `prctile`, `boxplot`, more distributions.
4. **Optimization:** `fminunc`, `fmincon`, `linprog`, `quadprog`, `lsqcurvefit`, `lsqnonlin`.
5. **Image processing basics** and **curve fitting**.
6. **Symbolic math:** evaluated separately; a large engine that may build on an existing permissive CAS.

**Package manager:** `beany install <pkg>` from a community registry, so others can contribute toolboxes.

## M8 — Block-diagram simulator (after 1.0, about 12+ weeks)

- [ ] **Open model format:** JSON in a zip. Opening `.slx` (zipped XML) is a later interoperability step.
- [ ] **Canvas editor:** blocks, ports, signal lines, subsystems, parameter dialogs.
- [ ] **Solver engine:** fixed and variable step, continuous (ODE) and discrete states, zero-crossing detection.
- [ ] **Block library:**
  - Sources: Step, Sine, Constant, Clock, From Workspace.
  - Sinks: Scope, To Workspace, Display.
  - Math, Gain, Sum, Product.
  - Integrator, Transfer Fcn, State-Space, PID, Saturation.
  - Discrete blocks, Mux/Demux, Switch, Logic.
  - A user-code block that runs `.m` functions.
- [ ] Scope viewer, signal logging to the workspace, scripting from the command window (`sim`).

---

## Decisions needed

| # | Decision | Status |
|---|---|---|
| D1 | License | **Decided 2026-10-15: Apache-2.0** (permissive, with a patent grant). |
| D2 | Project name | **Decided 2026-10-15: BeanyLab.** A web and GitHub search found no software or repository with this name; the nearest is BeagleLAB, an unrelated embedded-hardware project. A formal trademark search is still advisable before any commercial use. |
| D3 | Plot renderer | Open. Decide at M5 using measurements. |
| D4 | Hosting | **Decided 2026-10-15: GitHub** (CI, releases, issues, discussions). |

## Progress log

| Date | Milestone | Corpus score | Notes |
|---|---|---|---|
| 2026-10-15 | v0.1 | not measured yet | Course files cv2, cv3, cv4 run. |
| 2026-10-16 | M0 in progress | 2 / 2 (corpus too small to mean much) | License, docs, CI, conformance runner (13/13), first commit. |

## Effort estimate

These estimates assume one developer working with an AI assistant, about 20–30 hours a week:
- M0–M6, through 1.0: roughly **8–10 months**.
- Toolboxes and the simulator: another **6–12 months**.

The estimates will be updated at each milestone exit using the actual pace.

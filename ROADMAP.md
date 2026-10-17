# BeanyLab Roadmap

The plan for building BeanyLab into a working, open-source numeric computing environment that runs existing `.m` code on Windows and Linux. We follow it milestone by milestone. Changes to this plan happen by editing this file, never silently.

## Goal

**Version 1.0:** a student or engineer can install BeanyLab, open their existing `.m` files, and run them unmodified. That covers scripts, functions, classes, plots, `.mat` data and the common toolbox functions used in university engineering courses. It also includes a block-diagram simulator for building and running dynamic-system models.

**Measured by:**
- BeanyLab runs at least **90% of the compatibility corpus** (see M0) with correct output.
- The simulator passes its reference-model suite (see M1).

After 1.0, wider toolbox coverage follows (M8).

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
- [x] Public GitHub repository: github.com/greenythebeany/BeanyLab.
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
  - [ ] Add real programs: course files (with permission) and permissively licensed `.m` repositories. Target at least 30 programs before M2, the first language milestone.
- [x] Nightly installer workflow written (`.github/workflows/nightly.yml`); first run on GitHub still pending.

**Exit:**
- A fresh clone builds on both platforms with one command.
- CI publishes the test results and the corpus score.

## M1 — Block-diagram simulator (10–12 weeks)

Moved here from after 1.0 on 2026-10-16 (decision D5). A visual editor for dynamic-system models, plus the engine that simulates them. It builds on the existing interpreter and figure system.

Because the language milestones now come later, the simulator only uses language features that already work. Its blocks are implemented in Rust, except the user-code block. The ODE solvers it needs are built here and also exposed as `ode45`/`ode23` at the command line, which moves those two items out of M5.

**Model format and engine**
- [x] **Open model format:** a `.bmdl` file holding blocks, parameters, connections, subsystems and solver settings. Plain JSON rather than JSON inside a zip, so models diff well in version control (changed 2026-10-17). Versioned.
  - [x] Document the format in `docs/model-format.md`.
- [x] **Execution order:** sort blocks by data dependency, detect algebraic loops and report only the blocks on the loop. Signal widths flow dynamically.
- [x] **Sample times:** continuous, discrete (per-block period), inherited, and constant.
  - [x] Per-block sample-time offsets: `ts = [period offset]` in the dialogs.
- [x] **Solvers:**
  - Fixed step: Euler (ode1), Heun (ode2), RK4 (ode4).
  - Variable step: Dormand–Prince 5(4) (ode45) and Bogacki–Shampine 3(2) (ode23), with relative and absolute tolerances.
  - [x] Stiff solver: Rosenbrock `ode23s` (also used for `ode15s` until a BDF solver lands in M5), in models and on the command line.
  - [x] `ode45`/`ode23` (with `odeset` RelTol/AbsTol/MaxStep/InitialStep) as command-line functions.
- [x] **Zero-crossing detection** by bisection, for Step, Ramp, Pulse, Switch, Saturation, Dead Zone, Relay, Abs, Sign, Relational Operator, integrator limits and integrator resets.
- [x] **Model parameters** from the workspace (`K = 2` in the command window, `K` in a Gain block), evaluated before the simulation starts; errors name the block and parameter.
- [x] **Command-line control:** `out = sim('model', 'StopTime', '5')`, `get_param`, `set_param`, `open_system`.
  - [x] `new_system`, `load_system`, `add_block`, `add_line`, `save_system`, `close_system` for building models from scripts.

**Block library** (each block with a parameter dialog and an icon; every block is exercised by `every_library_block_runs`)
- [x] **Sources:** Constant, Step, Ramp, Sine Wave, Pulse Generator, Clock, Signal Generator, From Workspace, Random Number.
- [x] **Sinks:** Scope, To Workspace, Display, Terminator, Stop Simulation, Outport (logged as `out.yout`).
- [x] **Math:** Gain (scalar, vector, matrix), Sum, Product, Bias, Abs, Sign, Math Function, Trigonometric Function, MinMax, Dot Product, Fcn (expression).
- [x] **Continuous:** Integrator (limits, reset, external initial condition, state port), Derivative, Transfer Fcn, State-Space, Zero-Pole, Transport Delay, PID Controller.
- [x] **Discrete:** Unit Delay, Discrete Transfer Fcn, Discrete State-Space, Discrete-Time Integrator, Zero-Order Hold, Discrete PID.
- [x] **Nonlinear:** Saturation, Dead Zone, Relay, Rate Limiter, Quantizer, 1-D Lookup Table.
- [x] **Routing and logic:** Mux, Demux, Switch, Multiport Switch, Goto/From, Bus Creator/Selector, Relational Operator, Logical Operator.
- [x] **Structure:** Subsystem, Enabled Subsystem, Inport, Outport.
- [x] **User code:** User Function block calls a `.m` function; Fcn block evaluates an expression in `u`.

**Editor (in the desktop app)**
- [x] Model editor tab: searchable block library, drag-and-drop canvas, ports, orthogonal signal lines with fan-out, zoom and pan, multi-select, copy/paste, undo/redo, delete.
- [x] Double-click a block to edit its parameters; double-click a subsystem to open it; breadcrumb navigation.
- [x] Run and Stop, stop time, solver choice and solver settings; errors highlight the block that caused them.
  - [x] Pause and resume.
- [x] **Scope window:** traces update during the simulation in the desktop app, autoscale, zoom, pan and image export.
  - [x] One axes per input (Scope "Axes" parameter).
  - [x] Time cursor with a read-out of every signal.
- [x] Logged signals appear in the workspace as `out` (`out.tout`, To Workspace variables, `out.yout`).
- [x] Current Folder shows `.bmdl` files with the model icon; double-click opens them; right-click → Simulate.
- [x] Display blocks show their value on the canvas, updated live during the run.
- [x] Each model opens in its own window in the desktop app; figures open as their own OS windows (maximize, Tile Figures).
- [x] Double-click on empty canvas opens a quick block search; Space fits the diagram to the window.

- [x] **Interoperability:** open `.slx` (all package layouts: XML, JSON settings, separate system parts) and `.mdl` files; masked subsystems, scope logging, local Goto tags, active configuration set. Unsupported blocks become zero-output placeholders with a note. Save as `.slx` (the default) or `.bmdl`; exported models were checked to load and simulate in the reference software.
  - [ ] Signal Builder data, Stateflow charts, triggered subsystems, signal logging (`logsout`).

**Not in M1:** code generation and hardware targets are out of scope.

**Exit:**
- [x] **Reference-model suite** (`crates/beany-core/tests/sim.rs`, models in `examples/models`), written by us with known analytic or independently computed answers, all within tolerance:
  - first-order lag step response
  - mass–spring–damper
  - closed-loop PID speed control
  - discrete low-pass filter
  - bouncing ball (zero-crossing and state reset)
  - Van der Pol oscillator
  - a model using the user-code block
- [x] A model with an algebraic loop produces a clear error that points at the loop.
- [ ] Three people who haven't seen the editor build and run a feedback-control model from scratch, with no blocking issues.

## M2 — Language completeness (6–8 weeks)

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

## M3 — Classes (`classdef`) (4–6 weeks)

- [ ] Value and handle classes, properties (with defaults and validation), methods, constructors.
- [ ] Static methods, `Access`/`SetAccess`, `Dependent` properties with get/set methods.
- [ ] Inheritance, `super` calls, abstract classes, `isa`.
- [ ] Operator overloading (`plus`, `mtimes`, `subsref`/`subsasgn`, `disp`).
- [ ] Enumerations and events/listeners (basic).
- [ ] `@folder` class directories.

**Exit:** OOP conformance suite at least 95%, and corpus at least 70%.

## M4 — Performance (4 weeks)

The simulator's inner loop is part of the benchmark suite, so model runs get faster too.

- [ ] Copy-on-write arrays (shared storage), so passing and assigning large matrices no longer copies them.
- [ ] A bytecode compiler and register VM that replaces the tree walker on hot paths.
- [ ] Scalar fast paths and type specialization in loops.
- [ ] Fast linear algebra (`faer` for dense) and multithreaded elementwise ops for large arrays.
- [ ] Benchmark suite in CI, which fails on more than a 10% regression.

**Exit:**
- Scalar loops run at least 20 million iterations per second (today about 1.4 million).
- Vectorized math is within 2× of NumPy on the benchmark suite.

## M5 — Core numerical library (6 weeks, then ongoing)

- [ ] Linear algebra: complex `eig`/`svd`/`schur`/`hess`, `expm`/`logm`/`sqrtm`, `lsqminnorm`, `orth`/`null`, condition estimates.
- [ ] Sparse matrices: `sparse`/`full`/`spdiags`/`speye`, sparse `\`, `eigs`.
- [ ] FFT family (`rustfft`): `fft`/`ifft`/`fft2`/`fftshift`, `conv` via FFT.
- [ ] ODE solvers: `ode15s`, `ode113`, `odeset` options and events. (`ode45` and `ode23` arrive with the simulator in M1.)
- [ ] Integration and roots: `integral`/`integral2`, `quadgk`, `fzero`, `fminbnd`, `fminsearch`.
- [ ] Interpolation: `interp1` (all methods), `interp2`, `spline`, `pchip`, `griddedInterpolant`.
- [ ] File I/O:
  - `textscan`, `fscanf`, `readtable`/`readmatrix`/`readcell` (CSV and XLSX)
  - `jsonencode`/`jsondecode`
  - `imread`/`imwrite`
  - `.mat` v7.3 (HDF5)
- [ ] Data tools: `accumarray`, `histcounts2`, `movmean`/`movsum`, `smoothdata`, `cumtrapz`, `rescale`, `normalize`.

**Exit:** numerics conformance suite at least 95%, and corpus at least 80%.

## M6 — Graphics system (6 weeks)

- [ ] Handle graphics object tree: figure → axes → line/surface/patch/text/image objects, plus `get`/`set`/`gcf`/`gca`/`gco`/`findobj`/`delete`.
- [ ] Complete axes properties: ticks, labels, limits, scales, colors, fonts, `yyaxis`, `polaraxes`.
- [ ] `patch`, `fill`, `quiver`, `contour3`, `waterfall`, `bar3`, `pcolor`, `polarplot`, `histogram2`.
- [ ] Interactive zoom/pan/rotate, data tips, docked or undocked figures, figure toolbar.
- [ ] Export: `saveas`/`exportgraphics`/`print` to PNG, SVG and PDF.
- [ ] **Decision point:** keep Plotly or move to our own WebGL/wgpu renderer (decision D3), based on fidelity and animation speed measured here, including the simulator's live scopes.

**Exit:** graphics conformance suite (image comparison) at least 90%, and corpus at least 85%.

## M7 — Desktop IDE (6 weeks)

- [ ] **Debugger:** breakpoints (including conditional), step in/over/out, call-stack panel, inspect variables in any frame, `dbstop if error`.
- [ ] **Editor:** signature help, go to definition, find references, rename, code analyzer warnings, auto-formatting.
- [ ] **Variable editor:** edit cells, plots from a selection.
- [ ] **Files:** file search, recent files, workspace save/load (`.mat`), command history search.
- [ ] **Code tools:** profiler (time per line), and a help browser with our own written documentation.
- [ ] **App:** preferences dialog (fonts, theme, keyboard shortcuts), saved layouts.

**Exit:** a usability test of 5 people each completing a set of debugging and editing tasks, with no blocking issues.

## Release 1.0

- [ ] Corpus at least 90%, all conformance suites at least 95%, simulator reference-model suite passing.
- [ ] Signed installers for Windows and Linux, auto-update, documentation website.
- [ ] No open crash or data-loss bugs.

---

## M8 — Toolboxes (after 1.0, ongoing)

Ordered by how often course and engineering code uses them:

1. **Control systems:** `tf`/`ss`/`zpk` algebra, `feedback`, `series`, `bode`, `nyquist`, `rlocus`, `margin`, `impulse`, `lsim`, `c2d`/`d2c`, `pid`.
2. **Signal processing:** `filter`/`filtfilt`, `butter`/`cheby1`/`fir1`, `freqz`, `pwelch`, `spectrogram`, `resample`, `xcorr`.
3. **Statistics:** `fitlm`, `anova1`, `ttest`/`ttest2`, `vartest2`, `regress`, `prctile`, `boxplot`, more distributions.
4. **Optimization:** `fminunc`, `fmincon`, `linprog`, `quadprog`, `lsqcurvefit`, `lsqnonlin`.
5. **Image processing basics** and **curve fitting**.
6. **Symbolic math:** evaluated separately; a large engine that may build on an existing permissive CAS.

**Package manager:** `beany install <pkg>` from a community registry, so others can contribute toolboxes and block libraries.

**Simulator extensions:** `.slx` import, more block libraries (physical modeling, state machines), and code generation are evaluated here.

---

## Decisions needed

| # | Decision | Status |
|---|---|---|
| D1 | License | **Decided 2026-10-15: Apache-2.0** (permissive, with a patent grant). |
| D2 | Project name | **Decided 2026-10-15: BeanyLab.** A web and GitHub search found no software or repository with this name; the nearest is BeagleLAB, an unrelated embedded-hardware project. A formal trademark search is still advisable before any commercial use. |
| D3 | Plot renderer | Open. Decide at M6 using measurements. |
| D4 | Hosting | **Decided 2026-10-15: GitHub** (CI, releases, issues, discussions). |
| D5 | Simulator timing | **Decided 2026-10-16: build it right after M0 (as M1)**, ahead of the language milestones, and include it in 1.0. |

## Progress log

| Date | Milestone | Corpus score | Notes |
|---|---|---|---|
| 2026-10-15 | v0.1 | not measured yet | Course files cv2, cv3, cv4 run. |
| 2026-10-16 | M0 in progress | 2 / 2 (corpus too small to mean much) | License, docs, CI, conformance runner (13/13), first commit. |
| 2026-10-17 | M1 in progress | 2 / 2 | Repo public. Simulator engine, 54 blocks, editor, scopes; reference-model suite 11/11. |

## Effort estimate

These estimates assume one developer working with an AI assistant, about 20–30 hours a week:
- M0–M7, through 1.0 including the simulator: roughly **11–13 months**.
- Toolboxes after 1.0: another **4–8 months** for the first set, then ongoing.

The estimates will be updated at each milestone exit using the actual pace.

# BeanyLab

An open-source numeric computing environment for Windows and Linux that opens and runs existing `.m` files. Licensed under [Apache-2.0](LICENSE).

> BeanyLab is an independent clean-room project. It is not affiliated with The MathWorks, Inc. See [CLEAN_ROOM.md](CLEAN_ROOM.md) and [NOTICE](NOTICE).

- **Engine** (`crates/beany-core`, Rust): lexer, parser and interpreter. About 430 built-in functions, linear algebra (nalgebra), `.mat` files, transfer functions, tables, and a figure model.
- **Desktop app** (`src-tauri` + `ui`, Tauri 2 + TypeScript): toolstrip, Current Folder, Editor (Monaco), Command Window, Workspace, Command History, variable editor, and figure windows (Plotly).
- **CLI** (`crates/beany-cli`): `beany` REPL, `beany file.m` runner, `beany conformance` test runner, and `beany serve`, which runs the UI in a browser.

## Status

Early development (v0.1). The plan, milestones and progress are in [ROADMAP.md](ROADMAP.md).

## Run it

```bash
npm install
npm run tauri dev                                     # desktop app with hot reload
npx tauri build                                       # installers in target/release/bundle
cargo run -p beany-cli                                # terminal REPL
cargo run -p beany-cli -- examples/fibonacci_demo.m   # run a file
npm run serve                                         # UI in a browser at http://127.0.0.1:7777
```

Linux builds need the Tauri system packages: `libwebkit2gtk-4.1-dev`, `libgtk-3-dev`, `librsvg2-dev`, `patchelf`.

## Tests

```bash
cargo test --workspace                                  # engine tests
cargo run -p beany-cli -- conformance tests/conformance # .m conformance suite
cargo run -p beany-cli -- conformance tests/corpus      # real-world compatibility corpus
```

## Language support

- **Works:**
  - Matrices, ranges, `end` in indexing, logical indexing, growth and deletion, implicit expansion.
  - `if`/`for`/`while`/`switch`/`try`/`break`/`continue`/`return`.
  - Function files (with or without `end`), local functions in scripts, `nargin`/`nargout`, `varargin`/`varargout`, recursion, `global`, `persistent`.
  - Anonymous functions; cells and cs-lists; structs and struct arrays; char arrays and `"string"` scalars; integer and logical classes.
  - Command syntax (`hold on`, `format long`); errors with identifiers.
  - `.mat` load/save; `tf`/`step`/`dcgain`/`pole`; `table`/`writetable`; statistics distributions.
- **Plots:** `plot`, `plot3`, `scatter(3)`, `bar(h)`, `histogram`, `stem`, `stairs`, `area`, `errorbar`, `surf`, `mesh`, `contour(f)`, `imagesc`, `pie`, `fplot`, log axes, `subplot`, `tiledlayout`/`nexttile`, `uitable`, `legend`, `colorbar`, `text`.
- **Not yet:** complex numbers, arrays with more than 2 dimensions, `classdef`, debugger, `input()`, live scripts, the block-diagram simulator. These are scheduled in [ROADMAP.md](ROADMAP.md).

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). All contributions must follow [CLEAN_ROOM.md](CLEAN_ROOM.md).

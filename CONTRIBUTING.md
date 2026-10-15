# Contributing to BeanyLab

Thanks for helping. Please read [CLEAN_ROOM.md](CLEAN_ROOM.md) first: every contribution must follow it.

## How work is organized

The project follows [ROADMAP.md](ROADMAP.md) one milestone at a time. Pick an open item from the current milestone, or a bug labelled `compat` (a real `.m` file that fails). Larger ideas outside the current milestone go into a GitHub issue labelled `backlog`.

## Building

Requirements:
- Rust (stable)
- Node.js 20+
- **Windows:** MSVC Build Tools and WebView2 (preinstalled on Windows 11).
- **Linux:** `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev patchelf`

```bash
npm install
npm run tauri dev                         # desktop app
cargo run -p beany-cli                    # terminal REPL
cargo run -p beany-cli -- path/to/file.m  # run a file
```

## Tests

Every feature needs tests:

- **Conformance tests** (preferred for language and library behavior) live in `tests/conformance/<area>/`. Each test is a `name.m` file plus a `name.out` file holding the exact expected command-window output. Run them all:

  ```bash
  cargo run -p beany-cli -- conformance tests/conformance
  ```

  To create or refresh an expected-output file after checking the output is right, use `--bless`.

- **Rust tests** for engine internals: `cargo test --workspace`.

Before opening a pull request:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p beany-cli -- conformance tests/conformance
npx tsc --noEmit -p .
```

## Compatibility reports

If a `.m` file doesn't run, open an issue with the smallest snippet that shows the problem, what you expected, and what BeanyLab did. Only share code you are allowed to share.

## Pull requests

- One topic per pull request, with tests.
- Sign off your commits (`git commit -s`) to confirm the [CLEAN_ROOM.md](CLEAN_ROOM.md) contributor statement.
- Code style: `rustfmt` defaults; TypeScript follows the existing files.

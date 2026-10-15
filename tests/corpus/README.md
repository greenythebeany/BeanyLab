# Compatibility corpus

Real-world `.m` programs used to measure how much existing code BeanyLab runs. A program passes when it runs to the end without an error. If it has a matching `.out` file, its output must also match that file.

Run it:

```bash
cargo run -p beany-cli -- conformance tests/corpus
```

The score is tracked in the progress log in [ROADMAP.md](../../ROADMAP.md).

## Adding programs

- Only add code you have the right to share: your own, or code under a permissive open-source license (record the source and license in `SOURCES.md` in that folder).
- Never add MathWorks example code or toolbox files.
- Put each program in its own folder with any data files it needs.

# Clean-room rules

BeanyLab reimplements a programming language and its function library so that existing `.m` code runs. Implementing a language's behavior is lawful in most places; copying someone else's code, documentation or artwork is not. These rules keep the project on the right side of that line. Every contribution must follow them.

## Never do this

1. **Don't read or copy MathWorks source code**, including the plain-text `.m` files that ship inside a MATLAB installation. Being readable doesn't make them free to use.
2. **Don't decompile, disassemble or otherwise reverse-engineer MathWorks binaries** (`.dll`, `.p`, `.mexw64`, `.jar`, and so on).
3. **Don't copy MathWorks documentation text, examples or help comments** into code, comments, tests or docs. Write your own words and your own examples.
4. **Don't copy MathWorks test suites**, or expected outputs taken from their documentation.
5. **Don't use MathWorks icons, artwork, UI graphics or color schemes.** BeanyLab's look is its own.
6. **Don't use "MATLAB" or "Simulink"** in product names, logos or anything that suggests endorsement. Describing compatibility ("runs `.m` files") is fine.
7. **Don't copy GPL-licensed code** (for example from GNU Octave). BeanyLab is Apache-2.0. Studying a published algorithm in a textbook or paper is fine.

## Where behavior comes from

- **Observable behavior:** what code does when you run it. Writing a small `.m` script, running it in software you're licensed to use, and recording what happens is how compatibility is checked. Record the observation (input → output), not any implementation detail.
- **Public specifications and standards:** IEEE 754, the published MAT-file Level 5 layout, file formats like XLSX and HDF5.
- **Mathematics:** algorithms from textbooks and papers, such as Golub & Van Loan, Numerical Recipes concepts, or Hairer & Wanner for ODEs, written up in your own code.
- **Your own tests**, written from the above.

## Error messages and function names

Function names, argument orders and error-message wording are part of the language interface that user code depends on, for example code that checks `err.identifier`. Matching them is interoperability, not copying. Keep messages short and functional; don't copy explanatory documentation prose.

## If you're unsure

Open a discussion before writing the code. When in doubt, leave it out.

## Contributor statement

By submitting a pull request you confirm that your contribution follows these rules and that you have the right to license it under Apache-2.0. Commits carry a `Signed-off-by` line ([Developer Certificate of Origin](https://developercertificate.org/)).

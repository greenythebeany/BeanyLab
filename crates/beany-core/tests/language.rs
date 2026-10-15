//! Behavior tests for the language engine. Each test runs code and checks values or output.

use beany_core::value::{NumClass, Value};
use beany_core::Interp;

fn run(code: &str) -> Interp {
    let mut it = Interp::new();
    if let Err(e) = it.eval_string(code) {
        panic!("code failed: {}\n---\n{}", e.report(), code);
    }
    it
}

fn out(code: &str) -> String {
    let mut it = run(code);
    it.take_output()
}

fn err(code: &str) -> String {
    let mut it = Interp::new();
    match it.eval_string(code) {
        Ok(()) => panic!("expected an error from: {}", code),
        Err(e) => e.message,
    }
}

fn num(it: &Interp, name: &str) -> Vec<f64> {
    it.get_var(name)
        .unwrap_or_else(|| panic!("no variable {}", name))
        .to_num()
        .unwrap()
        .data
}

fn scalar(code: &str, name: &str) -> f64 {
    let it = run(code);
    num(&it, name)[0]
}

fn dims(it: &Interp, name: &str) -> (usize, usize) {
    it.get_var(name).unwrap().dims()
}

fn text(it: &Interp, name: &str) -> String {
    it.get_var(name).unwrap().text().unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * (1.0 + b.abs())
}

// ---------------- syntax ----------------

#[test]
fn matrix_literals_and_whitespace() {
    let it = run("a = [1 -2]; b = [1 - 2]; c = [1 , 2 ; 3 4]; d = [1 2\n3 4]; e = [a' a'];");
    assert_eq!(num(&it, "a"), vec![1.0, -2.0]);
    assert_eq!(num(&it, "b"), vec![-1.0]);
    assert_eq!(num(&it, "c"), vec![1.0, 3.0, 2.0, 4.0]);
    assert_eq!(num(&it, "d"), vec![1.0, 3.0, 2.0, 4.0]);
    assert_eq!(dims(&it, "e"), (2, 2));
}

#[test]
fn transpose_versus_strings() {
    let it = run("x = [1 2 3]'; s = 'it''s'; y = x'; z = {'a' 'b'};");
    assert_eq!(dims(&it, "x"), (3, 1));
    assert_eq!(text(&it, "s"), "it's");
    assert_eq!(dims(&it, "y"), (1, 3));
    assert_eq!(dims(&it, "z"), (1, 2));
}

#[test]
fn operator_precedence() {
    assert_eq!(scalar("x = -2^2;", "x"), -4.0);
    assert_eq!(scalar("x = 2^-1;", "x"), 0.5);
    assert_eq!(scalar("x = 1 + 2 * 3 ^ 2;", "x"), 19.0);
    assert_eq!(scalar("x = ~0 + 1;", "x"), 2.0);
    let it = run("r = 1:3 + 1;");
    assert_eq!(num(&it, "r"), vec![1.0, 2.0, 3.0, 4.0]);
    assert_eq!(scalar("x = 3 > 2 && 2 > 1 || 0;", "x"), 1.0);
}

#[test]
fn comments_continuations_and_block_comments() {
    let it = run("a = 1 + ... comment here\n 2; % trailing\n%{\nb = 5;\n%}\nc = 3;");
    assert_eq!(num(&it, "a"), vec![3.0]);
    assert!(it.get_var("b").is_none());
    assert_eq!(num(&it, "c"), vec![3.0]);
}

#[test]
fn ranges() {
    let it = run("a = 1:5; b = 10:-3:1; c = 0:0.25:1; d = 5:1;");
    assert_eq!(num(&it, "a"), vec![1.0, 2.0, 3.0, 4.0, 5.0]);
    assert_eq!(num(&it, "b"), vec![10.0, 7.0, 4.0, 1.0]);
    assert_eq!(num(&it, "c").len(), 5);
    assert_eq!(dims(&it, "d"), (1, 0));
}

#[test]
fn command_syntax() {
    let o = out("format long\nx = pi\nformat short\ny = pi");
    assert!(o.contains("3.141592653589793"), "{}", o);
    assert!(o.contains("y = 3.1416"), "{}", o);
    let it = run("hold on; x = 1; disp hello");
    assert_eq!(num(&it, "x"), vec![1.0]);
}

#[test]
fn command_syntax_falls_back_for_variables() {
    let it = run("x = 5;\ny = x -1;");
    assert_eq!(num(&it, "y"), vec![4.0]);
}

// ---------------- indexing ----------------

#[test]
fn indexing_basics() {
    let it = run("A = magic(4); a = A(2,3); b = A(end, :); c = A(:, end); d = A(end); e = A([1 2], [1 end]); f = A(:)'; g = A(A > 12)';");
    assert_eq!(num(&it, "a"), vec![10.0]);
    assert_eq!(num(&it, "b"), vec![4.0, 14.0, 15.0, 1.0]);
    assert_eq!(num(&it, "c"), vec![13.0, 8.0, 12.0, 1.0]);
    assert_eq!(num(&it, "d"), vec![1.0]);
    assert_eq!(num(&it, "e"), vec![16.0, 5.0, 13.0, 8.0]);
    assert_eq!(num(&it, "f").len(), 16);
    assert_eq!(num(&it, "g"), vec![16.0, 14.0, 15.0, 13.0]);
}

#[test]
fn indexed_assignment_growth_and_deletion() {
    let it = run("x = []; x(3) = 7; y = 1:5; y(2) = []; z = eye(3); z(:, 2) = []; w(2,3) = 1; v = 1:3; v(end+1) = 4;");
    assert_eq!(num(&it, "x"), vec![0.0, 0.0, 7.0]);
    assert_eq!(num(&it, "y"), vec![1.0, 3.0, 4.0, 5.0]);
    assert_eq!(dims(&it, "z"), (3, 2));
    assert_eq!(dims(&it, "w"), (2, 3));
    assert_eq!(num(&it, "v"), vec![1.0, 2.0, 3.0, 4.0]);
}

#[test]
fn logical_indexing_assignment() {
    let it = run("x = 1:6; x(x > 3) = 0; y = magic(3); y(y < 5) = -1;");
    assert_eq!(num(&it, "x"), vec![1.0, 2.0, 3.0, 0.0, 0.0, 0.0]);
    assert!(num(&it, "y").iter().filter(|v| **v == -1.0).count() == 4);
}

#[test]
fn index_errors() {
    assert!(err("x = [1 2 3]; x(5)").contains("Index exceeds"));
    assert!(err("x = [1 2 3]; x(0)").contains("positive integer"));
    assert!(err("x = [1 2 3]; x(1.5)").contains("positive integers"));
}

// ---------------- control flow ----------------

#[test]
fn loops_and_branches() {
    let it = run(
        "s = 0; for k = 1:10, if mod(k, 2) == 0, continue; end, if k > 7, break; end, s = s + k; end\n\
         n = 0; while n < 5, n = n + 2; end\n\
         t = 0; for c = [1 2; 3 4], t = t + c(1) * c(2); end",
    );
    assert_eq!(num(&it, "s"), vec![16.0]);
    assert_eq!(num(&it, "n"), vec![6.0]);
    assert_eq!(num(&it, "t"), vec![1.0 * 3.0 + 2.0 * 4.0]);
}

#[test]
fn switch_statement() {
    let code = "function r = f(x)\nswitch x\n case {1, 2}\n r = 'low';\n case 'abc'\n r = 'text';\n otherwise\n r = 'other';\nend\nend";
    let dir = std::env::temp_dir().join(format!("beany_sw_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("f.m"), code).unwrap();
    let mut it = Interp::new();
    it.cwd = dir.clone();
    it.eval_string("a = f(2); b = f('abc'); c = f(9);").unwrap();
    assert_eq!(text(&it, "a"), "low");
    assert_eq!(text(&it, "b"), "text");
    assert_eq!(text(&it, "c"), "other");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn try_catch_and_error_identifiers() {
    let it = run("try\n error('pkg:bad', 'Value %d is bad', 7);\ncatch e\n id = e.identifier; msg = e.message;\nend");
    assert_eq!(text(&it, "id"), "pkg:bad");
    assert_eq!(text(&it, "msg"), "Value 7 is bad");
    let it = run("try\n x = [1 2](5);\ncatch\n ok = 1;\nend");
    assert_eq!(num(&it, "ok"), vec![1.0]);
}

// ---------------- functions ----------------

fn with_files(files: &[(&str, &str)], code: &str) -> Interp {
    let dir = std::env::temp_dir().join(format!("beany_t_{}_{}", std::process::id(), files[0].0));
    std::fs::create_dir_all(&dir).unwrap();
    for (n, c) in files {
        std::fs::write(dir.join(n), c).unwrap();
    }
    let mut it = Interp::new();
    it.cwd = dir.clone();
    let r = it.eval_string(code);
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = r {
        panic!("{}", e.report());
    }
    it
}

#[test]
fn function_files_with_multiple_outputs_and_nargin() {
    let it = with_files(
        &[("stats2.m", "function [m, s] = stats2(x, w)\n% help text\nif nargin < 2, w = 1; end\nm = mean(x) * w;\ns = helper(x);\nend\nfunction r = helper(x)\nr = sum(x);\nend\n")],
        "[a, b] = stats2([1 2 3]); c = stats2([2 4], 10); n = nargin('stats2');",
    );
    assert_eq!(num(&it, "a"), vec![2.0]);
    assert_eq!(num(&it, "b"), vec![6.0]);
    assert_eq!(num(&it, "c"), vec![30.0]);
    assert_eq!(num(&it, "n"), vec![2.0]);
}

#[test]
fn functions_without_end_keyword() {
    let it = with_files(
        &[(
            "noend.m",
            "function y = noend(x)\ny = twice(x) + 1;\n\nfunction z = twice(x)\nz = 2 * x;\n",
        )],
        "v = noend(4);",
    );
    assert_eq!(num(&it, "v"), vec![9.0]);
}

#[test]
fn varargin_varargout_and_recursion() {
    let it = with_files(
        &[
            ("vsum.m", "function varargout = vsum(varargin)\ntotal = 0;\nfor k = 1:numel(varargin)\n total = total + varargin{k};\nend\nvarargout{1} = total;\nvarargout{2} = numel(varargin);\nend"),
            ("fact.m", "function r = fact(n)\nif n <= 1\n r = 1;\nelse\n r = n * fact(n - 1);\nend\nend"),
        ],
        "[t, n] = vsum(1, 2, 3); f = fact(10);",
    );
    assert_eq!(num(&it, "t"), vec![6.0]);
    assert_eq!(num(&it, "n"), vec![3.0]);
    assert_eq!(num(&it, "f"), vec![3628800.0]);
}

#[test]
fn scripts_with_local_functions() {
    let it = run("y = sq(3) + sq(4);\nfunction r = sq(x)\n r = x^2;\nend");
    assert_eq!(num(&it, "y"), vec![25.0]);
}

#[test]
fn anonymous_functions_capture_values() {
    let it = run("a = 2; f = @(x) a * x + 1; a = 100; y = f(3); g = @(x, y) x + y; z = g(1, 2); h = @sin; w = h(0); k = feval(@max, [3 9 1]);");
    assert_eq!(num(&it, "y"), vec![7.0]);
    assert_eq!(num(&it, "z"), vec![3.0]);
    assert_eq!(num(&it, "w"), vec![0.0]);
    assert_eq!(num(&it, "k"), vec![9.0]);
}

#[test]
fn globals_and_persistents() {
    let it = with_files(
        &[
            ("counter.m", "function n = counter()\npersistent c\nif isempty(c), c = 0; end\nc = c + 1;\nn = c;\nend"),
            ("readg.m", "function v = readg()\nglobal G\nv = G * 2;\nend"),
        ],
        "counter(); counter(); n = counter(); global G; G = 21; v = readg();",
    );
    assert_eq!(num(&it, "n"), vec![3.0]);
    assert_eq!(num(&it, "v"), vec![42.0]);
}

#[test]
fn error_stack_names_function_and_line() {
    let dir = std::env::temp_dir().join(format!("beany_stack_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("boom.m"),
        "function boom()\nx = 1;\ny = x(4);\nend",
    )
    .unwrap();
    let mut it = Interp::new();
    it.cwd = dir.clone();
    let e = it.eval_string("boom()").unwrap_err();
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(e.stack, vec!["boom (line 3)".to_string()]);
}

// ---------------- data types ----------------

#[test]
fn structs_and_struct_arrays() {
    let it = run("s.a = 1; s.b.c = 'deep'; t(2).x = 5; n = numel(t); names = fieldnames(s); q = struct('k', {10, 20}); k2 = q(2).k; has = isfield(s, 'a'); d = s.('a');");
    assert_eq!(num(&it, "n"), vec![2.0]);
    assert_eq!(dims(&it, "names"), (2, 1));
    assert_eq!(num(&it, "k2"), vec![20.0]);
    assert_eq!(num(&it, "has"), vec![1.0]);
    assert_eq!(num(&it, "d"), vec![1.0]);
}

#[test]
fn cells_and_cs_lists() {
    let it = run("c = {1, 'two', [3 4]}; a = c{3}; n = numel(c); d = c(2); m = max(c{3}); [p, q] = c{[1 3]}; z = [c{[1 3]}]; c{5} = 'e'; e = isempty(c{4});");
    assert_eq!(num(&it, "a"), vec![3.0, 4.0]);
    assert_eq!(num(&it, "n"), vec![3.0]);
    assert!(matches!(it.get_var("d").unwrap(), Value::Cell(_)));
    assert_eq!(num(&it, "m"), vec![4.0]);
    assert_eq!(num(&it, "p"), vec![1.0]);
    assert_eq!(num(&it, "q"), vec![3.0, 4.0]);
    assert_eq!(num(&it, "z"), vec![1.0, 3.0, 4.0]);
    assert_eq!(num(&it, "e"), vec![1.0]);
}

#[test]
fn strings_and_chars() {
    let it = run("a = ['abc', 'def']; b = \"hi\" + \" there\"; c = upper('mixed'); d = strrep('a-b-c', '-', '+'); e = num2str(3.14159); f = sprintf('%05.1f|%s|%d', 3.14159, 'x', 42); g = strtrim('  pad  '); h = length('hello'); k = double('A');");
    assert_eq!(text(&it, "a"), "abcdef");
    assert_eq!(text(&it, "b"), "hi there");
    assert_eq!(text(&it, "c"), "MIXED");
    assert_eq!(text(&it, "d"), "a+b+c");
    assert_eq!(text(&it, "e"), "3.1416");
    assert_eq!(text(&it, "f"), "003.1|x|42");
    assert_eq!(text(&it, "g"), "pad");
    assert_eq!(num(&it, "h"), vec![5.0]);
    assert_eq!(num(&it, "k"), vec![65.0]);
}

#[test]
fn string_functions() {
    let it = run("p = strsplit('a,b,,c', ','); j = strjoin({'x','y','z'}, '-'); t = strcmp({'a','b'}, 'b'); r = regexprep('abc123', '\\d', '#'); [tok] = regexp('key=val', '(\\w+)=(\\w+)', 'tokens'); m = regexp('a1b22c333', '\\d+', 'match'); s = contains('hello world', 'wor'); i = strfind('abcabc', 'bc');");
    assert_eq!(dims(&it, "p"), (1, 3));
    assert_eq!(text(&it, "j"), "x-y-z");
    assert_eq!(num(&it, "t"), vec![0.0, 1.0]);
    assert_eq!(text(&it, "r"), "abc###");
    assert_eq!(dims(&it, "m"), (1, 3));
    assert_eq!(num(&it, "s"), vec![1.0]);
    assert_eq!(num(&it, "i"), vec![2.0, 5.0]);
}

#[test]
fn integer_and_logical_classes() {
    let it = run("a = int8(200); b = uint8(5) - 10; c = int32(7) / 2; d = class(true + true); e = class(int16(3) * 2.6); f = int32(2.5);");
    assert_eq!(num(&it, "a"), vec![127.0]);
    assert_eq!(num(&it, "b"), vec![0.0]);
    assert_eq!(num(&it, "c"), vec![4.0]);
    assert_eq!(text(&it, "d"), "double");
    assert_eq!(text(&it, "e"), "int16");
    assert_eq!(num(&it, "f"), vec![3.0]);
    assert!(matches!(
        it.get_var("a").unwrap(),
        Value::Num(_, NumClass::Int(_))
    ));
}

// ---------------- numerics ----------------

#[test]
fn implicit_expansion() {
    let it = run("a = [1 2 3] + [10; 20]; b = [1 2; 3 4] .* [10 100];");
    assert_eq!(dims(&it, "a"), (2, 3));
    assert_eq!(num(&it, "a"), vec![11.0, 21.0, 12.0, 22.0, 13.0, 23.0]);
    assert_eq!(num(&it, "b"), vec![10.0, 30.0, 200.0, 400.0]);
}

#[test]
fn linear_algebra() {
    let it = run("A = [4 -2; 1 1]; x = A \\ [2; 3]; d = det(A); B = inv(A) * A; e = eig([2 0; 0 3]); r = rank(magic(4)); n = norm([3 4]); t = trace(A); k = kron([1 2], [1; 1]);");
    let x = num(&it, "x");
    assert!(close(x[0], 4.0 / 3.0) && close(x[1], 5.0 / 3.0), "{:?}", x);
    assert!(close(num(&it, "d")[0], 6.0));
    let b = num(&it, "B");
    assert!(close(b[0], 1.0) && b[1].abs() < 1e-12 && close(b[3], 1.0));
    assert_eq!(num(&it, "e"), vec![2.0, 3.0]);
    assert_eq!(num(&it, "r"), vec![3.0]);
    assert_eq!(num(&it, "n"), vec![5.0]);
    assert_eq!(num(&it, "t"), vec![5.0]);
    assert_eq!(dims(&it, "k"), (2, 2));
}

#[test]
fn reductions_and_sorting() {
    let it = run("A = [3 1; 2 5]; s = sum(A); s2 = sum(A, 2); m = mean([1 2 3 4]); [mx, ix] = max([4 9 2]); c = cumsum(1:4); [v, i] = sort([3 1 2], 'descend'); u = unique([3 1 3 2 1]); md = median([5 1 3 2]); sd = std([2 4 4 4 5 5 7 9]); p = prod([1 2 3 4]);");
    assert_eq!(num(&it, "s"), vec![5.0, 6.0]);
    assert_eq!(num(&it, "s2"), vec![4.0, 7.0]);
    assert_eq!(num(&it, "m"), vec![2.5]);
    assert_eq!(num(&it, "mx"), vec![9.0]);
    assert_eq!(num(&it, "ix"), vec![2.0]);
    assert_eq!(num(&it, "c"), vec![1.0, 3.0, 6.0, 10.0]);
    assert_eq!(num(&it, "v"), vec![3.0, 2.0, 1.0]);
    assert_eq!(num(&it, "i"), vec![1.0, 3.0, 2.0]);
    assert_eq!(num(&it, "u"), vec![1.0, 2.0, 3.0]);
    assert_eq!(num(&it, "md"), vec![2.5]);
    assert!(close(num(&it, "sd")[0], 2.138089935299395));
    assert_eq!(num(&it, "p"), vec![24.0]);
}

#[test]
fn polynomials_and_interpolation() {
    let it = run("p = polyfit([0 1 2], [1 3 5], 1); y = polyval([1 0 -1], 3); r = roots([1 -3 2]); q = interp1([0 10], [0 100], [2.5 5]); c = conv([1 1], [1 -1]); t = trapz([0 1 2], [0 1 4]);");
    let p = num(&it, "p");
    assert!(close(p[0], 2.0) && close(p[1], 1.0), "{:?}", p);
    assert_eq!(num(&it, "y"), vec![8.0]);
    let r = num(&it, "r");
    assert!(close(r[0], 2.0) && close(r[1], 1.0), "{:?}", r);
    assert_eq!(num(&it, "q"), vec![25.0, 50.0]);
    assert_eq!(num(&it, "c"), vec![1.0, 0.0, -1.0]);
    assert_eq!(num(&it, "t"), vec![3.0]);
}

#[test]
fn construction_functions() {
    let it = run("a = zeros(2, 3); b = ones(2); c = eye(2, 3); d = linspace(0, 1, 5); [X, Y] = meshgrid(1:3, 1:2); r = repmat([1 2], 2, 2); s = reshape(1:6, 2, 3); f = fliplr([1 2 3]); g = diag([1 2]); m = magic(3);");
    assert_eq!(dims(&it, "a"), (2, 3));
    assert_eq!(dims(&it, "b"), (2, 2));
    assert_eq!(num(&it, "c"), vec![1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
    assert_eq!(num(&it, "d"), vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    assert_eq!(dims(&it, "X"), (2, 3));
    assert_eq!(dims(&it, "r"), (2, 4));
    assert_eq!(num(&it, "s"), vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
    assert_eq!(num(&it, "f"), vec![3.0, 2.0, 1.0]);
    assert_eq!(num(&it, "g"), vec![1.0, 0.0, 0.0, 2.0]);
    let m = num(&it, "m");
    // Every row, column and diagonal of a magic square sums to the same value.
    assert_eq!(m[0] + m[3] + m[6], 15.0);
    assert_eq!(m[0] + m[4] + m[8], 15.0);
}

#[test]
fn cellfun_and_arrayfun() {
    let it = run("a = cellfun(@numel, {'ab', 'cde', ''}); b = arrayfun(@(x) x^2, 1:3); c = cellfun(@(s) [s '!'], {'hi', 'yo'}, 'UniformOutput', false); d = cellfun('isempty', {1, [], 'x'});");
    assert_eq!(num(&it, "a"), vec![2.0, 3.0, 0.0]);
    assert_eq!(num(&it, "b"), vec![1.0, 4.0, 9.0]);
    assert!(matches!(it.get_var("c").unwrap(), Value::Cell(_)));
    assert_eq!(num(&it, "d"), vec![0.0, 1.0, 0.0]);
}

// ---------------- display ----------------

#[test]
fn display_formats() {
    let o = out("x = 5\ny = [1 2; 3 4]\nz = 1.5\ns = 'txt'\nc = {1, 'a'}\nb = true");
    assert!(o.contains("x = 5\n"), "{}", o);
    assert!(o.contains("y =\n\n     1     2\n     3     4\n"), "{}", o);
    assert!(o.contains("z = 1.5000"), "{}", o);
    assert!(o.contains("s =\n\n    'txt'"), "{}", o);
    assert!(o.contains("1×2 cell array"), "{}", o);
    assert!(o.contains("b = 1"), "{}", o);
}

#[test]
fn printf_recycles_format() {
    let o = out("fprintf('%d-%d\\n', [1 2 3 4]);");
    assert_eq!(o, "1-2\n3-4\n");
    let o = out("fprintf('%s=%g; ', 'a', 1.5);");
    assert_eq!(o, "a=1.5; ");
}

#[test]
fn ans_and_semicolon_suppression() {
    let o = out("3 + 4;\n");
    assert_eq!(o, "");
    let it = run("3 + 4;");
    assert_eq!(num(&it, "ans"), vec![7.0]);
}

// ---------------- plotting model ----------------

#[test]
fn plotting_builds_figure_model() {
    let mut it = run("x = 0:0.1:1; figure; subplot(2,1,1); plot(x, x.^2, 'r--', 'LineWidth', 2); hold on; plot(x, x); title('T'); subplot(2,1,2); bar([1 2 3]);");
    let (figs, _) = it.graphics.take_updates();
    assert_eq!(figs.len(), 1);
    let f = &figs[0];
    assert_eq!(f.axes.len(), 2);
    assert_eq!(f.axes[0].series.len(), 2);
    assert_eq!(f.axes[0].title.as_deref(), Some("T"));
}

#[test]
fn subplot_replaces_overlapping_axes() {
    let mut it = run("plot(1:3); subplot(2,1,2); bar([1 2]);");
    let (figs, _) = it.graphics.take_updates();
    assert_eq!(figs[0].axes.len(), 1);
}

// ---------------- statistics ----------------

#[test]
fn distribution_quantiles_match_reference_tables() {
    let it = run("a = norminv(0.975); b = tinv(0.975, 24); c = chi2inv(0.025, 24); d = chi2inv(0.975, 24); e = finv(0.95, 24, 8); f = normcdf(1.96); g = tcdf(2, 10); h = norminv([0.5 0.95], 10, 2);");
    // Values from standard statistical tables.
    assert!(
        close(num(&it, "a")[0], 1.959963984540054),
        "{:?}",
        num(&it, "a")
    );
    assert!(
        (num(&it, "b")[0] - 2.063898561628021).abs() < 1e-9,
        "{:?}",
        num(&it, "b")
    );
    assert!(
        (num(&it, "c")[0] - 12.401150217444).abs() < 1e-8,
        "{:?}",
        num(&it, "c")
    );
    assert!(
        (num(&it, "d")[0] - 39.364077026603).abs() < 1e-8,
        "{:?}",
        num(&it, "d")
    );
    assert!(
        (num(&it, "e")[0] - 3.1152).abs() < 1e-3,
        "{:?}",
        num(&it, "e")
    );
    assert!((num(&it, "f")[0] - 0.9750021048517795).abs() < 1e-12);
    assert!(
        (num(&it, "g")[0] - 0.9633062).abs() < 1e-6,
        "{:?}",
        num(&it, "g")
    );
    let h = num(&it, "h");
    assert!(
        close(h[0], 10.0) && (h[1] - 13.289707253902945).abs() < 1e-9,
        "{:?}",
        h
    );
}

// ---------------- files, models and tables ----------------

fn temp_dir(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("beany_{}_{}", tag, std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn mat_files_round_trip() {
    let dir = temp_dir("mat");
    let mut it = Interp::new();
    it.cwd = dir.clone();
    it.eval_string("a = magic(3); s.name = 'x'; s.v = [1 2]; c = {1, 'two'}; b = true(2); k = int16(-7); save('d.mat'); clear all; load('d.mat'); w = load('d.mat');").unwrap();
    let _ = std::fs::remove_dir_all(dir);
    assert_eq!(
        num(&it, "a"),
        vec![8.0, 3.0, 4.0, 1.0, 5.0, 9.0, 6.0, 7.0, 2.0]
    );
    assert!(matches!(it.get_var("s"), Some(Value::Struct(_))));
    assert!(matches!(it.get_var("c"), Some(Value::Cell(_))));
    assert!(matches!(
        it.get_var("b"),
        Some(Value::Num(_, NumClass::Logical))
    ));
    assert_eq!(num(&it, "k"), vec![-7.0]);
    assert!(matches!(it.get_var("w"), Some(Value::Struct(_))));
}

#[test]
fn transfer_functions() {
    let it = run("F = tf(0.5, [1 -0.5], 0.1); y = step(F, 0:0.1:0.3); g = dcgain(F); p = pole(F); G = tf([1 0], [1 -1.2 0.35], 1); q = pole(G); C = tf(1, [1 1]); yc = step(C, [0 1 5]);");
    let y = num(&it, "y");
    assert!(
        close(y[0], 0.0) && close(y[1], 0.5) && close(y[2], 0.75) && close(y[3], 0.875),
        "{:?}",
        y
    );
    assert!(close(num(&it, "g")[0], 1.0));
    assert!(close(num(&it, "p")[0], 0.5));
    let q = num(&it, "q");
    assert!(close(q[0], 0.7) && close(q[1], 0.5), "{:?}", q);
    // First-order lag 1/(s+1): y(t) = 1 - e^-t.
    let yc = num(&it, "yc");
    assert!(
        yc[0].abs() < 1e-9
            && (yc[1] - (1.0 - (-1.0f64).exp())).abs() < 1e-3
            && (yc[2] - (1.0 - (-5.0f64).exp())).abs() < 1e-3,
        "{:?}",
        yc
    );
}

#[test]
fn tables_basics() {
    let dir = temp_dir("tbl");
    let mut it = Interp::new();
    it.cwd = dir.clone();
    it.eval_string("x = [1; 2]; y = [3; 4]; T = table(x, y, 'RowNames', {'r1'; 'r2'}); n = T.Properties.VariableNames; T{:,:} = T{:,:} * 10; m = T{:,:}; c = table2cell(T); yy = T.y; writetable(T, 'out.csv', 'WriteRowNames', true); writetable(T, 'out.xlsx');").unwrap();
    let csv = std::fs::read_to_string(dir.join("out.csv")).unwrap();
    let xlsx = dir.join("out.xlsx").is_file();
    let _ = std::fs::remove_dir_all(dir);
    let names = it.get_var("n").unwrap().clone();
    assert!(
        matches!(names, Value::Cell(ref c) if c.data[0].text().unwrap() == "x" && c.data[1].text().unwrap() == "y")
    );
    assert_eq!(num(&it, "m"), vec![10.0, 20.0, 30.0, 40.0]);
    assert_eq!(dims(&it, "c"), (2, 2));
    assert_eq!(num(&it, "yy"), vec![30.0, 40.0]);
    assert_eq!(csv, "Row,x,y\nr1,10,30\nr2,20,40\n");
    assert!(xlsx);
}

#[test]
fn tiled_layout_and_uitable() {
    let mut it = run("f = figure('Position', [0 0 800 600]); tl = tiledlayout(f, 2, 2); tl.OuterPosition = [0 0.3 1 0.7]; nexttile(tl); plot(1:3); nexttile(tl); bar([1 2]); uitable(f, 'Data', {1, 'a'}, 'Units', 'normalized', 'Position', [0 0 1 0.25]);");
    let (figs, _) = it.graphics.take_updates();
    let f = &figs[0];
    assert_eq!(f.axes.len(), 2);
    assert_eq!(f.axes[1].cells, vec![2]);
    assert_eq!(f.area, Some([0.0, 0.3, 1.0, 0.7]));
    assert_eq!(f.uitables.len(), 1);
    assert_eq!(f.size, Some((800.0, 600.0)));
}

// ---------------- paths ----------------

#[test]
fn folder_paths_keep_drive_form() {
    use beany_core::interp::clean_path;
    use std::path::Path;
    let base = Path::new(r"C:\work\proj");
    assert_eq!(
        clean_path(base, r"Z:\Skola\Cvicenia"),
        Path::new(r"Z:\Skola\Cvicenia")
    );
    assert_eq!(clean_path(base, r"..\other"), Path::new(r"C:\work\other"));
    assert_eq!(
        clean_path(base, r".\sub\.\x"),
        Path::new(r"C:\work\proj\sub\x")
    );
    assert_eq!(
        clean_path(base, r"\\?\UNC\server\share\dir"),
        Path::new(r"\\server\share\dir")
    );
    assert_eq!(clean_path(base, r"\\?\D:\data"), Path::new(r"D:\data"));
    assert_eq!(clean_path(Path::new(r"Z:\"), ".."), Path::new(r"Z:\"));
}

// ---------------- performance smoke test ----------------

#[test]
fn loop_performance_is_reasonable() {
    let t = std::time::Instant::now();
    let it = run("s = 0; for k = 1:200000, s = s + k; end");
    assert_eq!(num(&it, "s"), vec![200000.0 * 200001.0 / 2.0]);
    // Generous bound so debug builds pass; release is much faster.
    assert!(t.elapsed().as_secs_f64() < 20.0);
}

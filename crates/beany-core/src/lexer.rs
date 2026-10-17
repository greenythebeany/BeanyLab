//! Tokenizer for `.m` source.
//!
//! Handles the context-sensitive parts of the language here so the parser stays simple:
//! quote-vs-transpose, newlines as row separators inside brackets, `...` continuations,
//! block comments and command syntax (`hold on`, `format long`).

use crate::error::MError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kw {
    If,
    Elseif,
    Else,
    End,
    For,
    Parfor,
    While,
    Switch,
    Case,
    Otherwise,
    Try,
    Catch,
    Function,
    Return,
    Break,
    Continue,
    Global,
    Persistent,
}

fn keyword(s: &str) -> Option<Kw> {
    Some(match s {
        "if" => Kw::If,
        "elseif" => Kw::Elseif,
        "else" => Kw::Else,
        "end" => Kw::End,
        "for" => Kw::For,
        "parfor" => Kw::Parfor,
        "while" => Kw::While,
        "switch" => Kw::Switch,
        "case" => Kw::Case,
        "otherwise" => Kw::Otherwise,
        "try" => Kw::Try,
        "catch" => Kw::Catch,
        "function" => Kw::Function,
        "return" => Kw::Return,
        "break" => Kw::Break,
        "continue" => Kw::Continue,
        "global" => Kw::Global,
        "persistent" => Kw::Persistent,
        _ => return None,
    })
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    Ident(String),
    Kw(Kw),
    /// Single-quoted char array literal.
    Char(String),
    /// Double-quoted string literal.
    Str(String),
    Op(&'static str),
    /// Command syntax: name, arguments, raw source text of the whole statement.
    Command(String, Vec<String>, String),
    Newline,
    Semi,
    Comma,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub tok: Tok,
    pub line: usize,
    /// Char offsets into the source.
    pub start: usize,
    pub end: usize,
    pub space_before: bool,
}

const OPS3: [&str; 1] = ["..."];
const OPS2: [&str; 12] = [
    ".*", "./", ".\\", ".^", ".'", "==", "~=", "!=", "<=", ">=", "&&", "||",
];
const OPS1: [&str; 18] = [
    "+", "-", "*", "/", "\\", "^", "<", ">", "&", "|", "~", "=", ":", "(", ")", ".", "@", "!",
];

/// Valid variable name: letter first, then letters, digits or underscores, not a keyword.
pub fn is_identifier(s: &str) -> bool {
    let mut cs = s.chars();
    cs.next().is_some_and(|c| c.is_ascii_alphabetic())
        && cs.all(|c| c.is_ascii_alphanumeric() || c == '_')
        && keyword(s).is_none()
}

pub fn tokenize(src: &[char]) -> Result<Vec<Token>, MError> {
    Lexer {
        s: src,
        i: 0,
        line: 1,
        toks: Vec::new(),
        brackets: Vec::new(),
        stmt_start: true,
    }
    .run()
}

struct Lexer<'a> {
    s: &'a [char],
    i: usize,
    line: usize,
    toks: Vec<Token>,
    brackets: Vec<char>,
    stmt_start: bool,
}

impl<'a> Lexer<'a> {
    fn peek(&self, k: usize) -> char {
        *self.s.get(self.i + k).unwrap_or(&'\0')
    }

    fn in_matrix(&self) -> bool {
        matches!(self.brackets.last(), Some('[') | Some('{'))
    }

    fn push(&mut self, tok: Tok, start: usize, space: bool) {
        let at_top = self.brackets.is_empty();
        self.stmt_start = at_top && matches!(tok, Tok::Newline | Tok::Semi | Tok::Comma);
        self.toks.push(Token {
            tok,
            line: self.line,
            start,
            end: self.i,
            space_before: space,
        });
    }

    fn prev_ends_value(&self) -> bool {
        match self.toks.last().map(|t| &t.tok) {
            Some(Tok::Num(_)) | Some(Tok::Ident(_)) | Some(Tok::Char(_)) | Some(Tok::Str(_)) => {
                true
            }
            Some(Tok::Op(o)) => matches!(*o, ")" | "]" | "}" | "'" | ".'"),
            Some(Tok::Kw(Kw::End)) => !self.brackets.is_empty(),
            _ => false,
        }
    }

    fn err(&self, msg: impl Into<String>) -> MError {
        MError::syntax(format!("{} (line {})", msg.into(), self.line))
    }

    /// Line starts with only whitespace before `i`.
    fn at_line_start(&self, i: usize) -> bool {
        let mut k = i;
        while k > 0 {
            let c = self.s[k - 1];
            if c == '\n' {
                return true;
            }
            if c != ' ' && c != '\t' && c != '\r' {
                return false;
            }
            k -= 1;
        }
        true
    }

    fn rest_of_line_blank(&self, from: usize) -> bool {
        let mut k = from;
        while k < self.s.len() && self.s[k] != '\n' {
            if !self.s[k].is_whitespace() {
                return false;
            }
            k += 1;
        }
        true
    }

    fn run(mut self) -> Result<Vec<Token>, MError> {
        let mut space = false;
        while self.i < self.s.len() {
            let c = self.peek(0);
            let start = self.i;
            if c == ' ' || c == '\t' || c == '\r' {
                self.i += 1;
                space = true;
                continue;
            }
            // Block comment: %{ alone on a line ... %} alone on a line.
            if c == '%'
                && self.peek(1) == '{'
                && self.at_line_start(self.i)
                && self.rest_of_line_blank(self.i + 2)
            {
                let mut depth = 0usize;
                loop {
                    if self.i >= self.s.len() {
                        break;
                    }
                    if self.at_line_start(self.i)
                        && self.peek(0) == '%'
                        && self.peek(1) == '{'
                        && self.rest_of_line_blank(self.i + 2)
                    {
                        depth += 1;
                        self.i += 2;
                    } else if self.at_line_start(self.i)
                        && self.peek(0) == '%'
                        && self.peek(1) == '}'
                        && self.rest_of_line_blank(self.i + 2)
                    {
                        depth -= 1;
                        self.i += 2;
                        if depth == 0 {
                            break;
                        }
                    } else {
                        if self.peek(0) == '\n' {
                            self.line += 1;
                        }
                        self.i += 1;
                    }
                }
                continue;
            }
            if c == '%' || (c == '#' && self.toks.is_empty()) {
                while self.i < self.s.len() && self.peek(0) != '\n' {
                    self.i += 1;
                }
                continue;
            }
            if c == '.' && self.peek(1) == '.' && self.peek(2) == '.' {
                // Continuation: ignore the rest of the line including the newline.
                while self.i < self.s.len() && self.peek(0) != '\n' {
                    self.i += 1;
                }
                if self.i < self.s.len() {
                    self.i += 1;
                    self.line += 1;
                }
                space = true;
                continue;
            }
            if c == '\n' {
                self.i += 1;
                if self.in_matrix() {
                    let last_is_sep = matches!(
                        self.toks.last().map(|t| &t.tok),
                        Some(Tok::Semi) | Some(Tok::Op("[")) | Some(Tok::Op("{"))
                    );
                    if !last_is_sep {
                        self.push(Tok::Semi, start, space);
                    }
                } else if self.brackets.is_empty() {
                    self.push(Tok::Newline, start, space);
                }
                self.line += 1;
                space = false;
                continue;
            }
            if c.is_ascii_digit() || (c == '.' && self.peek(1).is_ascii_digit()) {
                let n = self.number()?;
                self.push(Tok::Num(n), start, space);
                space = false;
                continue;
            }
            if c.is_alphabetic() || c == '_' {
                let mut j = self.i;
                while j < self.s.len() && (self.s[j].is_alphanumeric() || self.s[j] == '_') {
                    j += 1;
                }
                let word: String = self.s[self.i..j].iter().collect();
                if let Some(kw) = keyword(&word) {
                    self.i = j;
                    self.push(Tok::Kw(kw), start, space);
                } else if self.stmt_start && self.brackets.is_empty() && self.is_command_syntax(j) {
                    self.i = j;
                    let (args, raw_end) = self.command_args()?;
                    let raw: String = self.s[start..raw_end].iter().collect();
                    self.push(Tok::Command(word, args, raw), start, space);
                } else {
                    self.i = j;
                    self.push(Tok::Ident(word), start, space);
                }
                space = false;
                continue;
            }
            if c == '"' {
                let text = self.quoted('"')?;
                self.push(Tok::Str(text), start, space);
                space = false;
                continue;
            }
            if c == '\'' {
                let transpose = self.prev_ends_value() && !(space && self.in_matrix());
                if transpose {
                    self.i += 1;
                    self.push(Tok::Op("'"), start, space);
                } else {
                    let text = self.quoted('\'')?;
                    self.push(Tok::Char(text), start, space);
                }
                space = false;
                continue;
            }
            if c == ';' {
                self.i += 1;
                self.push(Tok::Semi, start, space);
                space = false;
                continue;
            }
            if c == ',' {
                self.i += 1;
                self.push(Tok::Comma, start, space);
                space = false;
                continue;
            }
            if c == '[' || c == '{' || c == '(' {
                self.i += 1;
                self.brackets.push(c);
                let op = match c {
                    '[' => "[",
                    '{' => "{",
                    _ => "(",
                };
                self.push(Tok::Op(op), start, space);
                space = false;
                continue;
            }
            if c == ']' || c == '}' || c == ')' {
                // Drop a trailing row separator produced by a newline right before the bracket.
                if c != ')'
                    && matches!(self.toks.last().map(|t| &t.tok), Some(Tok::Semi))
                    && self.s[self.toks.last().unwrap().start] == '\n'
                {
                    self.toks.pop();
                }
                self.i += 1;
                self.brackets.pop();
                let op = match c {
                    ']' => "]",
                    '}' => "}",
                    _ => ")",
                };
                self.push(Tok::Op(op), start, space);
                space = false;
                continue;
            }
            let mut matched = None;
            for op in OPS3.iter().chain(OPS2.iter()).chain(OPS1.iter()) {
                let chars: Vec<char> = op.chars().collect();
                if (0..chars.len()).all(|k| self.peek(k) == chars[k]) {
                    matched = Some(*op);
                    break;
                }
            }
            match matched {
                Some(op) => {
                    self.i += op.chars().count();
                    let op = match op {
                        "!" => "~",
                        "!=" => "~=",
                        o => o,
                    };
                    self.push(Tok::Op(op), start, space);
                    space = false;
                }
                None => return Err(self.err(format!("Invalid character '{}'", c))),
            }
        }
        let n = self.i;
        self.push(Tok::Eof, n, space);
        Ok(self.toks)
    }

    fn number(&mut self) -> Result<f64, MError> {
        let st = self.i;
        while self.peek(0).is_ascii_digit() {
            self.i += 1;
        }
        if self.peek(0) == '.'
            && !matches!(self.peek(1), '*' | '/' | '\\' | '^' | '\'')
            && !(self.peek(1) == '.' && self.peek(2) == '.')
        {
            self.i += 1;
            while self.peek(0).is_ascii_digit() {
                self.i += 1;
            }
        }
        if matches!(self.peek(0), 'e' | 'E' | 'd' | 'D') {
            let save = self.i;
            self.i += 1;
            if matches!(self.peek(0), '+' | '-') {
                self.i += 1;
            }
            if self.peek(0).is_ascii_digit() {
                while self.peek(0).is_ascii_digit() {
                    self.i += 1;
                }
            } else {
                self.i = save;
            }
        }
        let text: String = self.s[st..self.i]
            .iter()
            .map(|c| if *c == 'd' || *c == 'D' { 'e' } else { *c })
            .collect();
        if matches!(self.peek(0), 'i' | 'j')
            && !(self.peek(1).is_alphanumeric() || self.peek(1) == '_')
        {
            return Err(self.err("Complex numbers are not supported yet"));
        }
        text.parse::<f64>()
            .map_err(|_| self.err(format!("Invalid number '{}'", text)))
    }

    fn quoted(&mut self, q: char) -> Result<String, MError> {
        self.i += 1;
        let mut out = String::new();
        loop {
            let c = self.peek(0);
            if self.i >= self.s.len() || c == '\n' {
                return Err(self.err("String is not terminated properly"));
            }
            if c == q {
                if self.peek(1) == q {
                    out.push(q);
                    self.i += 2;
                    continue;
                }
                self.i += 1;
                return Ok(out);
            }
            out.push(c);
            self.i += 1;
        }
    }

    /// Decide whether `word` at statement start (ending at `j`) begins command syntax.
    fn is_command_syntax(&self, j: usize) -> bool {
        let mut k = j;
        let mut gap = false;
        while k < self.s.len() && (self.s[k] == ' ' || self.s[k] == '\t') {
            k += 1;
            gap = true;
        }
        if !gap || k >= self.s.len() {
            return false;
        }
        let c = self.s[k];
        let next = *self.s.get(k + 1).unwrap_or(&'\0');
        if matches!(c, '\n' | '\r' | ';' | ',' | '%' | '=' | '(') {
            return false;
        }
        if c.is_alphanumeric() || c == '_' {
            // `a b = ...` style is not command syntax; check the word isn't followed by `=` (assignment).
            let mut m = k;
            while m < self.s.len() && (self.s[m].is_alphanumeric() || self.s[m] == '_') {
                m += 1;
            }
            while m < self.s.len() && self.s[m] == ' ' {
                m += 1;
            }
            let after = *self.s.get(m).unwrap_or(&'\0');
            let after2 = *self.s.get(m + 1).unwrap_or(&'\0');
            if after == '=' && after2 != '=' {
                return false;
            }
            // `x 1` where word is a pure number followed by operator: keep command syntax anyway.
            return true;
        }
        if c == '-' && next.is_alphabetic() {
            return true;
        }
        if c == '\'' {
            return true;
        }
        false
    }

    fn command_args(&mut self) -> Result<(Vec<String>, usize), MError> {
        let mut args = Vec::new();
        let mut raw_end;
        loop {
            while self.peek(0) == ' ' || self.peek(0) == '\t' {
                self.i += 1;
            }
            raw_end = self.i;
            let c = self.peek(0);
            if self.i >= self.s.len() || matches!(c, '\n' | '\r' | ';' | ',' | '%') {
                break;
            }
            let mut arg = String::new();
            while self.i < self.s.len() {
                let c = self.peek(0);
                if matches!(c, ' ' | '\t' | '\n' | '\r' | ';' | ',') {
                    break;
                }
                if c == '\'' {
                    arg.push_str(&self.quoted('\'')?);
                    continue;
                }
                arg.push(c);
                self.i += 1;
            }
            args.push(arg);
        }
        Ok((args, raw_end))
    }
}

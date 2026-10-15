//! Recursive-descent parser producing [`crate::ast`] nodes.

use std::sync::Arc;

use crate::ast::*;
use crate::error::{MError, MResult};
use crate::lexer::{tokenize, Kw, Tok, Token};

#[derive(Clone, Copy, PartialEq)]
enum Ctx {
    /// Inside `[]` or `{}` literal: whitespace separates elements.
    Matrix,
    /// Inside parentheses or index arguments.
    Paren,
}

pub struct Parser<'a> {
    toks: Vec<Token>,
    pos: usize,
    src: &'a [char],
    ctx: Vec<Ctx>,
    index_depth: usize,
}

pub fn parse_file(path: &str, source: &str) -> MResult<FileUnit> {
    let chars: Vec<char> = source.chars().collect();
    let toks = tokenize(&chars).map_err(|e| with_file(e, path))?;
    let mut p = Parser {
        toks,
        pos: 0,
        src: &chars,
        ctx: Vec::new(),
        index_depth: 0,
    };
    p.file(path).map_err(|e| with_file(e, path))
}

/// Parse a chunk typed at the command window: a script that may also define functions.
pub fn parse_snippet(source: &str) -> MResult<FileUnit> {
    parse_file("", source)
}

/// Parse a single expression (used by `eval` fallbacks and anonymous-function text).
pub fn parse_expression(source: &str) -> MResult<Expr> {
    let chars: Vec<char> = source.chars().collect();
    let toks = tokenize(&chars)?;
    let mut p = Parser {
        toks,
        pos: 0,
        src: &chars,
        ctx: Vec::new(),
        index_depth: 0,
    };
    let e = p.expr()?;
    p.skip_newlines();
    if !matches!(p.tok(), Tok::Eof) {
        return Err(p.err("Unexpected text after expression"));
    }
    Ok(e)
}

fn with_file(mut e: MError, path: &str) -> MError {
    if !path.is_empty() && !e.message.contains(path) {
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
        e.message = format!("{}: {}", name, e.message);
    }
    e
}

fn binop_of(op: &str) -> Option<(BinOp, u8)> {
    // Higher number binds tighter.
    Some(match op {
        "||" => (BinOp::OrOr, 1),
        "&&" => (BinOp::AndAnd, 2),
        "|" => (BinOp::Or, 3),
        "&" => (BinOp::And, 4),
        "==" => (BinOp::Eq, 5),
        "~=" => (BinOp::Ne, 5),
        "<" => (BinOp::Lt, 5),
        "<=" => (BinOp::Le, 5),
        ">" => (BinOp::Gt, 5),
        ">=" => (BinOp::Ge, 5),
        "+" => (BinOp::Add, 7),
        "-" => (BinOp::Sub, 7),
        "*" => (BinOp::MatMul, 8),
        "/" => (BinOp::MatDiv, 8),
        "\\" => (BinOp::MatLDiv, 8),
        ".*" => (BinOp::ElMul, 8),
        "./" => (BinOp::ElDiv, 8),
        ".\\" => (BinOp::ElLDiv, 8),
        _ => return None,
    })
}

const RANGE_PREC: u8 = 6;

impl<'a> Parser<'a> {
    fn tok(&self) -> &Tok {
        &self.toks[self.pos].tok
    }
    fn tok_at(&self, k: usize) -> &Tok {
        &self.toks[(self.pos + k).min(self.toks.len() - 1)].tok
    }
    fn cur(&self) -> &Token {
        &self.toks[self.pos]
    }
    fn line(&self) -> usize {
        self.toks[self.pos].line
    }
    fn bump(&mut self) -> Token {
        let t = self.toks[self.pos].clone();
        if self.pos < self.toks.len() - 1 {
            self.pos += 1;
        }
        t
    }
    fn is_op(&self, op: &str) -> bool {
        matches!(self.tok(), Tok::Op(o) if *o == op)
    }
    fn is_kw(&self, kw: Kw) -> bool {
        matches!(self.tok(), Tok::Kw(k) if *k == kw)
    }
    fn err(&self, msg: &str) -> MError {
        let found = match self.tok() {
            Tok::Num(n) => format!("{}", n),
            Tok::Ident(s) => s.clone(),
            Tok::Kw(k) => format!("{:?}", k).to_lowercase(),
            Tok::Char(s) => format!("'{}'", s),
            Tok::Str(s) => format!("\"{}\"", s),
            Tok::Op(o) => o.to_string(),
            Tok::Command(n, _, _) => n.clone(),
            Tok::Newline => "end of line".into(),
            Tok::Semi => ";".into(),
            Tok::Comma => ",".into(),
            Tok::Eof => "end of input".into(),
        };
        MError::syntax(format!("{} near '{}' (line {})", msg, found, self.line()))
    }
    fn expect_op(&mut self, op: &str) -> MResult<()> {
        if self.is_op(op) {
            self.bump();
            Ok(())
        } else {
            Err(self.err(&format!("Expected '{}'", op)))
        }
    }
    fn expect_end(&mut self, what: &str) -> MResult<()> {
        if self.is_kw(Kw::End) {
            self.bump();
            Ok(())
        } else {
            Err(self.err(&format!("Missing 'end' for '{}'", what)))
        }
    }
    fn skip_newlines(&mut self) {
        while matches!(self.tok(), Tok::Newline | Tok::Semi | Tok::Comma) {
            self.bump();
        }
    }
    fn in_matrix(&self) -> bool {
        self.ctx.last() == Some(&Ctx::Matrix)
    }

    // ---------- files and functions ----------

    fn file(&mut self, path: &str) -> MResult<FileUnit> {
        self.skip_newlines();
        let mut unit = FileUnit {
            path: path.to_string(),
            script: None,
            functions: Vec::new(),
        };
        if !self.is_kw(Kw::Function) {
            let body = self.block(&[Kw::Function])?;
            unit.script = Some(body);
        }
        loop {
            self.skip_newlines();
            if matches!(self.tok(), Tok::Eof) {
                break;
            }
            if self.is_kw(Kw::Function) {
                let f = self.function()?;
                unit.functions.push(Arc::new(f));
            } else if self.is_kw(Kw::End) {
                return Err(self.err("Unexpected 'end'"));
            } else {
                return Err(self.err("Statements are not allowed after function definitions"));
            }
        }
        Ok(unit)
    }

    fn function(&mut self) -> MResult<Function> {
        let line = self.line();
        self.bump(); // function
        let mut outputs = Vec::new();
        let name;
        if self.is_op("[") {
            self.bump();
            while !self.is_op("]") {
                match self.bump().tok {
                    Tok::Ident(s) => outputs.push(s),
                    Tok::Comma => {}
                    _ => return Err(self.err("Invalid function output list")),
                }
            }
            self.bump();
            self.expect_op("=")?;
            name = self.func_name()?;
        } else {
            let first = self.func_name()?;
            if self.is_op("=") {
                self.bump();
                outputs.push(first);
                name = self.func_name()?;
            } else {
                name = first;
            }
        }
        let mut params = Vec::new();
        if self.is_op("(") {
            self.bump();
            while !self.is_op(")") {
                match self.bump().tok {
                    Tok::Ident(s) => params.push(s),
                    Tok::Op("~") => params.push("~".into()),
                    Tok::Comma => {}
                    _ => return Err(self.err("Invalid function parameter list")),
                }
            }
            self.bump();
        }
        let body = self.block(&[Kw::End, Kw::Function])?;
        if self.is_kw(Kw::End) {
            self.bump();
        }
        Ok(Function {
            name,
            params,
            outputs,
            body,
            line,
            help: String::new(),
        })
    }

    fn func_name(&mut self) -> MResult<String> {
        match self.bump().tok {
            Tok::Ident(s) => {
                let mut s = s;
                while self.is_op(".") {
                    self.bump();
                    if let Tok::Ident(p) = self.bump().tok {
                        s = format!("{}.{}", s, p);
                    }
                }
                Ok(s)
            }
            Tok::Command(n, _, _) => Ok(n),
            _ => Err(self.err("Expected function name")),
        }
    }

    // ---------- statements ----------

    fn block(&mut self, stop: &[Kw]) -> MResult<Vec<Stmt>> {
        let mut out = Vec::new();
        loop {
            self.skip_newlines();
            match self.tok() {
                Tok::Eof => return Ok(out),
                Tok::Kw(k) if stop.contains(k) => return Ok(out),
                Tok::Kw(Kw::End | Kw::Else | Kw::Elseif | Kw::Case | Kw::Otherwise | Kw::Catch) => {
                    return Err(self.err("Unexpected keyword"));
                }
                Tok::Kw(Kw::Function) => {
                    return Err(self.err("Function definitions are not allowed here"))
                }
                _ => out.push(self.statement()?),
            }
        }
    }

    /// Consume a statement terminator; returns whether output should be printed.
    fn terminator(&mut self) -> MResult<bool> {
        match self.tok() {
            Tok::Semi => {
                self.bump();
                if matches!(self.tok(), Tok::Comma) {
                    self.bump();
                }
                Ok(false)
            }
            Tok::Comma | Tok::Newline => {
                self.bump();
                Ok(true)
            }
            Tok::Eof => Ok(true),
            Tok::Kw(Kw::End | Kw::Else | Kw::Elseif | Kw::Case | Kw::Otherwise | Kw::Catch) => {
                Ok(true)
            }
            _ => Err(self.err("Parse error")),
        }
    }

    fn statement(&mut self) -> MResult<Stmt> {
        let line = self.line();
        let kind = match self.tok().clone() {
            Tok::Kw(Kw::If) => self.if_stmt()?,
            Tok::Kw(Kw::For) | Tok::Kw(Kw::Parfor) => self.for_stmt()?,
            Tok::Kw(Kw::While) => {
                self.bump();
                let cond = self.expr()?;
                let body = self.block(&[Kw::End])?;
                self.expect_end("while")?;
                StmtKind::While { cond, body }
            }
            Tok::Kw(Kw::Switch) => self.switch_stmt()?,
            Tok::Kw(Kw::Try) => {
                self.bump();
                if matches!(self.tok(), Tok::Comma) {
                    self.bump();
                }
                let body = self.block(&[Kw::Catch, Kw::End])?;
                let mut ident = None;
                let mut catch_body = Vec::new();
                if self.is_kw(Kw::Catch) {
                    let catch_line = self.line();
                    self.bump();
                    if let Tok::Ident(name) = self.tok().clone() {
                        if self.line() == catch_line {
                            self.bump();
                            ident = Some(name);
                        }
                    } else if let Tok::Command(name, args, raw) = self.tok().clone() {
                        // `catch err disp(x)` is rare; treat first word as identifier and re-parse the rest.
                        let _ = (args, raw);
                        self.bump();
                        ident = Some(name);
                    }
                    catch_body = self.block(&[Kw::End])?;
                }
                self.expect_end("try")?;
                StmtKind::Try {
                    body,
                    ident,
                    catch_body,
                }
            }
            Tok::Kw(Kw::Break) => {
                self.bump();
                self.terminator()?;
                StmtKind::Break
            }
            Tok::Kw(Kw::Continue) => {
                self.bump();
                self.terminator()?;
                StmtKind::Continue
            }
            Tok::Kw(Kw::Return) => {
                self.bump();
                self.terminator()?;
                StmtKind::Return
            }
            Tok::Kw(Kw::Global) | Tok::Kw(Kw::Persistent) => {
                let is_global = self.is_kw(Kw::Global);
                self.bump();
                let mut names = Vec::new();
                while let Tok::Ident(n) = self.tok().clone() {
                    names.push(n);
                    self.bump();
                }
                self.terminator()?;
                if is_global {
                    StmtKind::Global(names)
                } else {
                    StmtKind::Persistent(names)
                }
            }
            Tok::Command(name, args, raw) => {
                self.bump();
                let print = self.terminator()?;
                StmtKind::Command {
                    name,
                    args,
                    raw,
                    print,
                }
            }
            Tok::Op("[") if self.is_multi_assign() => {
                let lhs = self.lvalue_list()?;
                self.expect_op("=")?;
                let rhs = self.expr()?;
                let print = self.terminator()?;
                StmtKind::Assign { lhs, rhs, print }
            }
            _ => {
                let e = self.expr()?;
                if self.is_op("=") {
                    self.bump();
                    let lhs = vec![to_lvalue(e).map_err(|m| self.err(&m))?];
                    let rhs = self.expr()?;
                    let print = self.terminator()?;
                    StmtKind::Assign { lhs, rhs, print }
                } else {
                    let print = self.terminator()?;
                    StmtKind::Expr { expr: e, print }
                }
            }
        };
        Ok(Stmt { kind, line })
    }

    fn if_stmt(&mut self) -> MResult<StmtKind> {
        self.bump();
        let mut clauses = Vec::new();
        let cond = self.expr()?;
        let body = self.block(&[Kw::Elseif, Kw::Else, Kw::End])?;
        clauses.push((cond, body));
        let mut else_body = None;
        loop {
            if self.is_kw(Kw::Elseif) {
                self.bump();
                let c = self.expr()?;
                let b = self.block(&[Kw::Elseif, Kw::Else, Kw::End])?;
                clauses.push((c, b));
            } else if self.is_kw(Kw::Else) {
                self.bump();
                else_body = Some(self.block(&[Kw::End])?);
            } else {
                break;
            }
        }
        self.expect_end("if")?;
        Ok(StmtKind::If { clauses, else_body })
    }

    fn for_stmt(&mut self) -> MResult<StmtKind> {
        self.bump();
        let paren = self.is_op("(");
        if paren {
            self.bump();
        }
        let var = match self.bump().tok {
            Tok::Ident(s) => s,
            _ => return Err(self.err("Expected loop variable")),
        };
        self.expect_op("=")?;
        if paren {
            self.ctx.push(Ctx::Paren);
        }
        let iter = self.expr()?;
        if paren {
            self.ctx.pop();
            if self.is_op(",") {
                self.bump();
                self.expr()?; // parfor worker count: ignored
            }
            self.expect_op(")")?;
        }
        let body = self.block(&[Kw::End])?;
        self.expect_end("for")?;
        Ok(StmtKind::For { var, iter, body })
    }

    fn switch_stmt(&mut self) -> MResult<StmtKind> {
        self.bump();
        let subject = self.expr()?;
        self.skip_newlines();
        let mut cases = Vec::new();
        let mut otherwise = None;
        loop {
            if self.is_kw(Kw::Case) {
                self.bump();
                let v = self.expr()?;
                let b = self.block(&[Kw::Case, Kw::Otherwise, Kw::End])?;
                cases.push((v, b));
            } else if self.is_kw(Kw::Otherwise) {
                self.bump();
                otherwise = Some(self.block(&[Kw::Case, Kw::Otherwise, Kw::End])?);
            } else {
                break;
            }
        }
        self.expect_end("switch")?;
        Ok(StmtKind::Switch {
            subject,
            cases,
            otherwise,
        })
    }

    fn is_multi_assign(&self) -> bool {
        let mut depth = 0i32;
        let mut k = self.pos;
        while k < self.toks.len() {
            match &self.toks[k].tok {
                Tok::Op("[") | Tok::Op("(") | Tok::Op("{") => depth += 1,
                Tok::Op("]") | Tok::Op(")") | Tok::Op("}") => {
                    depth -= 1;
                    if depth == 0 {
                        return matches!(self.toks.get(k + 1).map(|t| &t.tok), Some(Tok::Op("=")));
                    }
                }
                Tok::Newline | Tok::Eof => return false,
                _ => {}
            }
            k += 1;
        }
        false
    }

    fn lvalue_list(&mut self) -> MResult<Vec<LValue>> {
        self.expect_op("[")?;
        self.ctx.push(Ctx::Matrix);
        let mut out = Vec::new();
        while !self.is_op("]") {
            if matches!(self.tok(), Tok::Comma) {
                self.bump();
                continue;
            }
            if self.is_op("~") {
                self.bump();
                out.push(LValue::Ignore);
                continue;
            }
            let e = self.postfix()?;
            out.push(to_lvalue(e).map_err(|m| self.err(&m))?);
        }
        self.ctx.pop();
        self.bump();
        Ok(out)
    }

    // ---------- expressions ----------

    pub fn expr(&mut self) -> MResult<Expr> {
        self.binary(1)
    }

    /// Whitespace rule inside brackets: `[1 -2]` is two elements, `[1 - 2]` is one.
    fn matrix_element_break(&self) -> bool {
        if !self.in_matrix() {
            return false;
        }
        let t = self.cur();
        if !t.space_before {
            return false;
        }
        let next_spaced = self.toks.get(self.pos + 1).is_none_or(|n| n.space_before);
        matches!(t.tok, Tok::Op("+") | Tok::Op("-")) && !next_spaced
    }

    fn binary(&mut self, min_prec: u8) -> MResult<Expr> {
        let mut lhs = self.unary()?;
        while let Tok::Op(o) = self.tok() {
            let op = *o;
            if op == ":" {
                if RANGE_PREC < min_prec {
                    break;
                }
                self.bump();
                let mid = self.binary(RANGE_PREC + 1)?;
                if self.is_op(":") {
                    self.bump();
                    let stop = self.binary(RANGE_PREC + 1)?;
                    lhs = Expr::Range(Box::new(lhs), Some(Box::new(mid)), Box::new(stop));
                } else {
                    lhs = Expr::Range(Box::new(lhs), None, Box::new(mid));
                }
                continue;
            }
            let Some((bop, prec)) = binop_of(op) else {
                break;
            };
            if prec < min_prec {
                break;
            }
            if self.matrix_element_break() {
                break;
            }
            self.bump();
            let rhs = self.binary(prec + 1)?;
            lhs = Expr::Binary(bop, Box::new(lhs), Box::new(rhs));
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> MResult<Expr> {
        let op = match self.tok() {
            Tok::Op("-") => Some(UnOp::Neg),
            Tok::Op("+") => Some(UnOp::Plus),
            Tok::Op("~") => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let e = self.unary()?;
            return Ok(Expr::Unary(op, Box::new(e)));
        }
        self.power()
    }

    fn power(&mut self) -> MResult<Expr> {
        let mut base = self.postfix()?;
        loop {
            let op = if self.is_op("^") {
                BinOp::MatPow
            } else if self.is_op(".^") {
                BinOp::ElPow
            } else {
                break;
            };
            self.bump();
            let exp = self.power_operand()?;
            base = Expr::Binary(op, Box::new(base), Box::new(exp));
        }
        Ok(base)
    }

    fn power_operand(&mut self) -> MResult<Expr> {
        let op = match self.tok() {
            Tok::Op("-") => Some(UnOp::Neg),
            Tok::Op("+") => Some(UnOp::Plus),
            Tok::Op("~") => Some(UnOp::Not),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let e = self.power_operand()?;
            return Ok(Expr::Unary(op, Box::new(e)));
        }
        self.postfix()
    }

    fn postfix(&mut self) -> MResult<Expr> {
        let mut e = self.primary()?;
        loop {
            let spaced = self.cur().space_before && self.in_matrix();
            match self.tok() {
                Tok::Op("(") if !spaced => {
                    self.bump();
                    let args = self.args(")")?;
                    e = Expr::Index(Box::new(e), args);
                }
                Tok::Op("{") if !spaced => {
                    self.bump();
                    let args = self.args("}")?;
                    e = Expr::CellIndex(Box::new(e), args);
                }
                Tok::Op(".") if !spaced => {
                    self.bump();
                    match self.tok().clone() {
                        Tok::Ident(name) => {
                            self.bump();
                            e = Expr::Field(Box::new(e), name);
                        }
                        Tok::Kw(k) => {
                            self.bump();
                            e = Expr::Field(Box::new(e), format!("{:?}", k).to_lowercase());
                        }
                        Tok::Op("(") => {
                            self.bump();
                            self.ctx.push(Ctx::Paren);
                            let f = self.expr()?;
                            self.ctx.pop();
                            self.expect_op(")")?;
                            e = Expr::DynField(Box::new(e), Box::new(f));
                        }
                        _ => return Err(self.err("Expected field name")),
                    }
                }
                Tok::Op("'") | Tok::Op(".'") => {
                    self.bump();
                    e = Expr::Transpose(Box::new(e));
                }
                _ => break,
            }
        }
        Ok(e)
    }

    fn args(&mut self, close: &str) -> MResult<Vec<Expr>> {
        self.ctx.push(Ctx::Paren);
        self.index_depth += 1;
        let mut out = Vec::new();
        loop {
            while matches!(self.tok(), Tok::Newline) {
                self.bump();
            }
            if self.is_op(close) {
                break;
            }
            if self.is_op(":") && matches!(self.tok_at(1), Tok::Comma | Tok::Op(")") | Tok::Op("}"))
            {
                self.bump();
                out.push(Expr::Colon);
            } else {
                out.push(self.expr()?);
            }
            if matches!(self.tok(), Tok::Comma) {
                self.bump();
            } else if !self.is_op(close) {
                self.index_depth -= 1;
                self.ctx.pop();
                return Err(self.err(&format!("Expected ',' or '{}'", close)));
            }
        }
        self.bump();
        self.index_depth -= 1;
        self.ctx.pop();
        Ok(out)
    }

    fn primary(&mut self) -> MResult<Expr> {
        let t = self.bump();
        match t.tok {
            Tok::Num(n) => Ok(Expr::Num(n)),
            Tok::Char(s) => Ok(Expr::Char(s)),
            Tok::Str(s) => Ok(Expr::Str(s)),
            Tok::Ident(s) => Ok(Expr::Ident(s)),
            Tok::Command(name, _, _) => Err(MError::syntax(format!(
                "Unexpected command syntax '{}' (line {})",
                name, t.line
            ))),
            Tok::Kw(Kw::End) if self.index_depth > 0 => Ok(Expr::End),
            Tok::Op("(") => {
                self.ctx.push(Ctx::Paren);
                let e = self.expr()?;
                self.ctx.pop();
                self.expect_op(")")?;
                Ok(Expr::Paren(Box::new(e)))
            }
            Tok::Op("[") => Ok(Expr::Matrix(self.matrix_rows("]")?)),
            Tok::Op("{") => Ok(Expr::Cell(self.matrix_rows("}")?)),
            Tok::Op("@") => {
                if self.is_op("(") {
                    self.bump();
                    let mut params = Vec::new();
                    while !self.is_op(")") {
                        match self.bump().tok {
                            Tok::Ident(s) => params.push(s),
                            Tok::Op("~") => params.push("~".into()),
                            Tok::Comma => {}
                            _ => return Err(self.err("Invalid anonymous function parameters")),
                        }
                    }
                    self.bump();
                    let body_start = self.cur().start;
                    let saved_ctx = std::mem::take(&mut self.ctx);
                    let saved_idx = self.index_depth;
                    self.index_depth = 0;
                    let body = self.expr();
                    self.ctx = saved_ctx;
                    self.index_depth = saved_idx;
                    let body = body?;
                    let body_end = self.toks[self.pos.saturating_sub(1)].end;
                    let body_text: String = self.src[body_start..body_end.max(body_start)]
                        .iter()
                        .collect();
                    let text = format!("@({}){}", params.join(","), body_text.trim());
                    Ok(Expr::AnonFn {
                        params,
                        body: Arc::new(body),
                        text,
                    })
                } else {
                    let mut name = match self.bump().tok {
                        Tok::Ident(s) => s,
                        _ => return Err(self.err("Invalid function handle")),
                    };
                    while self.is_op(".") && !self.cur().space_before {
                        self.bump();
                        if let Tok::Ident(p) = self.bump().tok {
                            name = format!("{}.{}", name, p);
                        }
                    }
                    Ok(Expr::FuncHandle(name))
                }
            }
            _ => {
                self.pos -= 1;
                Err(self.err("Invalid expression"))
            }
        }
    }

    fn matrix_rows(&mut self, close: &str) -> MResult<Vec<Vec<Expr>>> {
        self.ctx.push(Ctx::Matrix);
        let mut rows: Vec<Vec<Expr>> = Vec::new();
        let mut row: Vec<Expr> = Vec::new();
        loop {
            match self.tok() {
                Tok::Op(o) if *o == close => {
                    self.bump();
                    break;
                }
                Tok::Semi | Tok::Newline => {
                    self.bump();
                    if !row.is_empty() {
                        rows.push(std::mem::take(&mut row));
                    }
                }
                Tok::Comma => {
                    self.bump();
                }
                Tok::Eof => {
                    self.ctx.pop();
                    return Err(self.err(&format!("Missing '{}'", close)));
                }
                _ => {
                    let e = self.expr()?;
                    row.push(e);
                }
            }
        }
        if !row.is_empty() {
            rows.push(row);
        }
        self.ctx.pop();
        Ok(rows)
    }
}

fn to_lvalue(e: Expr) -> Result<LValue, String> {
    fn walk(e: Expr, path: &mut Vec<Access>) -> Result<String, String> {
        match e {
            Expr::Ident(n) => Ok(n),
            Expr::Index(b, a) => {
                let n = walk(*b, path)?;
                path.push(Access::Paren(a));
                Ok(n)
            }
            Expr::CellIndex(b, a) => {
                let n = walk(*b, path)?;
                path.push(Access::Brace(a));
                Ok(n)
            }
            Expr::Field(b, f) => {
                let n = walk(*b, path)?;
                path.push(Access::Field(f));
                Ok(n)
            }
            Expr::DynField(b, f) => {
                let n = walk(*b, path)?;
                path.push(Access::DynField(*f));
                Ok(n)
            }
            _ => Err("Invalid assignment target".into()),
        }
    }
    let mut path = Vec::new();
    let name = walk(e, &mut path)?;
    Ok(LValue::Var { name, path })
}

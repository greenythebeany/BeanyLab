use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    MatMul,
    ElMul,
    MatDiv,
    ElDiv,
    MatLDiv,
    ElLDiv,
    MatPow,
    ElPow,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    AndAnd,
    OrOr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnOp {
    Neg,
    Plus,
    Not,
}

#[derive(Debug, Clone)]
pub enum Expr {
    Num(f64),
    Char(String),
    Str(String),
    Ident(String),
    /// Bare `:` inside an index.
    Colon,
    /// `end` inside an index.
    End,
    Unary(UnOp, Box<Expr>),
    Binary(BinOp, Box<Expr>, Box<Expr>),
    /// `'` (conjugate transpose); `.'` is identical while complex numbers are unsupported.
    Transpose(Box<Expr>),
    Range(Box<Expr>, Option<Box<Expr>>, Box<Expr>),
    Index(Box<Expr>, Vec<Expr>),
    CellIndex(Box<Expr>, Vec<Expr>),
    Field(Box<Expr>, String),
    DynField(Box<Expr>, Box<Expr>),
    Matrix(Vec<Vec<Expr>>),
    Cell(Vec<Vec<Expr>>),
    AnonFn {
        params: Vec<String>,
        body: Arc<Expr>,
        text: String,
    },
    FuncHandle(String),
    /// Parenthesised expression: stops cs-list expansion and forces a single value.
    Paren(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum Access {
    Paren(Vec<Expr>),
    Brace(Vec<Expr>),
    Field(String),
    DynField(Expr),
}

#[derive(Debug, Clone)]
pub enum LValue {
    Var {
        name: String,
        path: Vec<Access>,
    },
    /// `~` placeholder in a multi-assignment.
    Ignore,
}

#[derive(Debug, Clone)]
pub struct Stmt {
    pub kind: StmtKind,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub enum StmtKind {
    Expr {
        expr: Expr,
        print: bool,
    },
    Assign {
        lhs: Vec<LValue>,
        rhs: Expr,
        print: bool,
    },
    If {
        clauses: Vec<(Expr, Vec<Stmt>)>,
        else_body: Option<Vec<Stmt>>,
    },
    For {
        var: String,
        iter: Expr,
        body: Vec<Stmt>,
    },
    While {
        cond: Expr,
        body: Vec<Stmt>,
    },
    Switch {
        subject: Expr,
        cases: Vec<(Expr, Vec<Stmt>)>,
        otherwise: Option<Vec<Stmt>>,
    },
    Try {
        body: Vec<Stmt>,
        ident: Option<String>,
        catch_body: Vec<Stmt>,
    },
    Break,
    Continue,
    Return,
    Global(Vec<String>),
    Persistent(Vec<String>),
    Command {
        name: String,
        args: Vec<String>,
        raw: String,
        print: bool,
    },
}

#[derive(Debug, Clone)]
pub struct Function {
    pub name: String,
    pub params: Vec<String>,
    pub outputs: Vec<String>,
    pub body: Vec<Stmt>,
    pub line: usize,
    pub help: String,
}

#[derive(Debug, Clone)]
pub struct FileUnit {
    pub path: String,
    /// Script statements (empty for function files).
    pub script: Option<Vec<Stmt>>,
    /// For function files, `functions[0]` is the main function.
    pub functions: Vec<Arc<Function>>,
}

impl FileUnit {
    pub fn find(&self, name: &str) -> Option<Arc<Function>> {
        self.functions.iter().find(|f| f.name == name).cloned()
    }
}

use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum WordPart {
    Literal(String),
    Quoted(String),
    Tilde(String),
    Parameter {
        name: String,
        subscript: Option<Subscript>,
        operator: Option<Box<ParameterOperation>>,
        quoted: bool,
    },
    Length {
        name: String,
        subscript: Option<Subscript>,
    },
    Indices {
        name: String,
        quoted: bool,
    },
    Indirect {
        name: String,
        operator: Option<Box<ParameterOperation>>,
        quoted: bool,
    },
    Names {
        prefix: String,
        joined: bool,
        quoted: bool,
    },
    CommandSubstitution {
        source: String,
        quoted: bool,
    },
    ProcessSubstitution {
        source: String,
        output: bool,
    },
    Arithmetic {
        expression: Word,
        quoted: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Subscript {
    All,
    Joined,
    Index(Word),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FallbackKind {
    Default,
    Assign,
    Alternative,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    Anywhere,
    Start,
    End,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ParameterOperation {
    Fallback {
        kind: FallbackKind,
        colon: bool,
        word: Word,
    },
    RemovePrefix {
        longest: bool,
        pattern: Word,
    },
    RemoveSuffix {
        longest: bool,
        pattern: Word,
    },
    Replace {
        all: bool,
        anchor: Anchor,
        pattern: Word,
        replacement: Word,
    },
    ChangeCase {
        upper: bool,
        all: bool,
    },
    Transform(char),
    Substring {
        offset: Word,
        length: Option<Word>,
    },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Word {
    pub parts: Vec<WordPart>,
}

impl Word {
    pub fn as_literal(&self) -> Option<&str> {
        match self.parts.as_slice() {
            [WordPart::Literal(text)] => Some(text),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RedirectKind {
    In,
    Out,
    Append,
    ReadWrite,
    DuplicateIn,
    DuplicateOut,
    OutAndError,
    AppendAndError,
    Heredoc,
    HereString,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Redirect {
    pub fd: i32,
    pub kind: RedirectKind,
    pub target: Word,
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignedValue {
    Scalar(Word),
    Array(Vec<Word>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    pub name: String,
    pub index: Option<Word>,
    pub append: bool,
    pub value: AssignedValue,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimpleCommand {
    pub assignments: Vec<Assignment>,
    pub words: Vec<Word>,
    pub declarations: Vec<Assignment>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CommandKind {
    Simple(SimpleCommand),
    Group(List),
    Subshell(List),
    If {
        branches: Vec<(List, List)>,
        otherwise: Option<List>,
    },
    Loop {
        condition: List,
        body: List,
        until: bool,
    },
    For {
        variable: String,
        items: Option<Vec<Word>>,
        body: List,
    },
    Case {
        subject: Word,
        arms: Vec<CaseArm>,
    },
    Conditional(Condition),
    Arithmetic(Word),
    ArithmeticFor {
        init: Word,
        condition: Word,
        step: Word,
        body: List,
    },
    FunctionDefinition {
        name: String,
        body: Rc<Command>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    And(Box<Condition>, Box<Condition>),
    Or(Box<Condition>, Box<Condition>),
    Not(Box<Condition>),
    Unary {
        operator: String,
        operand: Word,
    },
    Binary {
        operator: String,
        left: Word,
        right: Word,
    },
    Single(Word),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CaseEnd {
    Stop,
    FallThrough,
    TestNext,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CaseArm {
    pub end: CaseEnd,
    pub patterns: Vec<Word>,
    pub body: List,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub kind: CommandKind,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    pub negate: bool,
    pub timed: bool,
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Connector {
    And,
    Or,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AndOr {
    pub first: Pipeline,
    pub rest: Vec<(Connector, Pipeline)>,
    pub background: bool,
    pub text: String,
    pub line: usize,
}

pub type List = Vec<AndOr>;

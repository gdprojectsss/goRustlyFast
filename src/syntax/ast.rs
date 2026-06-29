use std::rc::Rc;

#[derive(Debug, Clone, PartialEq)]
pub enum WordPart {
    Literal(String),
    Quoted(String),
    Tilde(String),
    Parameter {
        name: String,
        operator: Option<Box<ParameterOperation>>,
        quoted: bool,
    },
    Length(String),
    CommandSubstitution {
        source: String,
        quoted: bool,
    },
    Arithmetic {
        expression: Word,
        quoted: bool,
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParameterOperationKind {
    Default,
    Assign,
    Alternative,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParameterOperation {
    pub kind: ParameterOperationKind,
    pub colon: bool,
    pub word: Word,
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

#[derive(Debug, Clone, PartialEq, Default)]
pub struct SimpleCommand {
    pub assignments: Vec<(String, Word)>,
    pub words: Vec<Word>,
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
    FunctionDefinition {
        name: String,
        body: Rc<Command>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Command {
    pub kind: CommandKind,
    pub redirects: Vec<Redirect>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Pipeline {
    pub negate: bool,
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
}

pub type List = Vec<AndOr>;

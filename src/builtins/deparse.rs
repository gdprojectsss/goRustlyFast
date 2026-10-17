use crate::syntax::ast::*;
use crate::syntax::lexer::{self, Token};

const INDENT: &str = "    ";

type Delimiters<'a> = std::slice::Iter<'a, (String, String)>;

pub(crate) fn function(name: &str, body: &Command) -> String {
    let mut printer = Printer::default();
    printer.out.push_str(name);
    printer.out.push_str(" () \n");
    match &body.kind {
        CommandKind::Group(list) => printer.group(list),
        _ => {
            printer.out.push_str("{ \n");
            printer.indent += 1;
            printer.newline_indent_text();
            printer.command(body, None);
            printer.flush_heredocs();
            printer.indent -= 1;
            printer.out.push_str("\n}");
            return printer.out;
        }
    }
    printer.redirects(&body.redirects, None);
    printer.out
}

#[derive(Default)]
struct Printer {
    out: String,
    indent: usize,
    pending_heredocs: Vec<String>,
    skip_semicolon: bool,
}

#[derive(Clone, Copy, PartialEq)]
enum Ending {
    Open,
    Terminated,
}

impl Printer {
    fn newline_indent_text(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str(INDENT);
        }
    }

    fn newline(&mut self) {
        self.out.push('\n');
        self.newline_indent_text();
    }

    fn flush_heredocs(&mut self) -> bool {
        if self.pending_heredocs.is_empty() {
            return false;
        }
        for heredoc in std::mem::take(&mut self.pending_heredocs) {
            self.out.push('\n');
            self.out.push_str(&heredoc);
        }
        self.skip_semicolon = true;
        true
    }

    fn separator(&mut self, background: bool) {
        if background {
            self.out.push_str(" &");
            return;
        }
        if self.flush_heredocs() {
            return;
        }
        if self.skip_semicolon {
            self.skip_semicolon = false;
            return;
        }
        self.out.push(';');
    }

    fn list(&mut self, list: &List, ending: Ending) {
        for (index, item) in list.iter().enumerate() {
            if index > 0 {
                self.newline();
            }
            self.and_or(item);
            let last = index + 1 == list.len();
            if !last || ending == Ending::Terminated || item.background {
                self.separator(item.background);
            } else {
                self.flush_heredocs();
            }
        }
    }

    fn inline_list(&mut self, list: &List) {
        for (index, item) in list.iter().enumerate() {
            if index > 0 {
                self.out.push(' ');
            }
            self.and_or(item);
            self.separator(item.background);
        }
    }

    fn group(&mut self, list: &List) {
        self.out.push_str("{ ");
        self.indent += 1;
        self.newline();
        self.list(list, Ending::Open);
        self.indent -= 1;
        self.newline();
        self.out.push('}');
    }

    fn and_or(&mut self, item: &AndOr) {
        if let Some(text) = raw_and_or(item) {
            self.out.push_str(&text);
            return;
        }
        let delimiters = heredoc_delimiters(&item.text);
        let mut delimiters = delimiters.iter();
        self.pipeline(&item.first, &mut delimiters);
        for (connector, pipeline) in &item.rest {
            self.out.push_str(match connector {
                Connector::And => " && ",
                Connector::Or => " || ",
            });
            self.pipeline(pipeline, &mut delimiters);
        }
    }

    fn pipeline(&mut self, pipeline: &Pipeline, delimiters: &mut Delimiters) {
        if pipeline.timed {
            self.out.push_str("time ");
        }
        if pipeline.negate {
            self.out.push_str("! ");
        }
        for (index, command) in pipeline.commands.iter().enumerate() {
            if index > 0 {
                self.out.push_str(" | ");
            }
            self.command(command, Some(delimiters));
        }
    }

    fn command(&mut self, command: &Command, delimiters: Option<&mut Delimiters>) {
        match &command.kind {
            CommandKind::Simple(simple) => self.simple(simple),
            CommandKind::Group(list) => self.group(list),
            CommandKind::Subshell(list) => {
                self.out.push_str("( ");
                self.list(list, Ending::Open);
                self.out.push_str(" )");
            }
            CommandKind::If {
                branches,
                otherwise,
            } => self.if_clause(branches, otherwise.as_ref()),
            CommandKind::Loop {
                condition,
                body,
                until,
            } => {
                self.out.push_str(if *until { "until " } else { "while " });
                self.inline_list(condition);
                self.out.push_str(" do");
                self.body(body);
                self.out.push_str("done");
            }
            CommandKind::For {
                variable,
                items,
                body,
            } => {
                self.out.push_str("for ");
                self.out.push_str(variable);
                self.out.push_str(" in ");
                match items {
                    Some(items) => {
                        let words: Vec<String> = items.iter().map(word).collect();
                        self.out.push_str(&words.join(" "));
                    }
                    None => self.out.push_str("\"$@\""),
                }
                self.out.push(';');
                self.newline();
                self.out.push_str("do");
                self.body(body);
                self.out.push_str("done");
            }
            CommandKind::ArithmeticFor {
                init,
                condition,
                step,
                body,
            } => {
                self.out.push_str(&format!(
                    "for (({}; {}; {}))",
                    raw_word(init).trim(),
                    raw_word(condition).trim(),
                    raw_word(step).trim()
                ));
                self.newline();
                self.out.push_str("do");
                self.body(body);
                self.out.push_str("done");
            }
            CommandKind::Case { subject, arms } => self.case(subject, arms),
            CommandKind::Conditional(condition) => {
                self.out.push_str("[[ ");
                self.out.push_str(&self::condition(condition));
                self.out.push_str(" ]]");
            }
            CommandKind::Arithmetic(expression) => {
                self.out.push_str("(( ");
                self.out.push_str(raw_word(expression).trim());
                self.out.push_str(" ))");
            }
            CommandKind::FunctionDefinition { name, body } => {
                let nested = function(name, body);
                let indentation = INDENT.repeat(self.indent);
                self.out
                    .push_str(&nested.replace('\n', &format!("\n{indentation}")));
            }
        }
        self.redirects(&command.redirects, delimiters);
    }

    fn body(&mut self, body: &List) {
        self.indent += 1;
        self.newline();
        self.list(body, Ending::Terminated);
        self.indent -= 1;
        self.newline();
    }

    fn if_clause(&mut self, branches: &[(List, List)], otherwise: Option<&List>) {
        let Some(((condition, body), rest)) = branches.split_first() else {
            return;
        };
        self.out.push_str("if ");
        self.inline_list(condition);
        self.out.push_str(" then");
        self.body(body);
        if !rest.is_empty() {
            self.out.push_str("else");
            self.indent += 1;
            self.newline();
            self.if_clause(rest, otherwise);
            self.out.push(';');
            self.indent -= 1;
            self.newline();
        } else if let Some(otherwise) = otherwise {
            self.out.push_str("else");
            self.body(otherwise);
        }
        self.out.push_str("fi");
    }

    fn case(&mut self, subject: &Word, arms: &[CaseArm]) {
        self.out.push_str("case ");
        self.out.push_str(&word(subject));
        self.out.push_str(" in ");
        self.indent += 1;
        for arm in arms {
            self.newline();
            let patterns: Vec<String> = arm.patterns.iter().map(word).collect();
            self.out.push_str(&patterns.join(" | "));
            self.out.push(')');
            self.indent += 1;
            if arm.body.is_empty() {
                self.out.push('\n');
            } else {
                self.newline();
                self.list(&arm.body, Ending::Open);
            }
            self.indent -= 1;
            self.newline();
            self.out.push_str(match arm.end {
                CaseEnd::Stop => ";;",
                CaseEnd::FallThrough => ";&",
                CaseEnd::TestNext => ";;&",
            });
        }
        self.indent -= 1;
        self.newline();
        self.out.push_str("esac");
    }

    fn simple(&mut self, simple: &SimpleCommand) {
        let mut parts: Vec<String> = simple.assignments.iter().map(assignment).collect();
        parts.extend(simple.words.iter().map(word));
        parts.extend(simple.declarations.iter().map(assignment));
        self.out.push_str(&parts.join(" "));
    }

    fn redirects(&mut self, redirects: &[Redirect], mut delimiters: Option<&mut Delimiters>) {
        for redirect in redirects {
            if !self.out.ends_with(' ') && !self.out.ends_with('\n') && !self.out.is_empty() {
                self.out.push(' ');
            }
            if redirect.kind == RedirectKind::Heredoc {
                let (operator, delimiter) = delimiters
                    .as_mut()
                    .and_then(|delimiters| delimiters.next())
                    .cloned()
                    .unwrap_or_else(|| ("<<EOF".into(), "EOF".into()));
                if redirect.fd != 0 {
                    self.out.push_str(&redirect.fd.to_string());
                }
                self.out.push_str(&operator);
                let body = raw_word(&redirect.target);
                self.pending_heredocs.push(format!("{body}{delimiter}\n"));
                continue;
            }
            self.out.push_str(&redirection(redirect));
        }
    }
}

fn redirection(redirect: &Redirect) -> String {
    let (default_fd, operator) = match redirect.kind {
        RedirectKind::In => (0, "<"),
        RedirectKind::Out => (1, ">"),
        RedirectKind::Clobber => (1, ">|"),
        RedirectKind::Append => (1, ">>"),
        RedirectKind::ReadWrite => (0, "<>"),
        RedirectKind::DuplicateIn => (0, "<&"),
        RedirectKind::DuplicateOut => (1, ">&"),
        RedirectKind::OutAndError => (1, "&>"),
        RedirectKind::AppendAndError => (1, "&>>"),
        RedirectKind::Heredoc => (0, "<<"),
        RedirectKind::HereString => (0, "<<<"),
    };
    let fd = if redirect.fd == default_fd {
        String::new()
    } else {
        redirect.fd.to_string()
    };
    let target = word(&redirect.target);
    if matches!(
        redirect.kind,
        RedirectKind::DuplicateIn | RedirectKind::DuplicateOut
    ) {
        format!("{fd}{operator}{target}")
    } else {
        format!("{fd}{operator} {target}")
    }
}

fn heredoc_delimiters(text: &str) -> Vec<(String, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut found = Vec::new();
    let mut index = 0;
    let mut quote: Option<char> = None;
    while index + 1 < chars.len() {
        let character = chars[index];
        match quote {
            Some(open) if character == open => quote = None,
            Some(_) => {}
            None if character == '\'' || character == '"' => quote = Some(character),
            None if character == '\\' => index += 1,
            None if character == '<' && chars[index + 1] == '<' => {
                if chars.get(index + 2) == Some(&'<') {
                    index += 3;
                    continue;
                }
                let mut position = index + 2;
                let mut operator = String::from("<<");
                if chars.get(position) == Some(&'-') {
                    operator.push('-');
                    position += 1;
                }
                while chars
                    .get(position)
                    .is_some_and(|next| *next == ' ' || *next == '\t')
                {
                    position += 1;
                }
                let start = position;
                while chars
                    .get(position)
                    .is_some_and(|next| !" \t\n;|&<>()".contains(*next))
                {
                    position += 1;
                }
                let raw: String = chars[start..position].iter().collect();
                let delimiter: String = raw
                    .chars()
                    .filter(|next| !"'\"\\".contains(*next))
                    .collect();
                operator.push_str(&raw);
                found.push((operator, delimiter));
                index = position;
                continue;
            }
            None => {}
        }
        index += 1;
    }
    found
}

fn is_plain(command: &Command) -> bool {
    let plain_kind = matches!(
        command.kind,
        CommandKind::Simple(_) | CommandKind::Conditional(_) | CommandKind::Arithmetic(_)
    );
    plain_kind
        && command
            .redirects
            .iter()
            .all(|redirect| redirect.kind != RedirectKind::Heredoc)
}

fn raw_and_or(item: &AndOr) -> Option<String> {
    let all_plain = std::iter::once(&item.first)
        .chain(item.rest.iter().map(|(_, pipeline)| pipeline))
        .all(|pipeline| pipeline.commands.iter().all(is_plain));
    if !all_plain || item.text.is_empty() {
        return None;
    }
    let lexed = lexer::lex(&item.text).ok()?;
    let chars: Vec<char> = item.text.chars().collect();
    let slice = |start: usize, end: usize| chars[start..end].iter().collect::<String>();
    let mut out = String::new();
    let mut attach_next = true;
    for spanned in &lexed.tokens {
        let text = slice(spanned.start, spanned.end);
        let (piece, attach_after) = match &spanned.token {
            Token::Eof => break,
            Token::Newline => continue,
            Token::Word(_) => (text, false),
            Token::Arithmetic(source) => (format!("(( {} ))", source.trim()), false),
            Token::Redirect(_, kind) => (
                text,
                matches!(kind, RedirectKind::DuplicateIn | RedirectKind::DuplicateOut),
            ),
            Token::Operator("(") => {
                out.push('(');
                attach_next = true;
                continue;
            }
            Token::Operator(")") => {
                out.push(')');
                continue;
            }
            Token::Operator("!") => ("!".to_string(), false),
            Token::Operator(operator) => (operator.to_string(), false),
            Token::Heredoc { .. } => return None,
        };
        if !attach_next && !out.is_empty() {
            out.push(' ');
        }
        out.push_str(&piece);
        attach_next = attach_after;
    }
    Some(out)
}

fn assignment(assignment: &Assignment) -> String {
    let mut text = assignment.name.clone();
    if let Some(index) = &assignment.index {
        text.push('[');
        text.push_str(&raw_word(index));
        text.push(']');
    }
    if assignment.append {
        text.push('+');
    }
    text.push('=');
    match &assignment.value {
        AssignedValue::Scalar(value) => text.push_str(&word(value)),
        AssignedValue::Array(values) => {
            let values: Vec<String> = values.iter().map(word).collect();
            text.push('(');
            text.push_str(&values.join(" "));
            text.push(')');
        }
    }
    text
}

fn condition(condition: &Condition) -> String {
    let grouped = |inner: &Condition| match inner {
        Condition::And(..) | Condition::Or(..) => format!("( {} )", self::condition(inner)),
        _ => self::condition(inner),
    };
    match condition {
        Condition::And(left, right) => format!("{} && {}", grouped(left), grouped(right)),
        Condition::Or(left, right) => format!("{} || {}", grouped(left), grouped(right)),
        Condition::Not(inner) => format!("! {}", grouped(inner)),
        Condition::Unary { operator, operand } => format!("{operator} {}", word(operand)),
        Condition::Binary {
            operator,
            left,
            right,
        } => format!("{} {operator} {}", word(left), word(right)),
        Condition::Single(operand) => word(operand),
    }
}

fn is_quoted(part: &WordPart) -> bool {
    match part {
        WordPart::Quoted(_) => true,
        WordPart::Parameter { quoted, .. }
        | WordPart::Indices { quoted, .. }
        | WordPart::Indirect { quoted, .. }
        | WordPart::Names { quoted, .. }
        | WordPart::CommandSubstitution { quoted, .. }
        | WordPart::Arithmetic { quoted, .. } => *quoted,
        _ => false,
    }
}

pub(crate) fn word(word: &Word) -> String {
    let mut out = String::new();
    let parts = &word.parts;
    let mut index = 0;
    while index < parts.len() {
        if is_quoted(&parts[index]) {
            let end = parts[index..]
                .iter()
                .position(|part| !is_quoted(part))
                .map_or(parts.len(), |offset| index + offset);
            out.push_str(&quoted_group(&parts[index..end]));
            index = end;
            continue;
        }
        let next = parts.get(index + 1);
        match &parts[index] {
            WordPart::Literal(text) => out.push_str(&escape_literal(text)),
            part => out.push_str(&expansion(part, next)),
        }
        index += 1;
    }
    out
}

fn quoted_group(parts: &[WordPart]) -> String {
    let only_text = parts.iter().all(|part| matches!(part, WordPart::Quoted(_)));
    if only_text {
        let text: String = parts
            .iter()
            .map(|part| match part {
                WordPart::Quoted(text) => text.as_str(),
                _ => "",
            })
            .collect();
        let needs_escapes = text.contains(['$', '`', '\\', '"', '!']);
        if needs_escapes && !text.contains('\'') {
            return format!("'{text}'");
        }
    }
    let mut out = String::from("\"");
    for (index, part) in parts.iter().enumerate() {
        match part {
            WordPart::Quoted(text) => {
                for character in text.chars() {
                    if matches!(character, '$' | '`' | '\\' | '"') {
                        out.push('\\');
                    }
                    out.push(character);
                }
            }
            part => out.push_str(&expansion(part, parts.get(index + 1))),
        }
    }
    out.push('"');
    out
}

fn escape_literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for character in text.chars() {
        if " \t\n'\"\\|&;()<>$`".contains(character) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

fn raw_word(word: &Word) -> String {
    let mut out = String::new();
    for (index, part) in word.parts.iter().enumerate() {
        match part {
            WordPart::Literal(text) | WordPart::Quoted(text) => out.push_str(text),
            part => out.push_str(&expansion(part, word.parts.get(index + 1))),
        }
    }
    out
}

fn subscript(subscript: &Option<Subscript>) -> String {
    match subscript {
        None => String::new(),
        Some(Subscript::All) => "[@]".into(),
        Some(Subscript::Joined) => "[*]".into(),
        Some(Subscript::Index(index)) => format!("[{}]", raw_word(index)),
    }
}

fn starts_name(part: Option<&WordPart>) -> bool {
    let text = match part {
        Some(WordPart::Literal(text)) | Some(WordPart::Quoted(text)) => text,
        _ => return false,
    };
    text.starts_with(|character: char| character.is_ascii_alphanumeric() || character == '_')
}

fn expansion(part: &WordPart, next: Option<&WordPart>) -> String {
    match part {
        WordPart::Literal(text) | WordPart::Quoted(text) => text.clone(),
        WordPart::Tilde(user) => format!("~{user}"),
        WordPart::Parameter {
            name,
            subscript: index,
            operator,
            ..
        } => {
            let simple = index.is_none()
                && operator.is_none()
                && (name.len() == 1
                    || !name.starts_with(|character: char| character.is_ascii_digit()))
                && !(starts_name(next) && crate::syntax::parser::is_name(name));
            if simple {
                format!("${name}")
            } else {
                let operation = operator
                    .as_deref()
                    .map(parameter_operation)
                    .unwrap_or_default();
                format!("${{{name}{}{operation}}}", subscript(index))
            }
        }
        WordPart::Length {
            name,
            subscript: index,
        } => format!("${{#{name}{}}}", subscript(index)),
        WordPart::Indices { name, .. } => format!("${{!{name}[@]}}"),
        WordPart::Indirect { name, operator, .. } => {
            let operation = operator
                .as_deref()
                .map(parameter_operation)
                .unwrap_or_default();
            format!("${{!{name}{operation}}}")
        }
        WordPart::Names { prefix, joined, .. } => {
            format!("${{!{prefix}{}}}", if *joined { '*' } else { '@' })
        }
        WordPart::CommandSubstitution { source, .. } => format!("$({source})"),
        WordPart::ProcessSubstitution { source, output } => {
            format!("{}({source})", if *output { '>' } else { '<' })
        }
        WordPart::Arithmetic { expression, .. } => format!("$(({}))", raw_word(expression)),
    }
}

fn parameter_operation(operation: &ParameterOperation) -> String {
    match operation {
        ParameterOperation::Fallback { kind, colon, word } => {
            let symbol = match kind {
                FallbackKind::Default => '-',
                FallbackKind::Assign => '=',
                FallbackKind::Alternative => '+',
                FallbackKind::Error => '?',
            };
            let colon = if *colon { ":" } else { "" };
            format!("{colon}{symbol}{}", self::word(word))
        }
        ParameterOperation::RemovePrefix { longest, pattern } => {
            format!(
                "{}{}",
                if *longest { "##" } else { "#" },
                self::word(pattern)
            )
        }
        ParameterOperation::RemoveSuffix { longest, pattern } => {
            format!(
                "{}{}",
                if *longest { "%%" } else { "%" },
                self::word(pattern)
            )
        }
        ParameterOperation::Replace {
            all,
            anchor,
            pattern,
            replacement,
        } => {
            let marker = match (all, anchor) {
                (true, _) => "//",
                (false, Anchor::Start) => "/#",
                (false, Anchor::End) => "/%",
                (false, Anchor::Anywhere) => "/",
            };
            format!(
                "{marker}{}/{}",
                self::word(pattern),
                self::word(replacement)
            )
        }
        ParameterOperation::ChangeCase { upper, all } => {
            let symbol = if *upper { "^" } else { "," };
            if *all {
                symbol.repeat(2)
            } else {
                symbol.to_string()
            }
        }
        ParameterOperation::Transform(letter) => format!("@{letter}"),
        ParameterOperation::Substring { offset, length } => match length {
            Some(length) => format!(":{}:{}", raw_word(offset), raw_word(length)),
            None => format!(":{}", raw_word(offset)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parser;

    fn printed(source: &str) -> String {
        let list = parser::parse_from_line(source, &Default::default(), 1).unwrap();
        let CommandKind::FunctionDefinition { name, body } = &list[0].first.commands[0].kind else {
            panic!("not a function");
        };
        function(name, body)
    }

    #[test]
    fn simple_bodies_keep_word_text() {
        assert_eq!(
            printed("f() { echo   'a'  \"b $x\"|cat;x=(1 2) y=2 cmd >out 2>&1 & }"),
            "f () \n{ \n    echo 'a' \"b $x\" | cat;\n    x=(1 2) y=2 cmd > out 2>&1 &\n}"
        );
    }

    #[test]
    fn compound_commands_are_indented() {
        assert_eq!(
            printed("f() { if true; then for i in a b; do echo $i; done; fi; }"),
            "f () \n{ \n    if true; then\n        for i in a b;\n        do\n            echo $i;\n        done;\n    fi\n}"
        );
        assert_eq!(
            printed("f() { case $1 in a|b) echo ab;; *) ;; esac; }"),
            "f () \n{ \n    case $1 in \n        a | b)\n            echo ab\n        ;;\n        *)\n\n        ;;\n    esac\n}"
        );
    }

    #[test]
    fn heredocs_follow_their_command() {
        assert_eq!(
            printed("f() { cat <<EOF\nhi $x\nEOF\necho after; }"),
            "f () \n{ \n    cat <<EOF\nhi $x\nEOF\n\n    echo after\n}"
        );
    }

    #[test]
    fn words_are_requoted() {
        let word = Word {
            parts: vec![
                WordPart::Literal("a b".into()),
                WordPart::Quoted("c$".into()),
            ],
        };
        assert_eq!(self::word(&word), "a\\ b'c$'");
    }
}

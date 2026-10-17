use crate::expand::glob;
use crate::shell::Shell;
use crate::syntax::ast::{Command, CommandKind, List};
use crate::syntax::{lexer, parser};

const PIECES: &[&str] = &[
    "if", "then", "elif", "else", "fi", "for", "in", "do", "done", "while", "until", "case",
    "esac", ";;", ";&", ";;&", "|", "||", "&&", ";", "&", "(", ")", "{", "}", "[[", "]]", "!", "<",
    ">", ">>", "<<", "<<-", "<<<", "2>&1", "&>", "EOF", "$", "${", "}", "$(", "$((", "))", "`",
    "'", "\"", "\\", "\n", " ", " ", " ", "x", "y", "1", "*", "?", "[", "]", "=", "+=", "#", "~",
    ":", "-", "%", "/", "^", ",", "..", "@", "f()", "=~", "a=(", "$?", "$@", "${x:-", "${x#",
    "${#",
];

struct Random(u64);

impl Random {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, limit: usize) -> usize {
        (self.next() % limit as u64) as usize
    }

    fn input(&mut self) -> String {
        let length = self.below(40);
        (0..length)
            .map(|_| PIECES[self.below(PIECES.len())])
            .collect()
    }
}

fn visit_words(list: &List, visit: &mut impl FnMut(&crate::syntax::ast::Word)) {
    fn command(command: &Command, visit: &mut impl FnMut(&crate::syntax::ast::Word)) {
        match &command.kind {
            CommandKind::Simple(simple) => simple.words.iter().for_each(&mut *visit),
            CommandKind::Group(list) | CommandKind::Subshell(list) => visit_words(list, visit),
            CommandKind::If {
                branches,
                otherwise,
            } => {
                for (condition, body) in branches {
                    visit_words(condition, visit);
                    visit_words(body, visit);
                }
                if let Some(list) = otherwise {
                    visit_words(list, visit);
                }
            }
            CommandKind::Loop {
                condition, body, ..
            } => {
                visit_words(condition, visit);
                visit_words(body, visit);
            }
            CommandKind::For { body, .. }
            | CommandKind::ArithmeticFor { body, .. }
            | CommandKind::Select { body, .. } => visit_words(body, visit),
            CommandKind::Coproc { .. } => {}
            CommandKind::Case { arms, .. } => {
                arms.iter().for_each(|arm| visit_words(&arm.body, visit))
            }
            CommandKind::Conditional(_)
            | CommandKind::Arithmetic(_)
            | CommandKind::FunctionDefinition { .. } => {}
        }
    }
    for item in list {
        for pipeline in
            std::iter::once(&item.first).chain(item.rest.iter().map(|(_, pipeline)| pipeline))
        {
            for each in &pipeline.commands {
                command(each, visit);
            }
        }
    }
}

#[test]
fn random_input_never_panics() {
    let mut random = Random(0x9E37_79B9_7F4A_7C15);
    let mut shell = Shell::new();
    for _ in 0..30_000 {
        let input = random.input();
        let _ = lexer::lex(&input);
        if let Ok(list) = parser::parse(&input) {
            visit_words(&list, &mut |word| {
                let _ = crate::expand::brace_expand(word);
            });
        }
        let _ = crate::expand::evaluate_arithmetic(&mut shell, &input);
        let pattern = random.input();
        let _ = glob::matches_pattern(&pattern, &input);
        let _ = glob::remove_prefix(&input, &pattern, random.below(2) == 0);
        let _ = glob::remove_suffix(&input, &pattern, random.below(2) == 0);
    }
}

fn with_large_stack(test: fn()) {
    std::thread::Builder::new()
        .stack_size(512 << 20)
        .spawn(test)
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn deep_nesting_is_handled() {
    with_large_stack(|| {
        let nested = format!("{}echo hi{}", "( ".repeat(200), " )".repeat(200));
        assert!(parser::parse(&nested).is_ok());
        let braces = format!("echo {}x{}", "{a,".repeat(50), "}".repeat(50));
        assert!(parser::parse(&braces).is_ok());
    });
}

#[test]
fn excessive_nesting_is_a_syntax_error() {
    with_large_stack(|| {
        for source in [
            format!("{}echo hi{}", "( ".repeat(100_000), " )".repeat(100_000)),
            format!("echo $(({}1{}))", "(".repeat(100_000), ")".repeat(100_000)),
            format!("echo {}x{}", "${x:-".repeat(10_000), "}".repeat(10_000)),
            format!("echo {}x{}", "$(echo ".repeat(5_000), ")".repeat(5_000)),
            "{ ".repeat(10_000),
            format!(
                "{}echo{}",
                "if true; then ".repeat(10_000),
                "; fi".repeat(10_000)
            ),
        ] {
            let result = parser::parse(&source);
            if source.starts_with("echo $((") {
                assert!(result.is_ok());
                continue;
            }
            let error = result.unwrap_err();
            assert!(!error.incomplete, "{}", &source[..20]);
        }
    });
}

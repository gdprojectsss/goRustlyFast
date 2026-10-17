mod agent;
mod builtins;
mod check;
mod commands;
mod directories;
mod exec;
mod expand;
mod jobs;
pub(crate) mod repl;
mod shell;
mod signals;
mod stack;
mod syntax;
mod sys;
mod table;
mod text;
mod trust;
mod variables;

pub fn run() -> i32 {
    stack::init();
    repl::run()
}

#[doc(hidden)]
pub mod fuzzing {
    pub fn parse(source: &str) {
        let _ = crate::syntax::lexer::lex(source);
        if let Ok(list) = crate::syntax::parser::parse(source) {
            let _ = crate::check::check_source(source, &[]);
            drop(list);
        }
    }

    pub fn words(source: &str) {
        let word = crate::syntax::lexer::parse_double_quoted_text(source);
        if let Ok(word) = word {
            let _ = crate::expand::brace_expand(&word);
        }
        let _ = crate::syntax::lexer::parse_regex_word(source);
    }

    pub fn pattern(pattern: &str, text: &str) {
        use crate::expand::glob;
        let _ = glob::matches_pattern(pattern, text);
        let _ = glob::remove_prefix(text, pattern, true);
        let _ = glob::remove_suffix(text, pattern, false);
        let _ = glob::replace(
            text,
            pattern,
            "x",
            true,
            crate::syntax::ast::Anchor::Anywhere,
        );
    }

    pub fn arithmetic(text: &str) {
        let mut shell = crate::shell::Shell::new();
        let _ = crate::expand::evaluate_arithmetic(&mut shell, text);
    }

    pub fn data(text: &str) {
        crate::table::parse_untrusted(text);
        let _ = crate::repl::theme::Theme::parse(text);
    }
}

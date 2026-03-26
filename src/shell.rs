use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Display;

pub struct Shell {
    pub aliases: HashMap<String, String>,
}

impl Shell {
    pub fn new() -> Self {
        Shell {
            aliases: HashMap::from([
                ("ls".into(), "ls --color=auto".into()),
                ("grep".into(), "grep --color=auto".into()),
            ]),
        }
    }

    pub fn expand_aliases<'a>(&self, input: &'a str) -> Cow<'a, str> {
        let trimmed = input.trim_start();
        let end = trimmed
            .find(|c: char| c.is_whitespace())
            .unwrap_or(trimmed.len());

        match self.aliases.get(&trimmed[..end]) {
            Some(expansion) => format!("{}{}", expansion, &trimmed[end..]).into(),
            None => input.into(),
        }
    }
}

pub fn error(msg: impl Display) {
    eprintln!("\x1b[1;31mgrfsh:\x1b[0m {}", msg);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_aliases() {
        let shell = Shell::new();
        assert_eq!(shell.aliases.get("ls").unwrap(), "ls --color=auto");
        assert_eq!(shell.aliases.get("grep").unwrap(), "grep --color=auto");
    }

    #[test]
    fn expand_known_alias() {
        let shell = Shell::new();
        assert_eq!(shell.expand_aliases("ls -la").as_ref(), "ls --color=auto -la");
    }

    #[test]
    fn expand_unknown_returns_borrowed() {
        let shell = Shell::new();
        let input = "echo hello";
        let result = shell.expand_aliases(input);
        assert!(matches!(result, Cow::Borrowed(_)));
        assert_eq!(result.as_ref(), "echo hello");
    }

    #[test]
    fn expand_bare_alias() {
        let shell = Shell::new();
        assert_eq!(shell.expand_aliases("ls").as_ref(), "ls --color=auto");
    }

    #[test]
    fn expand_preserves_args() {
        let mut shell = Shell::new();
        shell.aliases.insert("g".into(), "git".into());
        assert_eq!(shell.expand_aliases("g status --short").as_ref(), "git status --short");
    }

    #[test]
    fn expand_empty_input() {
        let shell = Shell::new();
        let result = shell.expand_aliases("");
        assert_eq!(result.as_ref(), "");
    }
}

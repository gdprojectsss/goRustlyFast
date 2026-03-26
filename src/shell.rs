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

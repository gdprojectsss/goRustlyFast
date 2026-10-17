mod files;
mod sources;

use rustyline::completion::Pair;

#[derive(Debug, Default, Clone)]
pub(crate) struct ShellNames {
    pub commands: Vec<String>,
    pub variables: Vec<String>,
    pub aliases: Vec<String>,
    pub jobs: Vec<String>,
    pub specs: std::collections::BTreeMap<String, crate::shell::CompletionSpec>,
}

#[derive(Debug, PartialEq)]
pub(super) struct Request {
    pub words: Vec<String>,
    pub current: String,
    pub start: usize,
}

#[derive(Debug, PartialEq)]
pub(super) enum Target {
    Commands,
    Variables { prefix: String },
    VariableNames,
    Aliases,
    Jobs,
    Directories,
    Files,
    Words(Vec<String>),
    WordsAndFiles(Vec<String>),
    WordsAndDirectories(Vec<String>),
}

const COMMAND_PREFIXES: &[&str] = &["sudo", "time", "command", "exec", "nohup", "nice", "env"];
const GIT_REF_COMMANDS: &[&str] = &[
    "checkout",
    "switch",
    "merge",
    "rebase",
    "branch",
    "cherry-pick",
    "reset",
    "diff",
    "log",
    "show",
    "push",
    "pull",
    "tag",
];

pub(super) fn analyze(before_cursor: &str) -> Request {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut start = 0;
    let mut quote: Option<char> = None;
    let mut chars = before_cursor.char_indices().peekable();

    while let Some((index, character)) = chars.next() {
        match (quote, character) {
            (Some(open), _) if character == open => quote = None,
            (Some(_), _) => current.push(character),
            (None, '\'' | '"') => quote = Some(character),
            (None, '\\') => {
                if let Some((_, escaped)) = chars.next() {
                    current.push(escaped);
                }
            }
            (None, ' ' | '\t') => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
                start = index + 1;
            }
            (None, '|' | ';' | '&' | '(' | ')' | '\n') => {
                words.clear();
                current.clear();
                start = index + character.len_utf8();
            }
            (None, _) => current.push(character),
        }
    }

    Request {
        words,
        current,
        start,
    }
}

fn command_words(words: &[String]) -> &[String] {
    let mut rest = words;
    loop {
        match rest.first() {
            Some(word) if word.contains('=') && !word.starts_with('=') => rest = &rest[1..],
            Some(word) if COMMAND_PREFIXES.contains(&word.as_str()) => rest = &rest[1..],
            _ => return rest,
        }
    }
}

fn run_generator(source: &str, request: &Request) -> Vec<String> {
    let line: Vec<&str> = request.words.iter().map(String::as_str).collect();
    std::process::Command::new("sh")
        .args(["-c", source])
        .env(
            "COMP_LINE",
            format!("{} {}", line.join(" "), request.current),
        )
        .env("COMP_WORD", &request.current)
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .split_whitespace()
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn target_with(request: &Request, names: &ShellNames) -> Target {
    let words = command_words(&request.words);
    if let Some(command) = words.first()
        && let Some(spec) = names.specs.get(command)
        && !request.current.starts_with('$')
    {
        let mut candidates = spec.words.clone();
        if let Some(source) = &spec.command {
            candidates.extend(run_generator(source, request));
        }
        return match (spec.directories, spec.files) {
            (_, true) => Target::WordsAndFiles(candidates),
            (true, false) if candidates.is_empty() => Target::Directories,
            (true, false) => Target::WordsAndDirectories(candidates),
            (false, false) => Target::Words(candidates),
        };
    }
    target(request)
}

pub(super) fn target(request: &Request) -> Target {
    if let Some(prefix) = request.current.strip_prefix('$')
        && !prefix.contains('/')
    {
        return Target::Variables {
            prefix: prefix.to_string(),
        };
    }
    let words = command_words(&request.words);
    let Some(command) = words.first() else {
        return if request.current.contains('/') {
            Target::Files
        } else {
            Target::Commands
        };
    };
    let position = words.len();

    match command.as_str() {
        "cd" | "z" | "pushd" | "rmdir" => Target::Directories,
        "git" if position == 1 => Target::Words(sources::git_subcommands()),
        "git" if GIT_REF_COMMANDS.contains(&words[1].as_str()) => {
            Target::WordsAndFiles(sources::git_refs())
        }
        "cargo" if position == 1 => Target::Words(sources::cargo_subcommands()),
        "ssh" | "mosh" | "ping" | "dig" | "host" => Target::Words(sources::ssh_hosts()),
        "scp" | "rsync" | "sftp" => Target::WordsAndFiles(
            sources::ssh_hosts()
                .into_iter()
                .map(|host| format!("{host}:"))
                .collect(),
        ),
        "make" => Target::WordsAndFiles(sources::make_targets()),
        "fg" | "bg" | "wait" | "kill" if request.current.starts_with('%') => Target::Jobs,
        "unset" | "export" | "read" | "local" => Target::VariableNames,
        "unalias" => Target::Aliases,
        "type" | "which" | "man" | "whatis" => Target::Commands,
        _ => Target::Files,
    }
}

pub(super) fn candidates(target: &Target, current: &str, names: &ShellNames) -> Vec<Pair> {
    let pool: Vec<String> = match target {
        Target::Commands => names.commands.clone(),
        Target::Variables { prefix } => {
            return names
                .variables
                .iter()
                .filter(|name| name.starts_with(prefix.as_str()))
                .map(|name| pair(format!("${name}")))
                .collect();
        }
        Target::VariableNames => names.variables.clone(),
        Target::Aliases => names.aliases.clone(),
        Target::Jobs => names.jobs.clone(),
        Target::Words(words)
        | Target::WordsAndFiles(words)
        | Target::WordsAndDirectories(words) => words.clone(),
        Target::Directories | Target::Files => Vec::new(),
    };
    let mut matches: Vec<String> = pool
        .into_iter()
        .filter(|candidate| candidate.starts_with(current))
        .collect();
    matches.sort();
    matches.dedup();
    matches.into_iter().map(pair).collect()
}

pub(super) fn files(raw: &str, directories_only: bool) -> Vec<Pair> {
    files::complete(raw, directories_only)
}

pub(super) fn finish_single(mut pairs: Vec<Pair>) -> Vec<Pair> {
    if let [only] = pairs.as_mut_slice()
        && !only.replacement.ends_with(['/', ':', '='])
    {
        if let Some(open) = only
            .replacement
            .chars()
            .next()
            .filter(|first| *first == '\'' || *first == '"')
            && only.replacement.matches(open).count() % 2 == 1
        {
            only.replacement.push(open);
        }
        only.replacement.push(' ');
    }
    pairs
}

fn pair(text: String) -> Pair {
    Pair {
        display: text.clone(),
        replacement: text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(list: &[&str]) -> Vec<String> {
        list.iter().map(|word| word.to_string()).collect()
    }

    #[test]
    fn analysis_tracks_the_last_command() {
        let request = analyze("ls | git chec");
        assert_eq!(request.words, words(&["git"]));
        assert_eq!(request.current, "chec");
        assert_eq!(request.start, 9);
    }

    #[test]
    fn quoted_spaces_stay_in_the_word() {
        let request = analyze("cat 'my fi");
        assert_eq!(request.words, words(&["cat"]));
        assert_eq!(request.current, "my fi");
    }

    #[test]
    fn trailing_space_starts_a_new_word() {
        let request = analyze("cd ");
        assert_eq!(request.current, "");
        assert_eq!(request.start, 3);
        assert_eq!(target(&request), Target::Directories);
    }

    #[test]
    fn targets_by_context() {
        assert_eq!(target(&analyze("gi")), Target::Commands);
        assert_eq!(target(&analyze("./scr")), Target::Files);
        assert_eq!(
            target(&analyze("echo $HO")),
            Target::Variables {
                prefix: "HO".into()
            }
        );
        assert_eq!(target(&analyze("ls $HOME/pro")), Target::Files);
        assert_eq!(target(&analyze("sudo vi")), Target::Commands);
        assert_eq!(target(&analyze("FOO=1 cd sr")), Target::Directories);
        assert_eq!(target(&analyze("fg %")), Target::Jobs);
        assert_eq!(target(&analyze("unset PA")), Target::VariableNames);
        assert_eq!(target(&analyze("vim notes")), Target::Files);
    }

    #[test]
    fn variable_candidates_keep_the_dollar() {
        let names = ShellNames {
            variables: words(&["HOME", "HOSTNAME", "PATH"]),
            ..ShellNames::default()
        };
        let target = Target::Variables {
            prefix: "HO".into(),
        };
        let found: Vec<String> = candidates(&target, "$HO", &names)
            .into_iter()
            .map(|pair| pair.replacement)
            .collect();
        assert_eq!(found, ["$HOME", "$HOSTNAME"]);
    }
}

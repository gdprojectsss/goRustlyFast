use std::path::Path;

const ZSH_FILES: &[&str] = &[".zshrc", ".zprofile", ".zshenv", ".zlogin", ".zlogout"];
const BASH_FILES: &[&str] = &[".bashrc", ".bash_profile", ".bash_login", ".bash_logout"];
const ZSH_COMMANDS: &[&str] = &[
    "setopt", "unsetopt", "autoload", "zstyle", "compinit", "zmodload", "compdef", "zle",
];

#[derive(Debug, PartialEq)]
enum Shell {
    Zsh,
    Bash,
}

fn by_name(path: &str) -> Option<Shell> {
    let name = Path::new(path).file_name()?.to_str()?;
    if ZSH_FILES.contains(&name) || name.ends_with(".zsh") {
        Some(Shell::Zsh)
    } else if BASH_FILES.contains(&name) {
        Some(Shell::Bash)
    } else {
        None
    }
}

fn by_contents(contents: &str) -> Option<Shell> {
    let zsh_line = |line: &str| {
        let line = line.trim_start();
        let first = line.split_whitespace().next().unwrap_or_default();
        ZSH_COMMANDS.contains(&first) || line.contains("oh-my-zsh.sh") || line.contains("${(")
    };
    contents.lines().any(zsh_line).then_some(Shell::Zsh)
}

pub(crate) fn startup_file_hint(path: &str, contents: &str) -> Option<String> {
    let shell = by_name(path).or_else(|| by_contents(contents))?;
    Some(match shell {
        Shell::Zsh => format!(
            "{path} looks like a zsh startup file; grfsh runs bash style scripts, so zsh only lines will fail. Copy what you need into ~/.grfshrc, see https://grf.sh/docs/migrating-from-zsh/"
        ),
        Shell::Bash => format!(
            "{path} is a bash startup file; lines that need bash only features grfsh lacks will fail. Copy what you need into ~/.grfshrc"
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zsh_files_are_recognized_by_name_and_contents() {
        assert!(
            startup_file_hint("/home/me/.zshrc", "export A=1")
                .is_some_and(|hint| hint.contains("zsh startup file"))
        );
        assert!(startup_file_hint("theme.zsh", "").is_some());
        assert!(startup_file_hint("rc", "export A=1\nsetopt autocd\n").is_some());
        assert!(startup_file_hint("rc", "source $ZSH/oh-my-zsh.sh").is_some());
    }

    #[test]
    fn bash_files_are_recognized_by_name() {
        assert!(
            startup_file_hint("/home/me/.bashrc", "shopt -s histappend")
                .is_some_and(|hint| hint.contains("bash startup file"))
        );
    }

    #[test]
    fn ordinary_scripts_get_no_hint() {
        assert_eq!(
            startup_file_hint("env.sh", "export A=1\nalias ll='ls -l'\n"),
            None
        );
    }
}

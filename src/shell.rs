use std::collections::HashMap;
use std::fmt::Display;
use std::rc::Rc;

use crate::directories::DirectoryHistory;
use crate::jobs::JobTable;
use crate::syntax::ast::Command;
use crate::sys;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Flow {
    Normal,
    Break(u32),
    Continue(u32),
    Return,
    Exit,
    Interrupted,
}

pub struct Shell {
    pub aliases: HashMap<String, String>,
    pub variables: HashMap<String, String>,
    pub functions: HashMap<String, Rc<Command>>,
    pub positional: Vec<String>,
    pub arg0: String,
    pub last_status: i32,
    pub substitution_status: Option<i32>,
    pub last_background_pid: Option<i32>,
    pub shell_pid: i32,
    pub interactive: bool,
    pub shell_pgid: i32,
    pub jobs: JobTable,
    pub flow: Flow,
    pub locals: Vec<HashMap<String, Option<String>>>,
    pub loop_depth: u32,
    pub source_depth: u32,
    pub path_commands: Option<(String, Vec<String>)>,
    pub directories: DirectoryHistory,
}

impl Shell {
    pub fn new() -> Self {
        let mut shell = Shell {
            aliases: HashMap::from([
                ("ls".into(), "ls --color=auto".into()),
                ("grep".into(), "grep --color=auto".into()),
            ]),
            variables: HashMap::new(),
            functions: HashMap::new(),
            positional: Vec::new(),
            arg0: "grfsh".into(),
            last_status: 0,
            substitution_status: None,
            last_background_pid: None,
            shell_pid: sys::getpid(),
            interactive: false,
            shell_pgid: 0,
            jobs: JobTable::default(),
            flow: Flow::Normal,
            locals: Vec::new(),
            loop_depth: 0,
            source_depth: 0,
            path_commands: None,
            directories: DirectoryHistory::new(
                std::env::var_os("HOME")
                    .map(|home| std::path::Path::new(&home).join(".grfsh_dirs")),
            ),
        };
        shell.init_working_directory();
        shell
    }

    fn init_working_directory(&mut self) {
        let Ok(current_dir) = std::env::current_dir() else {
            return;
        };
        let pwd_matches = std::env::var("PWD")
            .ok()
            .and_then(|path| std::fs::canonicalize(path).ok())
            .zip(std::fs::canonicalize(&current_dir).ok())
            .is_some_and(|(from_pwd, from_cwd)| from_pwd == from_cwd);
        if !pwd_matches {
            unsafe { std::env::set_var("PWD", current_dir) };
        }
    }

    pub fn get_variable(&self, name: &str) -> Option<String> {
        match name {
            "?" => return Some(self.last_status.to_string()),
            "$" => return Some(self.shell_pid.to_string()),
            "!" => return self.last_background_pid.map(|pid| pid.to_string()),
            "#" => return Some(self.positional.len().to_string()),
            "0" => return Some(self.arg0.clone()),
            "@" | "*" => return Some(self.positional.join(" ")),
            "-" => {
                return Some(if self.interactive {
                    "i".into()
                } else {
                    String::new()
                });
            }
            "RANDOM" => return Some((random() % 32768).to_string()),
            _ => {}
        }
        if let Ok(index) = name.parse::<usize>() {
            return self.positional.get(index.wrapping_sub(1)).cloned();
        }
        self.variables
            .get(name)
            .cloned()
            .or_else(|| std::env::var(name).ok())
    }

    pub fn set_variable(&mut self, name: &str, value: &str) {
        if !self.variables.contains_key(name) && std::env::var_os(name).is_some() {
            unsafe { std::env::set_var(name, value) };
        } else {
            self.variables.insert(name.into(), value.into());
        }
    }

    pub fn export_variable(&mut self, name: &str, value: Option<&str>) {
        let value = value
            .map(String::from)
            .or_else(|| self.variables.get(name).cloned());
        self.variables.remove(name);
        unsafe { std::env::set_var(name, value.unwrap_or_default()) };
    }

    pub fn unset_variable(&mut self, name: &str) {
        self.variables.remove(name);
        unsafe { std::env::remove_var(name) };
    }
}

fn random() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u128(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |duration| duration.as_nanos()),
    );
    hasher.finish()
}

pub fn error(message: impl Display) {
    eprintln!("\x1b[1;31mgrfsh:\x1b[0m {}", message);
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
    fn special_parameters() {
        let mut shell = Shell::new();
        shell.last_status = 3;
        shell.positional = vec!["a".into(), "b".into()];
        assert_eq!(shell.get_variable("?").as_deref(), Some("3"));
        assert_eq!(shell.get_variable("#").as_deref(), Some("2"));
        assert_eq!(shell.get_variable("2").as_deref(), Some("b"));
        assert_eq!(shell.get_variable("3"), None);
        assert_eq!(
            shell.get_variable("$"),
            Some(std::process::id().to_string())
        );
    }

    #[test]
    fn shell_variables_shadow_environment() {
        let mut shell = Shell::new();
        shell.set_variable("GRFSH_UNIT_LOCAL", "1");
        assert_eq!(shell.get_variable("GRFSH_UNIT_LOCAL").as_deref(), Some("1"));
        assert!(std::env::var("GRFSH_UNIT_LOCAL").is_err());
    }
}

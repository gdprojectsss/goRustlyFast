use std::collections::HashMap;
use std::fmt::Display;
use std::io::IsTerminal;
use std::rc::Rc;

use crate::directories::DirectoryHistory;
use crate::jobs::JobTable;
use crate::syntax::ast::Command;
use crate::sys;
use crate::variables::{Array, Attributes, Evaluated, Key, Saved};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Flow {
    Normal,
    Break(u32),
    Continue(u32),
    Return,
    Exit,
    Interrupted,
    Abort,
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ShellOptions {
    pub errexit: bool,
    pub nounset: bool,
    pub xtrace: bool,
    pub pipefail: bool,
    pub vi: bool,
    pub globstar: bool,
    pub nullglob: bool,
    pub dotglob: bool,
    pub nocaseglob: bool,
    pub extglob: bool,
}

impl ShellOptions {
    pub fn flags(&self) -> String {
        [(self.errexit, 'e'), (self.nounset, 'u'), (self.xtrace, 'x')]
            .iter()
            .filter(|(enabled, _)| *enabled)
            .map(|(_, flag)| *flag)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum KeyBinding {
    Action(String),
    Insert(String),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct CompletionSpec {
    pub words: Vec<String>,
    pub command: Option<String>,
    pub directories: bool,
    pub files: bool,
}

pub struct Shell {
    pub aliases: HashMap<String, String>,
    pub variables: HashMap<String, String>,
    pub arrays: HashMap<String, Array>,
    pub attributes: HashMap<String, Attributes>,
    pub pending_declarations: Vec<Evaluated>,
    pub getopts_offset: usize,
    pub current_line: usize,
    pub running_hook: bool,
    pub source_stack: Vec<String>,
    pub function_stack: Vec<String>,
    pub abbreviations: std::collections::BTreeMap<String, String>,
    pub key_bindings: Vec<(String, KeyBinding)>,
    pub bindings_version: u64,
    pub completion_specs: std::collections::BTreeMap<String, CompletionSpec>,
    pub pipe_statuses: Vec<i32>,
    pub last_pipe_statuses: Vec<i32>,
    pub substitution_fds: Vec<i32>,
    pub substitution_pids: Vec<i32>,
    pub started: std::time::Instant,
    pub getopts_index: usize,
    pub traps: std::collections::BTreeMap<String, String>,
    pub condition_depth: u32,
    pub directory_stack: Vec<String>,
    pub history: Vec<String>,
    pub history_cleared: bool,
    pub options: ShellOptions,
    pub functions: HashMap<String, Rc<Command>>,
    pub positional: Vec<String>,
    pub arg0: String,
    pub last_status: i32,
    pub substitution_status: Option<i32>,
    pub last_background_pid: Option<i32>,
    pub shell_pid: i32,
    pub interactive: bool,
    pub agent: bool,
    pub explaining: bool,
    pub shell_pgid: i32,
    pub jobs: JobTable,
    pub flow: Flow,
    pub locals: Vec<HashMap<String, Saved>>,
    pub loop_depth: u32,
    pub source_depth: u32,
    pub path_commands: Option<(String, Vec<String>)>,
    pub directories: DirectoryHistory,
    pub pipe_to_table: bool,
}

impl Shell {
    pub fn new() -> Self {
        let mut shell = Shell {
            aliases: HashMap::from([
                ("ls".into(), "ls --color=auto".into()),
                ("grep".into(), "grep --color=auto".into()),
            ]),
            variables: HashMap::new(),
            arrays: HashMap::new(),
            attributes: HashMap::new(),
            pending_declarations: Vec::new(),
            getopts_offset: 0,
            current_line: 0,
            running_hook: false,
            source_stack: Vec::new(),
            function_stack: Vec::new(),
            abbreviations: std::collections::BTreeMap::new(),
            key_bindings: Vec::new(),
            bindings_version: 0,
            completion_specs: std::collections::BTreeMap::new(),
            pipe_statuses: Vec::new(),
            last_pipe_statuses: Vec::new(),
            substitution_fds: Vec::new(),
            substitution_pids: Vec::new(),
            started: std::time::Instant::now(),
            getopts_index: 1,
            traps: std::collections::BTreeMap::new(),
            condition_depth: 0,
            directory_stack: Vec::new(),
            history: Vec::new(),
            history_cleared: false,
            options: ShellOptions::default(),
            functions: HashMap::new(),
            positional: Vec::new(),
            arg0: "grfsh".into(),
            last_status: 0,
            substitution_status: None,
            last_background_pid: None,
            shell_pid: sys::getpid(),
            interactive: false,
            agent: false,
            explaining: false,
            shell_pgid: 0,
            jobs: JobTable::default(),
            flow: Flow::Normal,
            locals: Vec::new(),
            loop_depth: 0,
            source_depth: 0,
            path_commands: None,
            pipe_to_table: false,
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
                let interactive = if self.interactive { "i" } else { "" };
                return Some(format!("{}{interactive}", self.options.flags()));
            }
            "RANDOM" => return Some((random() % 32768).to_string()),
            "LINENO" => return Some(self.current_line.to_string()),
            "SECONDS" => return Some(self.started.elapsed().as_secs().to_string()),
            "PPID" => return Some(crate::sys::parent_pid().to_string()),
            _ => {}
        }
        if let Ok(index) = name.parse::<usize>() {
            return self.positional.get(index.wrapping_sub(1)).cloned();
        }
        if let Some(array) = self.arrays.get(name) {
            return array.first().cloned();
        }
        if let Some(values) = self.special_array(name) {
            return values.into_iter().next();
        }
        self.variables
            .get(name)
            .cloned()
            .or_else(|| std::env::var(name).ok())
    }

    pub fn snapshot(&self, name: &str) -> Saved {
        Saved {
            scalar: self.variables.get(name).cloned(),
            array: self.arrays.get(name).cloned(),
            attributes: self.attributes.get(name).copied(),
        }
    }

    pub fn restore(&mut self, name: String, saved: Saved) {
        match saved.scalar {
            Some(value) => self.variables.insert(name.clone(), value),
            None => self.variables.remove(&name),
        };
        match saved.array {
            Some(array) => self.arrays.insert(name.clone(), array),
            None => self.arrays.remove(&name),
        };
        match saved.attributes {
            Some(attributes) => self.attributes.insert(name, attributes),
            None => self.attributes.remove(&name),
        };
    }

    pub fn special_array(&self, name: &str) -> Option<Vec<String>> {
        match name {
            "BASH_SOURCE" => Some(self.source_stack.iter().rev().cloned().collect()),
            "PIPESTATUS" => Some(
                self.last_pipe_statuses
                    .iter()
                    .map(|status| status.to_string())
                    .collect(),
            ),
            "FUNCNAME" => {
                let mut names: Vec<String> = self.function_stack.iter().rev().cloned().collect();
                if !names.is_empty() && self.source_stack.is_empty() {
                    names.push("main".into());
                }
                Some(names)
            }
            _ => None,
        }
    }

    pub fn is_associative(&self, name: &str) -> bool {
        self.arrays.get(name).is_some_and(Array::is_associative)
    }

    pub fn is_readonly(&self, name: &str) -> bool {
        self.attributes
            .get(name)
            .is_some_and(|attributes| attributes.readonly)
    }

    fn check_writable(&self, name: &str) -> Result<(), String> {
        if self.is_readonly(name) {
            Err(format!("{name}: readonly variable"))
        } else {
            Ok(())
        }
    }

    pub fn transform_value(&mut self, name: &str, value: &str) -> Result<String, String> {
        let Some(attributes) = self.attributes.get(name).copied() else {
            return Ok(value.to_string());
        };
        let mut value = if attributes.integer {
            crate::expand::evaluate_arithmetic(self, value)?.to_string()
        } else {
            value.to_string()
        };
        if attributes.lower {
            value = value.to_lowercase();
        }
        if attributes.upper {
            value = value.to_uppercase();
        }
        Ok(value)
    }

    pub fn set_array(&mut self, name: &str, array: Array) -> Result<(), String> {
        self.check_writable(name)?;
        self.variables.remove(name);
        self.arrays.insert(name.to_string(), array);
        Ok(())
    }

    pub fn set_element(&mut self, name: &str, key: Key, value: &str) -> Result<(), String> {
        self.check_writable(name)?;
        let value = self.transform_value(name, value)?;
        if !self.arrays.contains_key(name) {
            let existing = self
                .variables
                .remove(name)
                .map_or_else(Vec::new, |value| vec![value]);
            self.arrays
                .insert(name.to_string(), Array::indexed(existing));
        }
        let array = self
            .arrays
            .get_mut(name)
            .ok_or_else(|| format!("{name}: not an array"))?;
        array
            .set(key, value)
            .map_err(|message| format!("{name}[{message}"))
    }

    pub fn try_set_variable(&mut self, name: &str, value: &str) -> Result<(), String> {
        self.check_writable(name)?;
        let value = self.transform_value(name, value)?;
        if let Some(array) = self.arrays.get_mut(name) {
            return array.set(Key::Index(0), value);
        }
        if !self.variables.contains_key(name) && std::env::var_os(name).is_some() {
            unsafe { std::env::set_var(name, value) };
        } else {
            self.variables.insert(name.into(), value);
        }
        Ok(())
    }

    pub fn set_variable(&mut self, name: &str, value: &str) {
        if let Err(message) = self.try_set_variable(name, value) {
            error(message);
        }
    }

    pub fn export_variable(&mut self, name: &str, value: Option<&str>) {
        let value = value
            .map(String::from)
            .or_else(|| self.variables.get(name).cloned())
            .or_else(|| std::env::var(name).ok());
        self.variables.remove(name);
        if let Some(value) = value {
            unsafe { std::env::set_var(name, value) };
        }
    }

    pub fn unset_variable(&mut self, name: &str) {
        if self.is_readonly(name) {
            error(format!("unset: {name}: readonly variable"));
            return;
        }
        self.attributes.remove(name);
        self.arrays.remove(name);
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

thread_local! {
    static LOCATION: std::cell::RefCell<Option<(String, usize)>> = const { std::cell::RefCell::new(None) };
}

pub fn set_location(script: Option<&str>, line: usize) {
    LOCATION.with(|location| {
        let mut location = location.borrow_mut();
        match (script, location.as_mut()) {
            (Some(name), _) => *location = Some((name.to_string(), line)),
            (None, Some(current)) => current.1 = line,
            (None, None) => {}
        }
    });
}

pub fn error(message: impl Display) {
    let prefix = LOCATION.with(|location| match location.borrow().as_ref() {
        Some((script, line)) if *line > 0 && script.is_empty() => format!("line {line}: "),
        Some((script, line)) if *line > 0 => format!("{script}: line {line}: "),
        Some((script, _)) if script.is_empty() => String::new(),
        Some((script, _)) => format!("{script}: "),
        None => String::new(),
    });
    if crate::text::colors_enabled() && std::io::stderr().is_terminal() {
        eprintln!("\x1b[1;31mgrfsh:\x1b[0m {prefix}{message}");
    } else {
        eprintln!("grfsh: {prefix}{message}");
    }
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

use std::collections::HashMap;
use std::rc::Rc;

use crate::builtins;
use crate::commands;
use crate::expand;
use crate::shell::{self, Flow, Shell};
use crate::syntax::ast::*;
use crate::syntax::parser;
use crate::sys;

mod process;
mod redirect;

const MAX_FUNCTION_DEPTH: usize = 1000;

impl Shell {
    pub(crate) fn run_source(&mut self, source: &str) -> i32 {
        match parser::parse_with_aliases(source, &self.aliases) {
            Ok(list) => self.run_list(&list),
            Err(error) => {
                shell::error(error);
                self.last_status = 2;
                2
            }
        }
    }

    pub(crate) fn run_lines(&mut self, mut next_line: impl FnMut() -> Option<String>) -> i32 {
        let mut buffer = String::new();
        while let Some(line) = next_line() {
            buffer.push_str(&line);
            if !buffer.ends_with('\n') {
                buffer.push('\n');
            }
            match parser::parse_with_aliases(&buffer, &self.aliases) {
                Err(error) if error.incomplete => continue,
                Err(error) => {
                    shell::error(error);
                    self.last_status = 2;
                }
                Ok(list) => {
                    self.run_list(&list);
                    self.reap_jobs();
                }
            }
            buffer.clear();
            match self.flow {
                Flow::Exit | Flow::Return => return self.last_status,
                _ => self.flow = Flow::Normal,
            }
        }
        if !buffer.trim().is_empty() {
            self.run_source(&buffer);
        }
        self.last_status
    }

    pub(crate) fn run_list(&mut self, list: &List) -> i32 {
        for item in list {
            if self.flow != Flow::Normal {
                break;
            }
            if item.background {
                self.run_background(item);
            } else {
                self.run_and_or(item);
            }
        }
        self.last_status
    }

    pub(crate) fn run_list_exec(&mut self, list: &List) -> i32 {
        if let [item] = list.as_slice()
            && !item.background
            && item.rest.is_empty()
            && !item.first.negate
            && let [command] = item.first.commands.as_slice()
        {
            return self.run_command(command, true);
        }
        self.run_list(list)
    }

    pub(crate) fn run_and_or(&mut self, item: &AndOr) -> i32 {
        let mut status = self.run_pipeline(&item.first, &item.text);
        for (connector, pipeline) in &item.rest {
            if self.flow != Flow::Normal {
                break;
            }
            let run = match connector {
                Connector::And => status == 0,
                Connector::Or => status != 0,
            };
            if run {
                status = self.run_pipeline(pipeline, &item.text);
            }
        }
        status
    }

    pub(crate) fn run_pipeline(&mut self, pipeline: &Pipeline, text: &str) -> i32 {
        let status = match pipeline.commands.as_slice() {
            [command] => self.run_command(command, false),
            commands => self.run_forked(commands, text),
        };
        let status = if pipeline.negate {
            (status == 0) as i32
        } else {
            status
        };
        self.last_status = status;
        status
    }

    pub(crate) fn run_command(&mut self, command: &Command, exec_in_place: bool) -> i32 {
        match &command.kind {
            CommandKind::Simple(simple) => {
                self.run_simple(simple, &command.redirects, exec_in_place)
            }
            CommandKind::FunctionDefinition { name, body } => {
                self.functions.insert(name.clone(), Rc::clone(body));
                0
            }
            kind => self.with_redirects(&command.redirects, |shell| shell.run_compound(kind)),
        }
    }

    pub(crate) fn run_compound(&mut self, kind: &CommandKind) -> i32 {
        match kind {
            CommandKind::Group(list) => self.run_list(list),
            CommandKind::Subshell(list) => {
                let group = Command {
                    kind: CommandKind::Group(list.clone()),
                    redirects: Vec::new(),
                };
                self.run_forked(std::slice::from_ref(&group), "( ... )")
            }
            CommandKind::If {
                branches,
                otherwise,
            } => {
                for (condition, body) in branches {
                    self.run_list(condition);
                    if self.flow != Flow::Normal {
                        return self.last_status;
                    }
                    if self.last_status == 0 {
                        return self.run_list(body);
                    }
                }
                match otherwise {
                    Some(body) => self.run_list(body),
                    None => 0,
                }
            }
            CommandKind::Loop {
                condition,
                body,
                until,
            } => {
                let mut status = 0;
                self.loop_depth += 1;
                loop {
                    self.run_list(condition);
                    if self.flow != Flow::Normal || (self.last_status == 0) == *until {
                        break;
                    }
                    status = self.run_list(body);
                    if self.loop_should_exit() {
                        break;
                    }
                }
                self.loop_depth -= 1;
                status
            }
            CommandKind::For {
                variable,
                items,
                body,
            } => {
                let values = match items {
                    Some(words) => match expand::words(self, words) {
                        Ok(value) => value,
                        Err(error) => {
                            shell::error(error);
                            return 1;
                        }
                    },
                    None => self.positional.clone(),
                };
                let mut status = 0;
                self.loop_depth += 1;
                for value in values {
                    self.set_variable(variable, &value);
                    status = self.run_list(body);
                    if self.loop_should_exit() {
                        break;
                    }
                }
                self.loop_depth -= 1;
                status
            }
            CommandKind::Simple(_) | CommandKind::FunctionDefinition { .. } => unreachable!(),
        }
    }

    pub(crate) fn loop_should_exit(&mut self) -> bool {
        match self.flow {
            Flow::Normal => false,
            Flow::Break(count) => {
                self.flow = if count > 1 {
                    Flow::Break(count - 1)
                } else {
                    Flow::Normal
                };
                true
            }
            Flow::Continue(count) if count > 1 => {
                self.flow = Flow::Continue(count - 1);
                true
            }
            Flow::Continue(_) => {
                self.flow = Flow::Normal;
                false
            }
            Flow::Return | Flow::Exit | Flow::Interrupted => true,
        }
    }

    pub(crate) fn run_simple(
        &mut self,
        simple: &SimpleCommand,
        redirects: &[Redirect],
        exec_in_place: bool,
    ) -> i32 {
        self.substitution_status = None;
        let mut argv = match expand::words(self, &simple.words) {
            Ok(arguments) => arguments,
            Err(error) => {
                shell::error(error);
                return 1;
            }
        };
        let mut assignments = Vec::with_capacity(simple.assignments.len());
        for (name, value) in &simple.assignments {
            match expand::single(self, value) {
                Ok(value) => assignments.push((name.clone(), value)),
                Err(error) => {
                    shell::error(error);
                    return 1;
                }
            }
        }

        if argv.is_empty() {
            for (name, value) in &assignments {
                self.set_variable(name, value);
            }
            let status = self.substitution_status.unwrap_or(0);
            return self.with_redirects(redirects, |_| status);
        }

        let mut bypass_functions = false;
        while argv.len() > 1 && argv[0] == "command" && argv[1] != "-v" {
            argv.remove(0);
            bypass_functions = true;
        }

        let name = argv[0].as_str();
        if !bypass_functions && let Some(body) = self.functions.get(name).cloned() {
            return self.with_redirects(redirects, |shell| {
                shell.with_temp_env(&assignments, |shell| shell.call_function(&body, &argv))
            });
        }
        if builtins::is_builtin(name) {
            return self.with_redirects(redirects, |shell| {
                shell.with_temp_env(&assignments, |shell| builtins::run(shell, &argv))
            });
        }

        let resolved = self.resolve(name);
        if self.interactive
            && argv.len() == 1
            && assignments.is_empty()
            && std::path::Path::new(name).is_dir()
            && (resolved.is_none() || name.contains('/'))
        {
            return builtins::change_directory(self, name);
        }
        let Some(path) = resolved else {
            self.command_not_found(name);
            return 127;
        };
        if exec_in_place {
            self.exec_external(&path, &argv, &assignments, redirects)
        }
        match sys::fork() {
            Ok(0) => {
                self.enter_child(self.interactive.then_some(0), true);
                self.exec_external(&path, &argv, &assignments, redirects)
            }
            Ok(pid) => {
                if self.interactive {
                    sys::setpgid(pid, pid);
                }
                self.wait_foreground(pid, vec![pid], &argv.join(" "))
            }
            Err(error) => {
                shell::error(format!("fork: {}", sys::error_message(&error)));
                1
            }
        }
    }

    pub(crate) fn command_not_found(&mut self, name: &str) {
        let suggestion = self.suggest_command(name);
        match suggestion {
            Some(candidate) => shell::error(format!(
                "{name}: command not found, did you mean '{candidate}'?"
            )),
            None => shell::error(format!("{name}: command not found")),
        }
    }

    pub(crate) fn path_command_names(&mut self) -> &[String] {
        let path = self.get_variable("PATH").unwrap_or_default();
        if self
            .path_commands
            .as_ref()
            .is_none_or(|(cached, _)| *cached != path)
        {
            let found = commands::on_path(&path);
            self.path_commands = Some((path, found));
        }
        self.path_commands
            .as_ref()
            .map_or(&[], |(_, names)| names.as_slice())
    }

    fn suggest_command(&mut self, name: &str) -> Option<String> {
        self.path_command_names();
        let on_path = self
            .path_commands
            .iter()
            .flat_map(|(_, names)| names.iter());
        let candidates = on_path
            .chain(self.functions.keys())
            .chain(self.aliases.keys())
            .map(String::as_str)
            .chain(builtins::names().iter().copied());
        commands::closest(name, candidates).map(String::from)
    }

    pub(crate) fn resolve(&self, name: &str) -> Option<String> {
        if name.contains('/') {
            return std::path::Path::new(name)
                .exists()
                .then(|| name.to_string());
        }
        let path = self.get_variable("PATH")?;
        path.split(':')
            .map(|directory| {
                if directory.is_empty() {
                    format!("./{name}")
                } else {
                    format!("{directory}/{name}")
                }
            })
            .find(|candidate| is_executable(candidate))
    }

    pub(crate) fn call_function(&mut self, body: &Rc<Command>, argv: &[String]) -> i32 {
        if self.locals.len() >= MAX_FUNCTION_DEPTH {
            shell::error(format!("{}: maximum function nesting exceeded", argv[0]));
            return 1;
        }
        let saved = std::mem::replace(&mut self.positional, argv[1..].to_vec());
        self.locals.push(HashMap::new());
        self.run_command(body, false);
        if self.flow == Flow::Return {
            self.flow = Flow::Normal;
        }
        for (name, previous) in self.locals.pop().unwrap_or_default() {
            match previous {
                Some(value) => self.variables.insert(name, value),
                None => self.variables.remove(&name),
            };
        }
        self.positional = saved;
        self.last_status
    }

    pub(crate) fn with_temp_env(
        &mut self,
        assignments: &[(String, String)],
        action: impl FnOnce(&mut Self) -> i32,
    ) -> i32 {
        if assignments.is_empty() {
            return action(self);
        }
        let saved: Vec<_> = assignments
            .iter()
            .map(|(count, _)| {
                (
                    count.clone(),
                    self.variables.remove(count),
                    std::env::var(count).ok(),
                )
            })
            .collect();
        for (name, value) in assignments {
            unsafe { std::env::set_var(name, value) };
        }
        let status = action(self);
        for (name, shell_value, env_value) in saved {
            match env_value {
                Some(value) => unsafe { std::env::set_var(&name, value) },
                None => unsafe { std::env::remove_var(&name) },
            }
            if let Some(value) = shell_value {
                self.variables.insert(name, value);
            }
        }
        status
    }
}

pub(crate) fn is_executable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

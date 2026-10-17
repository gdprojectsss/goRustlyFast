use std::collections::HashMap;
use std::rc::Rc;

use crate::builtins::{self, test};
use crate::commands;
use crate::expand;
use crate::shell::{self, Flow, Shell};
use crate::signals;
use crate::syntax::ast::*;
use crate::syntax::parser;
use crate::sys;
use crate::table;
use crate::variables::{Array, Evaluated, Key};

mod process;
mod redirect;

const MAX_FUNCTION_DEPTH: usize = 1000;

impl Shell {
    pub(crate) fn run_source(&mut self, source: &str) -> i32 {
        let first_line = self.current_line.max(1);
        match parser::parse_from_line(source, &self.aliases, first_line) {
            Ok(list) => self.run_list(&list),
            Err(error) => {
                self.report_parse_error(error);
                2
            }
        }
    }

    pub(crate) fn run_lines(&mut self, mut next_line: impl FnMut() -> Option<String>) -> i32 {
        let mut buffer = String::new();
        let mut lines_read = 0;
        let mut chunk_start = 1;
        while let Some(line) = next_line() {
            if buffer.is_empty() {
                chunk_start = lines_read + 1;
            }
            lines_read += 1;
            buffer.push_str(&line);
            if !buffer.ends_with('\n') {
                buffer.push('\n');
            }
            match parser::parse_from_line(&buffer, &self.aliases, chunk_start) {
                Err(error) if error.incomplete => continue,
                Err(error) => self.report_parse_error(error),
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
            match parser::parse_from_line(&buffer, &self.aliases, chunk_start) {
                Ok(list) => {
                    self.run_list(&list);
                }
                Err(error) => {
                    self.report_parse_error(error.at_line(chunk_start));
                }
            }
        }
        self.last_status
    }

    fn report_parse_error(&mut self, error: parser::ParseError) {
        match error.line {
            Some(line) if !self.interactive => {
                shell::set_location(None, line);
                shell::error(&error.message);
            }
            _ => shell::error(&error.message),
        }
        self.last_status = 2;
    }

    pub(crate) fn run_list(&mut self, list: &List) -> i32 {
        for item in list {
            if signals::any_pending() {
                self.handle_signals();
            }
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
        if item.line > 0 {
            self.current_line = item.line;
            if !self.interactive {
                shell::set_location(None, item.line);
            }
        }
        let last = item.rest.len();
        let mut status = self.run_guarded(&item.first, &item.text, last > 0);
        let mut ran_last = last == 0;
        let mut negated = item.first.negate;
        for (index, (connector, pipeline)) in item.rest.iter().enumerate() {
            if self.flow != Flow::Normal {
                break;
            }
            let run = match connector {
                Connector::And => status == 0,
                Connector::Or => status != 0,
            };
            if run {
                status = self.run_guarded(pipeline, &item.text, index + 1 < last);
                ran_last = index + 1 == last;
                negated = pipeline.negate;
            }
        }
        if status != 0
            && ran_last
            && !negated
            && self.condition_depth == 0
            && self.flow == Flow::Normal
        {
            self.command_failed(status);
        }
        status
    }

    fn run_guarded(&mut self, pipeline: &Pipeline, text: &str, guarded: bool) -> i32 {
        if guarded {
            self.condition_depth += 1;
        }
        let status = self.run_pipeline(pipeline, text);
        if guarded {
            self.condition_depth -= 1;
        }
        status
    }

    pub(crate) fn run_condition(&mut self, list: &List) -> i32 {
        self.condition_depth += 1;
        let status = self.run_list(list);
        self.condition_depth -= 1;
        status
    }

    fn command_failed(&mut self, status: i32) {
        if let Some(command) = self.traps.get("ERR").cloned()
            && !command.is_empty()
        {
            self.run_trap(&command);
        }
        if self.options.errexit {
            self.last_status = status;
            self.flow = Flow::Exit;
        }
    }

    fn run_trap(&mut self, command: &str) {
        let status = self.last_status;
        self.condition_depth += 1;
        self.run_source(command);
        self.condition_depth -= 1;
        if self.flow != Flow::Exit {
            self.last_status = status;
        }
    }

    pub(crate) fn handle_signals(&mut self) {
        if crate::sandbox::expired() {
            self.last_status = 124;
            self.flow = Flow::Interrupted;
        }
        if self.agent && crate::agent::timed_out() {
            self.last_status = 124;
            self.flow = Flow::Interrupted;
        }
        for signal in signals::take_pending() {
            let name = sys::signal_name(signal).unwrap_or_default();
            match self.traps.get(name).cloned() {
                Some(command) if !command.is_empty() => self.run_trap(&command),
                Some(_) => {}
                None if signal == libc::SIGHUP && self.interactive => {
                    self.hang_up_jobs();
                    self.last_status = 128 + libc::SIGHUP;
                    self.flow = Flow::Exit;
                }
                None if signal == libc::SIGINT && self.interactive => {
                    eprintln!();
                    self.last_status = 128 + libc::SIGINT;
                    self.flow = Flow::Interrupted;
                }
                None => {}
            }
        }
    }

    pub(crate) fn run_hook(&mut self, name: &str, args: &[String]) {
        if self.running_hook {
            return;
        }
        let mut functions: Vec<String> = Vec::new();
        if self.functions.contains_key(name) {
            functions.push(name.to_string());
        }
        if let Some(array) = self.arrays.get(&format!("{name}_functions")) {
            functions.extend(array.values());
        }
        if functions.is_empty() {
            return;
        }
        self.running_hook = true;
        let status = self.last_status;
        for function in functions {
            if let Some(body) = self.functions.get(&function).cloned() {
                let argv: Vec<String> = std::iter::once(function)
                    .chain(args.iter().cloned())
                    .collect();
                self.call_function(&body, &argv);
            }
        }
        if self.flow == Flow::Return {
            self.flow = Flow::Normal;
        }
        self.last_status = status;
        self.running_hook = false;
    }

    fn hang_up_jobs(&mut self) {
        for job in self.jobs.iter() {
            let _ = sys::kill(-job.pgid, libc::SIGHUP);
            let _ = sys::kill(-job.pgid, libc::SIGCONT);
        }
    }

    pub(crate) fn run_exit_trap(&mut self) {
        if let Some(command) = self.traps.remove("EXIT")
            && !command.is_empty()
        {
            let status = self.last_status;
            self.flow = Flow::Normal;
            self.run_source(&command);
            if self.flow != Flow::Exit {
                self.last_status = status;
            }
        }
    }

    pub(crate) fn run_pipeline(&mut self, pipeline: &Pipeline, text: &str) -> i32 {
        if pipeline.timed {
            let started = std::time::Instant::now();
            let (user_before, system_before) = sys::cpu_times();
            let untimed = Pipeline {
                timed: false,
                ..pipeline.clone()
            };
            let status = self.run_pipeline(&untimed, text);
            let (user_after, system_after) = sys::cpu_times();
            let format =
                |seconds: f64| format!("{}m{:.3}s", (seconds / 60.0) as u64, seconds % 60.0);
            eprintln!(
                "\nreal\t{}\nuser\t{}\nsys\t{}",
                format(started.elapsed().as_secs_f64()),
                format(user_after - user_before),
                format(system_after - system_before)
            );
            return status;
        }
        let status = match pipeline.commands.as_slice() {
            [command] => self.run_command(command, false),
            commands => self.run_forked(commands, text),
        };
        if pipeline.commands.len() == 1 {
            self.last_pipe_statuses.clear();
            self.last_pipe_statuses.push(status);
        } else {
            self.last_pipe_statuses = std::mem::take(&mut self.pipe_statuses);
        }
        let status = if pipeline.negate {
            (status == 0) as i32
        } else {
            status
        };
        self.last_status = status;
        status
    }

    pub(crate) fn run_command(&mut self, command: &Command, exec_in_place: bool) -> i32 {
        let mark = self.substitution_fds.len();
        let status = self.run_command_kind(command, exec_in_place);
        for fd in self.substitution_fds.drain(mark..) {
            sys::close(fd);
        }
        status
    }

    fn run_command_kind(&mut self, command: &Command, exec_in_place: bool) -> i32 {
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
                    self.run_condition(condition);
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
                    self.run_condition(condition);
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
            CommandKind::Arithmetic(expression) => match expand::arithmetic(self, expression) {
                Ok(value) => i32::from(value == 0),
                Err(error) => {
                    shell::error(error);
                    1
                }
            },
            CommandKind::ArithmeticFor {
                init,
                condition,
                step,
                body,
            } => {
                let evaluate = |shell: &mut Shell, word: &Word| -> Result<i64, String> {
                    if word.parts.is_empty() {
                        Ok(1)
                    } else {
                        expand::arithmetic(shell, word)
                    }
                };
                if let Err(error) = evaluate(self, init) {
                    shell::error(error);
                    return 1;
                }
                let mut status = 0;
                self.loop_depth += 1;
                loop {
                    match evaluate(self, condition) {
                        Ok(0) => break,
                        Ok(_) => {}
                        Err(error) => {
                            shell::error(error);
                            status = 1;
                            break;
                        }
                    }
                    status = self.run_list(body);
                    if self.loop_should_exit() {
                        break;
                    }
                    if let Err(error) = evaluate(self, step) {
                        shell::error(error);
                        status = 1;
                        break;
                    }
                }
                self.loop_depth -= 1;
                status
            }
            CommandKind::Conditional(condition) => match self.evaluate_condition(condition) {
                Ok(true) => 0,
                Ok(false) => 1,
                Err(error) => {
                    shell::error(format!("[[: {error}"));
                    2
                }
            },
            CommandKind::Case { subject, arms } => {
                let subject = match expand::single(self, subject) {
                    Ok(value) => value,
                    Err(error) => {
                        shell::error(error);
                        return 1;
                    }
                };
                let mut status = 0;
                let mut run_next = false;
                for arm in arms {
                    let mut matched = run_next;
                    for pattern in &arm.patterns {
                        if matched {
                            break;
                        }
                        let pattern = match expand::pattern(self, pattern) {
                            Ok(pattern) => pattern,
                            Err(error) => {
                                shell::error(error);
                                return 1;
                            }
                        };
                        matched = expand::glob::matches_pattern(&pattern, &subject);
                    }
                    run_next = false;
                    if !matched {
                        continue;
                    }
                    status = self.run_list(&arm.body);
                    if self.flow != Flow::Normal {
                        return status;
                    }
                    match arm.end {
                        CaseEnd::Stop => return status,
                        CaseEnd::FallThrough => run_next = true,
                        CaseEnd::TestNext => {}
                    }
                }
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

    fn trace(&self, assignments: &[(String, String)], argv: &[String]) {
        let prefix = self.get_variable("PS4").unwrap_or_else(|| "+ ".into());
        let words: Vec<String> = assignments
            .iter()
            .map(|(name, value)| format!("{name}={}", trace_quote(value)))
            .chain(argv.iter().map(|argument| trace_quote(argument)))
            .collect();
        eprintln!("{prefix}{}", words.join(" "));
    }

    fn expansion_failed(&mut self, error: String) -> i32 {
        shell::error(error);
        if !self.interactive {
            self.flow = Flow::Exit;
        }
        1
    }

    fn exec_builtin(
        &mut self,
        argv: &[String],
        assignments: &[(String, String)],
        redirects: &[Redirect],
    ) -> i32 {
        let Some(name) = argv.first() else {
            return match self.apply_redirects(redirects) {
                Ok(()) => 0,
                Err(error) => {
                    shell::error(format!("exec: {error}"));
                    1
                }
            };
        };
        let Some(path) = self.resolve(name) else {
            self.command_not_found(name);
            if !self.interactive {
                self.flow = Flow::Exit;
            }
            return 127;
        };
        sys::reset_child_signals();
        self.exec_external(&path, argv, assignments, redirects)
    }

    fn evaluate_condition(&mut self, condition: &Condition) -> Result<bool, String> {
        match condition {
            Condition::And(left, right) => {
                Ok(self.evaluate_condition(left)? && self.evaluate_condition(right)?)
            }
            Condition::Or(left, right) => {
                Ok(self.evaluate_condition(left)? || self.evaluate_condition(right)?)
            }
            Condition::Not(inner) => Ok(!self.evaluate_condition(inner)?),
            Condition::Single(word) => Ok(!expand::single(self, word)?.is_empty()),
            Condition::Unary { operator, operand } => {
                let operand = expand::single(self, operand)?;
                Ok(test::unary(self, operator, &operand))
            }
            Condition::Binary {
                operator,
                left,
                right,
            } => {
                let left = expand::single(self, left)?;
                match operator.as_str() {
                    "==" | "=" | "!=" => {
                        let pattern = expand::pattern(self, right)?;
                        let matched = expand::glob::matches_pattern(&pattern, &left);
                        Ok(matched == (operator != "!="))
                    }
                    "=~" => {
                        let pattern = expand::regex(self, right)?;
                        match test::regex_match(&left, &pattern)? {
                            Some(groups) => {
                                self.set_array("BASH_REMATCH", Array::indexed(groups))?;
                                Ok(true)
                            }
                            None => {
                                self.set_array("BASH_REMATCH", Array::indexed(Vec::new()))?;
                                Ok(false)
                            }
                        }
                    }
                    _ => {
                        let right = expand::single(self, right)?;
                        test::binary(operator, &left, &right)
                    }
                }
            }
        }
    }

    pub(crate) fn array_key(&mut self, name: &str, index: &Word) -> Result<Key, String> {
        if self.is_associative(name) {
            Ok(Key::Name(expand::single(self, index)?))
        } else {
            Ok(Key::Index(expand::arithmetic(self, index)?))
        }
    }

    fn evaluate_assignment(&mut self, assignment: &Assignment) -> Result<Evaluated, String> {
        match &assignment.value {
            AssignedValue::Scalar(word) => {
                let key = match &assignment.index {
                    Some(index) => Some(self.array_key(&assignment.name, index)?),
                    None => None,
                };
                Ok(Evaluated::Scalar {
                    name: assignment.name.clone(),
                    key,
                    append: assignment.append,
                    value: expand::single(self, word)?,
                })
            }
            AssignedValue::Array(words) => Ok(Evaluated::Array {
                name: assignment.name.clone(),
                append: assignment.append,
                elements: expand::words(self, words)?,
            }),
        }
    }

    pub(crate) fn assign(&mut self, evaluated: Evaluated) -> Result<(), String> {
        match evaluated {
            Evaluated::Scalar {
                name,
                key: Some(key),
                append,
                value,
            } => {
                let value = if append {
                    let current = self
                        .arrays
                        .get(&name)
                        .and_then(|array| array.get(&key))
                        .cloned()
                        .unwrap_or_default();
                    current + &value
                } else {
                    value
                };
                self.set_element(&name, key, &value)
            }
            Evaluated::Scalar {
                name,
                key: None,
                append,
                value,
            } => {
                let integer = self
                    .attributes
                    .get(&name)
                    .is_some_and(|attributes| attributes.integer);
                let value = match (append, integer) {
                    (true, true) => format!(
                        "{}+({value})",
                        self.get_variable(&name).unwrap_or_else(|| "0".into())
                    ),
                    (true, false) => self.get_variable(&name).unwrap_or_default() + &value,
                    (false, _) => value,
                };
                self.try_set_variable(&name, &value)
            }
            Evaluated::Array {
                name,
                append,
                elements,
            } => {
                let mut array = match self.arrays.get(&name) {
                    Some(existing) if append => existing.clone(),
                    Some(existing) if existing.is_associative() => Array::associative(),
                    _ if append => {
                        Array::indexed(self.variables.get(&name).cloned().into_iter().collect())
                    }
                    _ => Array::indexed(Vec::new()),
                };
                let elements = if self.attributes.contains_key(&name) {
                    elements
                        .into_iter()
                        .map(|element| match element.split_once("]=") {
                            Some((key, value)) if key.starts_with('[') => self
                                .transform_value(&name, value)
                                .map(|value| format!("{key}]={value}")),
                            _ => self.transform_value(&name, &element),
                        })
                        .collect::<Result<_, _>>()?
                } else {
                    elements
                };
                array
                    .extend(elements)
                    .map_err(|message| format!("{name}: {message}"))?;
                self.set_array(&name, array)
            }
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
            Err(error) => return self.expansion_failed(error),
        };
        let mut declarations = Vec::with_capacity(simple.declarations.len());
        for declaration in &simple.declarations {
            match self.evaluate_assignment(declaration) {
                Ok(evaluated) => declarations.push(evaluated),
                Err(error) => return self.expansion_failed(error),
            }
        }
        self.pending_declarations = declarations;
        let mut pending = Vec::with_capacity(simple.assignments.len());
        for assignment in &simple.assignments {
            match self.evaluate_assignment(assignment) {
                Ok(evaluated) => pending.push(evaluated),
                Err(error) => return self.expansion_failed(error),
            }
        }

        if argv.is_empty() {
            if self.options.xtrace {
                let traced: Vec<(String, String)> = pending
                    .iter()
                    .filter_map(|evaluated| match evaluated {
                        Evaluated::Scalar { name, value, .. } => {
                            Some((name.clone(), value.clone()))
                        }
                        Evaluated::Array { .. } => None,
                    })
                    .collect();
                self.trace(&traced, &[]);
            }
            for evaluated in pending {
                if let Err(error) = self.assign(evaluated) {
                    shell::error(error);
                    return 1;
                }
            }
            let status = self.substitution_status.unwrap_or(0);
            return self.with_redirects(redirects, |_| status);
        }
        let assignments: Vec<(String, String)> = pending
            .into_iter()
            .filter_map(|evaluated| match evaluated {
                Evaluated::Scalar {
                    name,
                    key: None,
                    value,
                    ..
                } => Some((name, value)),
                _ => None,
            })
            .collect();

        if self.options.xtrace {
            self.trace(&assignments, &argv);
        }

        let mut bypass_functions = false;
        while argv.len() > 1 && argv[0] == "command" && argv[1] != "-v" {
            argv.remove(0);
            bypass_functions = true;
        }

        if crate::sandbox::refuses(
            &argv,
            !bypass_functions && self.functions.contains_key(&argv[0]),
        ) {
            return 126;
        }
        if argv[0] == "exec" {
            return self.exec_builtin(&argv[1..], &assignments, redirects);
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
        if !bypass_functions && table::is_command(name) {
            return self.with_redirects(redirects, |shell| {
                shell.with_temp_env(&assignments, |shell| table::run(shell, &argv))
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
            .chain(builtins::names().iter().copied())
            .chain(table::names().iter().copied());
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
        self.function_stack.push(argv[0].clone());
        self.run_command(body, false);
        self.function_stack.pop();
        if self.flow == Flow::Return {
            self.flow = Flow::Normal;
        }
        for (name, previous) in self.locals.pop().unwrap_or_default() {
            self.restore(name, previous);
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

fn trace_quote(text: &str) -> String {
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_./=:@%+,".contains(character));
    if plain {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', "'\\''"))
    }
}

pub(crate) fn is_executable(path: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

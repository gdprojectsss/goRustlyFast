use std::collections::HashMap;
use std::rc::Rc;

use crate::builtins::{self, test, trap};
use crate::commands;
use crate::expand;
use crate::shell::{self, Flow, Shell};
use crate::signals;
use crate::syntax::ast::*;
use crate::syntax::parser;
use crate::sys;
use crate::table;
use crate::variables::{Array, Evaluated, Key, Saved};

mod assign;
mod coproc;
mod process;
mod redirect;
mod select;

pub(crate) use assign::subscript_close;

const MAX_FUNCTION_DEPTH: usize = 1000;
const SUBSHELL_TEXT: &str = "( ... )";

thread_local! {
    static PARSE_DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

struct NestedParse;

impl NestedParse {
    fn enter() -> Self {
        PARSE_DEPTH.with(|depth| depth.set(depth.get() + 1));
        NestedParse
    }

    fn outermost() -> bool {
        PARSE_DEPTH.with(|depth| depth.get() <= 1)
    }
}

impl Drop for NestedParse {
    fn drop(&mut self) {
        PARSE_DEPTH.with(|depth| depth.set(depth.get().saturating_sub(1)));
    }
}

impl Shell {
    pub(crate) fn run_source(&mut self, source: &str) -> i32 {
        let first_line = self.current_line.max(1);
        let mut parser = parser::Parser::for_text(source, first_line);
        self.run_parsed(&mut parser, false, None)
    }

    pub(crate) fn run_text(&mut self, text: &str) -> i32 {
        let mut parser = parser::Parser::for_text(text, 1);
        self.run_parsed(&mut parser, true, None)
    }

    pub(crate) fn run_lines<'a>(
        &mut self,
        mut next_line: impl FnMut() -> Option<String> + 'a,
    ) -> i32 {
        let verbose = Rc::new(std::cell::Cell::new(self.options.verbose));
        let echo = Rc::clone(&verbose);
        let reader = move || {
            let line = next_line()?;
            if echo.get() {
                eprintln!("{}", line.trim_end_matches('\n'));
            }
            Some(line)
        };
        let mut parser = parser::Parser::for_lines(Box::new(reader));
        self.run_parsed(&mut parser, true, Some(&verbose))
    }

    fn run_parsed(
        &mut self,
        parser: &mut parser::Parser<'_>,
        file: bool,
        verbose: Option<&std::cell::Cell<bool>>,
    ) -> i32 {
        let _nested = NestedParse::enter();
        if crate::stack::exhausted() {
            shell::error("maximum nesting depth exceeded");
            self.last_status = 2;
            return 2;
        }
        loop {
            if let Some(verbose) = verbose {
                verbose.set(self.options.verbose);
            }
            parser.set_aliases(self.active_aliases());
            let unit = parser.next_unit();
            for warning in parser.take_warnings() {
                self.report_parse_warning(warning);
            }
            match unit {
                Ok(None) => break,
                Ok(Some(_)) if self.options.noexec && !self.interactive => {}
                Ok(Some(list)) => {
                    self.run_list(&list);
                    if file {
                        self.reap_jobs();
                    }
                }
                Err(error) => {
                    self.report_parse_error(error);
                    return 2;
                }
            }
            match self.flow {
                Flow::Normal => {}
                Flow::Abort if NestedParse::outermost() => self.flow = Flow::Normal,
                Flow::Exit | Flow::Return | Flow::Abort => return self.last_status,
                _ if file && self.loop_depth == 0 => self.flow = Flow::Normal,
                _ => return self.last_status,
            }
        }
        self.last_status
    }

    fn report_parse_warning(&mut self, warning: parser::ParseError) {
        if let Some(line) = warning.line
            && !self.interactive
        {
            shell::set_location(None, line);
        }
        shell::error(&warning.message);
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

    pub(crate) fn run_list(&mut self, list: &[AndOr]) -> i32 {
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
        if signals::any_pending() && self.flow == Flow::Normal {
            self.handle_signals();
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
        if self.trap_depth == 0 && self.traps.contains_key("DEBUG") {
            self.set_variable("BASH_COMMAND", item.text.trim());
        }
        let last = item.rest.len();
        let mut status = self.run_guarded(&item.first, &item.text, last > 0 || item.first.negate);
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
                status =
                    self.run_guarded(pipeline, &item.text, index + 1 < last || pipeline.negate);
                ran_last = index + 1 == last;
                negated = pipeline.negate;
            }
        }
        if status != 0
            && ran_last
            && !negated
            && !self.compound_status
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
        self.run_named_trap("ERR");
        if self.options.errexit {
            self.last_status = status;
            self.flow = Flow::Exit;
        }
    }

    fn run_trap(&mut self, name: &str, command: &str) {
        let status = self.last_status;
        self.trap_depth += 1;
        self.active_traps.push(name.to_string());
        self.run_source(command);
        self.active_traps.pop();
        self.trap_depth -= 1;
        if self.flow != Flow::Exit {
            self.last_status = status;
        }
    }

    pub(crate) fn handle_signals(&mut self) -> Option<libc::c_int> {
        if crate::sandbox::expired() {
            self.last_status = 124;
            self.flow = Flow::Interrupted;
        }
        if self.agent && crate::agent::timed_out() {
            self.last_status = 124;
            self.flow = Flow::Interrupted;
        }
        let mut handled = None;
        for signal in signals::take_pending() {
            self.handle_signal(signal);
            if signal != libc::SIGCHLD || self.traps.contains_key("CHLD") {
                handled = Some(signal);
            }
        }
        handled
    }

    fn handle_signal(&mut self, signal: libc::c_int) {
        let name = sys::signal_name(signal).unwrap_or_default();
        match self.traps.get(name).cloned() {
            Some(command) if !command.is_empty() => self.run_signal_trap(name, &command),
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
            None if !self.interactive && trap::is_terminating(signal) => self.die_by_signal(signal),
            None => {}
        }
    }

    fn run_signal_trap(&mut self, name: &str, command: &str) {
        let line = self.current_line;
        self.current_line = 0;
        self.run_trap(name, command);
        self.current_line = line;
        if !self.interactive && line > 0 {
            shell::set_location(None, line);
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
        for job in self.jobs.iter().filter(|job| !job.keep_on_hangup) {
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
            return self.run_timed(pipeline, text);
        }
        let lastpipe = self.options.lastpipe && !self.interactive;
        if pipeline.commands.len() > 1 && !self.debug_before_pipeline(pipeline, lastpipe) {
            return self.last_status;
        }
        let status = match pipeline.commands.as_slice() {
            [command] => self.run_command(command, false),
            commands if lastpipe => self.run_lastpipe(commands, text),
            commands => self.run_forked(commands, text),
        };
        if pipeline.commands.len() > 1 {
            self.compound_status = false;
        }
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

    fn debug_before_pipeline(&mut self, pipeline: &Pipeline, lastpipe: bool) -> bool {
        if !self.traps.contains_key("DEBUG") {
            return true;
        }
        let forked = pipeline.commands.len() - usize::from(lastpipe);
        for (index, command) in pipeline.commands[..forked].iter().enumerate() {
            if !is_debug_point(command) {
                continue;
            }
            if let Some(line) = pipeline.lines.get(index) {
                self.current_line = *line;
            }
            if !self.run_debug_trap() {
                return false;
            }
        }
        true
    }

    fn run_timed(&mut self, pipeline: &Pipeline, text: &str) -> i32 {
        let mut untimed = Pipeline {
            timed: false,
            ..pipeline.clone()
        };
        let posix = take_posix_flag(&mut untimed);
        let started = std::time::Instant::now();
        let before = sys::process_times();
        let status = self.run_pipeline(&untimed, text);
        let after = sys::process_times();
        let real = started.elapsed().as_secs_f64();
        let user = after[0] - before[0] + after[2] - before[2];
        let system = after[1] - before[1] + after[3] - before[3];
        let format = if posix {
            "real %2R\nuser %2U\nsys %2S".to_string()
        } else {
            self.get_variable("TIMEFORMAT")
                .unwrap_or_else(|| "\nreal\t%3lR\nuser\t%3lU\nsys\t%3lS".into())
        };
        if !format.is_empty() {
            match format_times(&format, real, user, system) {
                Ok(report) => eprintln!("{report}"),
                Err(character) => shell::error(format!(
                    "TIMEFORMAT: `{character}': invalid format character"
                )),
            }
        }
        self.last_status = status;
        status
    }

    pub(crate) fn run_command(&mut self, command: &Command, exec_in_place: bool) -> i32 {
        let mark = self.substitution_fds.len();
        let status = self.run_command_kind(command, exec_in_place);
        for fd in self.substitution_fds.drain(mark..) {
            sys::close(fd);
        }
        self.collect_children();
        status
    }

    fn run_command_kind(&mut self, command: &Command, exec_in_place: bool) -> i32 {
        match &command.kind {
            CommandKind::Simple(simple) => {
                let status = self.run_simple(simple, &command.redirects, exec_in_place);
                self.compound_status = false;
                status
            }
            CommandKind::FunctionDefinition { name, body, source } => {
                self.define_function(name, body, source);
                self.compound_status = false;
                0
            }
            kind => {
                self.compound_status = false;
                self.with_redirects(&command.redirects, |shell| {
                    let status = shell.run_compound(kind);
                    shell.compound_status = !matches!(
                        kind,
                        CommandKind::Subshell(_)
                            | CommandKind::Arithmetic(_)
                            | CommandKind::Conditional(_)
                            | CommandKind::Coproc { .. }
                    );
                    status
                })
            }
        }
    }

    pub(crate) fn run_compound(&mut self, kind: &CommandKind) -> i32 {
        if crate::stack::exhausted() {
            shell::error("maximum nesting depth exceeded");
            self.flow = Flow::Abort;
            return 2;
        }
        match kind {
            CommandKind::Group(list) => self.run_list(list),
            CommandKind::Subshell(list) => {
                let group = Command {
                    kind: CommandKind::Group(list.clone()),
                    redirects: Vec::new(),
                };
                self.run_forked(std::slice::from_ref(&group), SUBSHELL_TEXT)
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
                    if self.flow != Flow::Normal {
                        status = self.last_status;
                        if self.loop_should_exit() {
                            break;
                        }
                        continue;
                    }
                    if (self.last_status == 0) == *until {
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
            CommandKind::Arithmetic(_) | CommandKind::Conditional(_) | CommandKind::Case { .. }
                if !self.run_debug_trap() =>
            {
                self.last_status
            }
            CommandKind::Arithmetic(expression) => match expand::arithmetic(self, expression) {
                Ok(value) => i32::from(value == 0),
                Err(error) => {
                    shell::error(format!("((: {error}"));
                    1
                }
            },
            CommandKind::ArithmeticFor {
                init,
                condition,
                step,
                body,
            } => {
                let line = self.current_line;
                let evaluate = |shell: &mut Shell, word: &Word| -> Result<i64, String> {
                    shell.current_line = line;
                    if shell.flow != Flow::Normal || !shell.run_debug_trap() {
                        return Ok(0);
                    }
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
                    Err(error) => return self.expansion_failed(error),
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
                        matched = expand::glob::matches_with(
                            &pattern,
                            &subject,
                            self.options.nocasematch,
                        );
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
                        Err(error) => return self.expansion_failed(error),
                    },
                    None => self.positional.clone(),
                };
                let mut status = 0;
                self.loop_depth += 1;
                let reference = self.is_reference(variable);
                let line = self.current_line;
                for value in values {
                    self.current_line = line;
                    if !self.run_debug_trap() {
                        status = self.last_status;
                        break;
                    }
                    if reference {
                        self.variables.insert(variable.clone(), value);
                    } else if let Err(error) = self.try_set_variable(variable, &value) {
                        shell::error(error);
                        status = 1;
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
            CommandKind::Select {
                variable,
                items,
                body,
            } => self.run_select(variable, items.as_deref(), body),
            CommandKind::Coproc { name, body } => self.run_coproc(name, body),
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
            Flow::Return | Flow::Exit | Flow::Interrupted | Flow::Abort => true,
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
        if error == expand::INTERRUPTED {
            return 128 + libc::SIGINT;
        }
        let unmatched_glob = error.starts_with(expand::NO_MATCH);
        shell::error(error);
        if unmatched_glob && self.options.errexit && !self.interactive {
            self.last_status = 1;
            self.flow = Flow::Exit;
        } else if unmatched_glob {
            self.flow = Flow::Interrupted;
        } else if !self.interactive && self.flow != Flow::Abort {
            self.flow = Flow::Exit;
            if self.command_string && sys::getpid() == self.shell_pid {
                return 127;
            }
        }
        1
    }

    fn exec_builtin(
        &mut self,
        argv: &[String],
        assignments: &[(String, String)],
        redirects: &[Redirect],
    ) -> i32 {
        let (options, argv) = match parse_exec_options(argv) {
            Ok(parsed) => parsed,
            Err(message) => {
                shell::error(message);
                return 2;
            }
        };
        let Some(name) = argv.first() else {
            return match self.apply_redirects(redirects) {
                Ok(()) => 0,
                Err(error) => self.redirect_failed(error),
            };
        };
        let Some(path) = self.resolve(name) else {
            let name = name.clone();
            let status = self.with_redirects(redirects, |_| {
                shell::error(format!("exec: {name}: not found"));
                127
            });
            if !self.interactive {
                self.flow = Flow::Exit;
            }
            return status;
        };
        if self.interactive {
            sys::reset_child_signals();
        }
        let mut argv = argv.to_vec();
        if let Some(name) = options.name {
            argv[0] = name;
        }
        if options.login {
            argv[0] = format!("-{}", argv[0]);
        }
        if options.clear_environment {
            for (name, _) in std::env::vars_os() {
                unsafe { std::env::remove_var(name) };
            }
        }
        self.exec_external(&path, &argv, assignments, redirects)
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
            Condition::Unary { operator, operand } if operator == "-v" => {
                let operand = expand::single(self, operand)?;
                Ok(self.is_set_with_subscript(&operand))
            }
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
                        let matched = if self.options.nocasematch {
                            expand::glob::matches_extended(
                                &pattern.to_lowercase(),
                                &left.to_lowercase(),
                            )
                        } else {
                            expand::glob::matches_extended(&pattern, &left)
                        };
                        Ok(matched == (operator != "!="))
                    }
                    "=~" => {
                        let pattern = expand::regex(self, right)?;
                        match test::regex_match_case(&left, &pattern, self.options.nocasematch)? {
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
                    "-eq" | "-ne" | "-lt" | "-le" | "-gt" | "-ge" => {
                        let right = expand::single(self, right)?;
                        let left = expand::evaluate_arithmetic(self, &left);
                        let right = expand::evaluate_arithmetic(self, &right);
                        match (left, right) {
                            (Ok(left), Ok(right)) => {
                                test::binary(operator, &left.to_string(), &right.to_string())
                            }
                            (Err(error), _) | (_, Err(error)) => {
                                shell::error(format!("[[: {error}"));
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

    fn is_set_with_subscript(&mut self, text: &str) -> bool {
        let Some((name, key)) = shell::split_subscript(text) else {
            return self.variable_is_set(text);
        };
        let name = self.resolve_name(name);
        if self.is_associative(&name) || key == "@" || key == "*" {
            return self.variable_is_set(&format!("{name}[{key}]"));
        }
        match expand::evaluate_arithmetic(self, key) {
            Ok(index) => self.variable_is_set(&format!("{name}[{index}]")),
            Err(_) => false,
        }
    }

    pub(crate) fn array_key(&mut self, name: &str, index: &Word) -> Result<Key, String> {
        let name = &self.resolve_name(name);
        if self.is_associative(name) {
            Ok(Key::Name(expand::single(self, index)?))
        } else {
            Ok(Key::Index(expand::arithmetic(self, index)?))
        }
    }

    pub(crate) fn evaluate_assignment(
        &mut self,
        assignment: &Assignment,
    ) -> Result<Evaluated, String> {
        let evaluated = self.evaluate_declaration(assignment)?;
        match evaluated {
            Evaluated::Scalar {
                name,
                subscript: Some(subscript),
                append,
                value,
                ..
            } => Ok(Evaluated::Scalar {
                key: Some(self.subscript_key(&name, &subscript)?),
                name,
                subscript: None,
                append,
                value,
            }),
            other => Ok(other),
        }
    }

    pub(crate) fn evaluate_declaration(
        &mut self,
        assignment: &Assignment,
    ) -> Result<Evaluated, String> {
        match &assignment.value {
            AssignedValue::Scalar(word) => {
                let subscript = match &assignment.index {
                    Some(index) => Some(expand::single(self, index)?),
                    None => None,
                };
                Ok(Evaluated::Scalar {
                    name: assignment.name.clone(),
                    key: None,
                    subscript,
                    append: assignment.append,
                    value: expand::single(self, &expand::assignment_tildes(word))?,
                })
            }
            AssignedValue::Array(words) => Ok(Evaluated::Array {
                name: assignment.name.clone(),
                append: assignment.append,
                elements: self.evaluate_elements(words)?,
            }),
        }
    }

    pub(crate) fn assign(&mut self, evaluated: Evaluated) -> Result<(), String> {
        match evaluated {
            Evaluated::Scalar {
                name,
                key: None,
                subscript: Some(subscript),
                append,
                value,
            } => {
                let key = self.subscript_key(&name, &subscript)?;
                self.assign(Evaluated::Scalar {
                    name,
                    key: Some(key),
                    subscript: None,
                    append,
                    value,
                })
            }
            Evaluated::Scalar {
                name,
                key: Some(key),
                append,
                value,
                ..
            } => {
                let name = self.resolve_name(&name);
                let value = if append {
                    let current = self
                        .arrays
                        .get(&name)
                        .and_then(|array| array.get(&key))
                        .cloned()
                        .or_else(|| {
                            (key == Key::Index(0) && !self.arrays.contains_key(&name))
                                .then(|| self.get_variable(&name))
                                .flatten()
                        })
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
                ..
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
            } => self.assign_elements(&name, append, elements),
        }
    }

    fn define_function(&mut self, name: &str, body: &Rc<Command>, source: &str) {
        self.functions.insert(name.to_string(), Rc::clone(body));
        let file = self.current_source();
        self.function_files.insert(name.to_string(), file);
        self.function_lines
            .insert(name.to_string(), self.current_line);
        self.function_sources
            .insert(name.to_string(), Rc::from(source));
        if self.is_function_exported(name) {
            self.export_function(name);
        }
    }

    fn assign_in_order(&mut self, assignments: &[Assignment]) -> i32 {
        for assignment in assignments {
            let evaluated = match self.evaluate_assignment(assignment) {
                Ok(evaluated) => evaluated,
                Err(error) if error.ends_with("bad array subscript") => {
                    shell::error(error);
                    self.flow = Flow::Abort;
                    return 1;
                }
                Err(error) => return self.expansion_failed(error),
            };
            if self.options.xtrace
                && let Evaluated::Scalar { name, value, .. } = &evaluated
            {
                self.trace(&[(name.clone(), value.clone())], &[]);
            }
            if let Err(error) = self.assign(evaluated) {
                shell::error(error);
                self.flow = Flow::Abort;
                return 1;
            }
        }
        self.substitution_status.unwrap_or(0)
    }

    fn evaluate_prefix(
        &mut self,
        assignments: &[Assignment],
    ) -> Result<Vec<(String, String)>, String> {
        let saved: Vec<(String, Saved)> = assignments
            .iter()
            .map(|assignment| (assignment.name.clone(), self.snapshot(&assignment.name)))
            .collect();
        let mut evaluated = Vec::with_capacity(assignments.len());
        let mut failure = None;
        for assignment in assignments {
            if let Some(index) = &assignment.index {
                shell::error(format!(
                    "`{}[{}]': not a valid identifier",
                    assignment.name,
                    builtins::deparse::word(index)
                ));
                continue;
            }
            let result = match &assignment.value {
                AssignedValue::Array(words) => Ok(Evaluated::Scalar {
                    name: assignment.name.clone(),
                    key: None,
                    subscript: None,
                    append: assignment.append,
                    value: format!(
                        "({})",
                        words
                            .iter()
                            .map(builtins::deparse::word)
                            .collect::<Vec<_>>()
                            .join(" ")
                    ),
                }),
                AssignedValue::Scalar(_) => self.evaluate_declaration(assignment),
            };
            match result {
                Ok(Evaluated::Scalar {
                    name,
                    append,
                    value,
                    ..
                }) => {
                    if self.is_readonly(&name) {
                        shell::error(format!("{name}: readonly variable"));
                        continue;
                    }
                    let value = if append {
                        self.get_variable(&name).unwrap_or_default() + &value
                    } else {
                        value
                    };
                    self.variables.remove(&name);
                    crate::variables::set_environment(&name, &value);
                    evaluated.push((name, value));
                }
                Ok(_) => {}
                Err(error) => {
                    failure = Some(error);
                    break;
                }
            }
        }
        for (name, previous) in saved.into_iter().rev() {
            self.restore(name, previous);
        }
        match failure {
            Some(error) => Err(error),
            None => Ok(evaluated),
        }
    }

    pub(crate) fn run_simple(
        &mut self,
        simple: &SimpleCommand,
        redirects: &[Redirect],
        exec_in_place: bool,
    ) -> i32 {
        self.substitution_status = None;
        if !self.run_debug_trap() {
            return self.last_status;
        }
        let mut argv = match expand::words(self, &simple.words) {
            Ok(arguments) => arguments,
            Err(error) => return self.expansion_failed(error),
        };
        let mut declarations = Vec::with_capacity(simple.declarations.len());
        for declaration in &simple.declarations {
            match self.evaluate_declaration(declaration) {
                Ok(evaluated) => declarations.push(evaluated),
                Err(error) => return self.expansion_failed(error),
            }
        }
        self.pending_declarations = declarations;

        if argv.is_empty() {
            let status = self.assign_in_order(&simple.assignments);
            if self.flow != Flow::Normal {
                return status;
            }
            return self.with_redirects(redirects, |_| status);
        }
        let assignments = match self.evaluate_prefix(&simple.assignments) {
            Ok(assignments) => assignments,
            Err(error) => return self.expansion_failed(error),
        };

        if self.options.xtrace {
            for assignment in &assignments {
                self.trace(std::slice::from_ref(assignment), &[]);
            }
            self.trace(&[], &argv);
        }

        let prefixes = match builtins::strip_prefixes(self, &mut argv) {
            Ok(prefixes) => prefixes,
            Err(message) => {
                return self.with_redirects(redirects, |_| {
                    shell::error(message);
                    1
                });
            }
        };
        if argv.is_empty() {
            return self.with_redirects(redirects, |_| 0);
        }
        let bypass_functions = prefixes.bypass_functions;

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
        if builtins::is_active(self, name) {
            return self.with_redirects(redirects, |shell| {
                shell.with_temp_env(&assignments, |shell| builtins::run(shell, &argv))
            });
        }
        if !bypass_functions && table::is_command(name) {
            return self.with_redirects(redirects, |shell| {
                shell.with_temp_env(&assignments, |shell| table::run(shell, &argv))
            });
        }

        let resolved = if prefixes.default_path {
            builtins::path_matches(name, builtins::DEFAULT_PATH, false)
                .into_iter()
                .next()
        } else {
            self.resolve(name)
        };
        if self.interactive
            && argv.len() == 1
            && assignments.is_empty()
            && crate::bytes::to_path(name).is_dir()
            && (resolved.is_none() || name.contains('/'))
        {
            return builtins::change_directory(self, name);
        }
        let Some(path) = resolved else {
            let name = name.to_string();
            return self.with_redirects(redirects, |shell| {
                shell.command_not_found(&name);
                127
            });
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
        if name.contains('/') {
            shell::error(format!("{name}: No such file or directory"));
            return;
        }
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
            return crate::bytes::to_path(name)
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
        if self.locals.len() >= MAX_FUNCTION_DEPTH || crate::stack::exhausted() {
            shell::error(format!("{}: maximum function nesting exceeded", argv[0]));
            return 1;
        }
        let saved = std::mem::replace(&mut self.positional, argv[1..].to_vec());
        self.locals.push(HashMap::new());
        self.function_stack.push(argv[0].clone());
        let line = self.current_line;
        let file = self
            .function_files
            .get(&argv[0])
            .cloned()
            .unwrap_or_else(|| self.current_source());
        self.push_frame(&argv[0], file, true);
        let traced = self.options.functrace || self.traced_functions.contains(&argv[0]);
        let hidden = self.hide_function_traps(traced);
        let loop_depth = std::mem::take(&mut self.loop_depth);
        if traced {
            self.run_named_trap("DEBUG");
        }
        let status = self.run_command(body, false);
        self.last_status = status;
        self.loop_depth = loop_depth;
        if self.flow == Flow::Return {
            self.flow = Flow::Normal;
        }
        self.run_named_trap("RETURN");
        self.restore_function_traps(hidden);
        self.call_stack.pop();
        self.current_line = line;
        if !self.interactive && line > 0 {
            shell::set_location(None, line);
        }
        self.function_stack.pop();
        for (name, previous) in self.locals.pop().unwrap_or_default() {
            self.restore(name, previous);
        }
        self.positional = saved;
        self.last_status
    }

    fn hide_function_traps(&mut self, traced: bool) -> Vec<(String, String)> {
        let mut hidden = Vec::new();
        for (name, inherited) in [
            ("ERR", self.options.errtrace),
            ("DEBUG", traced),
            ("RETURN", traced),
        ] {
            if !inherited && let Some(command) = self.traps.remove(name) {
                hidden.push((name.to_string(), command));
            }
        }
        hidden
    }

    fn restore_function_traps(&mut self, hidden: Vec<(String, String)>) {
        for (name, command) in hidden {
            self.traps.entry(name).or_insert(command);
        }
    }

    pub(crate) fn run_named_trap(&mut self, name: &str) {
        let blocked = if name == "RETURN" {
            self.trap_depth > 0
        } else {
            self.active_traps.iter().any(|active| active == name)
        };
        if blocked {
            return;
        }
        if let Some(command) = self.traps.get(name).cloned()
            && !command.is_empty()
        {
            self.run_trap(name, &command);
        }
    }

    pub(crate) fn run_debug_trap(&mut self) -> bool {
        if std::mem::take(&mut self.skip_debug_trap) {
            return true;
        }
        self.run_named_trap("DEBUG");
        self.flow == Flow::Normal
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
                    crate::bytes::env_var(count),
                )
            })
            .collect();
        for (name, value) in assignments {
            crate::variables::set_environment(name, value);
        }
        let status = action(self);
        for (name, shell_value, env_value) in saved {
            match env_value {
                Some(value) => crate::variables::set_environment(&name, &value),
                None => crate::variables::remove_environment(&name),
            }
            if let Some(value) = shell_value {
                self.variables.insert(name, value);
            }
        }
        status
    }
}

struct ExecOptions {
    name: Option<String>,
    login: bool,
    clear_environment: bool,
}

fn parse_exec_options(argv: &[String]) -> Result<(ExecOptions, &[String]), String> {
    let mut options = ExecOptions {
        name: None,
        login: false,
        clear_environment: false,
    };
    let mut index = 0;
    while let Some(argument) = argv.get(index) {
        if argument == "--" {
            index += 1;
            break;
        }
        let Some(flags) = argument.strip_prefix('-').filter(|flags| !flags.is_empty()) else {
            break;
        };
        index += 1;
        for (position, flag) in flags.char_indices() {
            match flag {
                'c' => options.clear_environment = true,
                'l' => options.login = true,
                'a' => {
                    let attached = &flags[position + 1..];
                    options.name = Some(if attached.is_empty() {
                        index += 1;
                        argv.get(index - 1)
                            .cloned()
                            .ok_or("exec: -a: option requires an argument")?
                    } else {
                        attached.to_string()
                    });
                    break;
                }
                other => return Err(format!("exec: -{other}: invalid option")),
            }
        }
    }
    Ok((options, &argv[index..]))
}

pub(crate) fn is_debug_point(command: &Command) -> bool {
    matches!(
        command.kind,
        CommandKind::Simple(_)
            | CommandKind::Arithmetic(_)
            | CommandKind::Conditional(_)
            | CommandKind::Case { .. }
    )
}

fn take_posix_flag(pipeline: &mut Pipeline) -> bool {
    let Some(Command {
        kind: CommandKind::Simple(simple),
        ..
    }) = pipeline.commands.first_mut()
    else {
        return false;
    };
    let posix = simple
        .words
        .first()
        .is_some_and(|word| word.as_literal() == Some("-p"));
    if posix {
        simple.words.remove(0);
    }
    posix
}

fn format_seconds(seconds: f64, precision: usize, long: bool) -> String {
    if long {
        let minutes = (seconds / 60.0) as u64;
        format!("{minutes}m{:.precision$}s", seconds - minutes as f64 * 60.0)
    } else {
        format!("{seconds:.precision$}")
    }
}

fn format_times(format: &str, real: f64, user: f64, system: f64) -> Result<String, char> {
    let mut out = String::new();
    let mut chars = format.chars().peekable();
    while let Some(character) = chars.next() {
        if character != '%' {
            out.push(character);
            continue;
        }
        let mut precision = None;
        if let Some(digit) = chars.peek().and_then(|next| next.to_digit(10)) {
            precision = Some((digit as usize).min(3));
            chars.next();
        }
        let long = chars.next_if_eq(&'l').is_some();
        match chars.next() {
            Some('%') => out.push('%'),
            Some('R') => out.push_str(&format_seconds(real, precision.unwrap_or(3), long)),
            Some('U') => out.push_str(&format_seconds(user, precision.unwrap_or(3), long)),
            Some('S') => out.push_str(&format_seconds(system, precision.unwrap_or(3), long)),
            Some('P') => {
                let percent = if real > 0.0 {
                    (user + system) / real * 100.0
                } else {
                    0.0
                };
                out.push_str(&format!("{percent:.*}", precision.unwrap_or(2)));
            }
            Some(other) => return Err(other),
            None => out.push('%'),
        }
    }
    Ok(out)
}

fn trace_quote(text: &str) -> String {
    if crate::escapes::needs_ansi_quote(text) {
        return crate::escapes::ansi_quote(text);
    }
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
    std::fs::metadata(crate::bytes::to_path(path))
        .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
}

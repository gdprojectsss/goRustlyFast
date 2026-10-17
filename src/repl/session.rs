use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use rustyline::config::Configurer;
use rustyline::error::ReadlineError;
use rustyline::history::FileHistory;
use rustyline::{
    Cmd, CompletionType, Config, EditMode, Editor, Event, EventHandler, KeyCode, KeyEvent,
    Modifiers,
};

use crate::shell::{self, Flow, KeyBinding, Shell};
use crate::signals;
use crate::sys;

use super::git::{self, REDRAW_SIGNAL};
use super::helper::{self, ShellHelper};
use super::prompt::{self, PromptState};
use super::{completion_names, config_dir, highlight, history, keys, log_command, theme};

const DEFAULT_HISTORY_SIZE: usize = 50_000;
const UNLIMITED_HISTORY_SIZE: usize = 10_000_000;

type LineEditor = Editor<ShellHelper, FileHistory>;

enum Input {
    Line(Result<String, ReadlineError>),
    Woken { drawn: bool },
}

enum Step {
    Continue,
    Redraw,
    Stop,
}

struct Wakeup {
    empty_input: i32,
    terminal: i32,
}

struct Session {
    editor: LineEditor,
    history_path: PathBuf,
    interrupted: Arc<AtomicBool>,
    last_arguments: Arc<Mutex<Vec<String>>>,
    wakeup: Option<Wakeup>,
    applied_bindings: u64,
    vi_mode: bool,
    warned_about_jobs: bool,
    duration: Duration,
    git_prompt: git::GitPrompt,
    themes: theme::ThemeCache,
}

fn history_size(shell: &Shell) -> usize {
    let configured = ["HISTFILESIZE", "HISTSIZE"]
        .iter()
        .filter_map(|name| shell.get_variable(name))
        .find(|value| !value.is_empty());
    match configured.map(|value| value.trim().parse::<i64>()) {
        None => DEFAULT_HISTORY_SIZE,
        Some(Ok(size)) if size < 0 => UNLIMITED_HISTORY_SIZE,
        Some(Ok(size)) => (size as usize).min(UNLIMITED_HISTORY_SIZE),
        Some(Err(_)) => DEFAULT_HISTORY_SIZE,
    }
}

fn history_file(shell: &Shell) -> PathBuf {
    shell
        .get_variable("HISTFILE")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(super::history_path)
}

fn new_editor(shell: &Shell) -> LineEditor {
    let mut builder = Config::builder().completion_type(CompletionType::List);
    if let Ok(sized) = builder.clone().max_history_size(history_size(shell)) {
        builder = sized;
    }
    if let Ok(ignoring) = builder.clone().history_ignore_dups(true) {
        builder = ignoring;
    }
    let mut editor = Editor::with_config(builder.build()).expect("failed to initialize editor");
    editor.set_helper(Some(ShellHelper::new()));
    editor
}

fn bind_editing_keys(
    editor: &mut LineEditor,
    interrupted: &Arc<AtomicBool>,
    arguments: &Arc<Mutex<Vec<String>>>,
) {
    let conditional =
        |handler: Box<dyn rustyline::ConditionalEventHandler>| EventHandler::Conditional(handler);
    editor.bind_sequence(
        Event::Any,
        conditional(Box::new(keys::WatchKeys {
            interrupted: Arc::clone(interrupted),
        })),
    );
    editor.bind_sequence(
        KeyEvent::ctrl('C'),
        conditional(Box::new(keys::InterruptLine {
            interrupted: Arc::clone(interrupted),
        })),
    );
    for key in [
        KeyEvent::ctrl('E'),
        KeyEvent(KeyCode::End, Modifiers::NONE),
        KeyEvent(KeyCode::Right, Modifiers::NONE),
    ] {
        editor.bind_sequence(
            key,
            conditional(Box::new(keys::AcceptSuggestion { whole: true })),
        );
    }
    editor.bind_sequence(
        KeyEvent::alt('f'),
        conditional(Box::new(keys::AcceptSuggestion { whole: false })),
    );
    editor.bind_sequence(
        KeyEvent::alt('.'),
        conditional(Box::new(keys::YankLastArgument::new(Arc::clone(arguments)))),
    );
}

fn apply_bindings(editor: &mut LineEditor, shell: &Shell) {
    for (spec, binding) in &shell.key_bindings {
        let Some(key) = keys::key(spec) else {
            continue;
        };
        let command = match binding {
            KeyBinding::Action(action) => keys::action(action),
            KeyBinding::Insert(text) => Some(Cmd::Insert(1, text.clone())),
        };
        if let Some(command) = command {
            editor.bind_sequence(key, EventHandler::Simple(command));
        }
    }
}

fn prepare_wakeup() -> Option<Wakeup> {
    let empty_input = sys::open("/dev/null", libc::O_RDONLY).ok()?;
    let Some(terminal) = sys::save_fd(0) else {
        sys::close(empty_input);
        return None;
    };
    signals::remember_main_thread();
    signals::catch_waking_prompt(libc::SIGHUP, true);
    signals::catch_waking_prompt(REDRAW_SIGNAL, false);
    Some(Wakeup {
        empty_input,
        terminal,
    })
}

fn update_window_size(shell: &mut Shell) {
    if let Some((columns, lines)) = sys::terminal_size(1).or_else(|| sys::terminal_size(0)) {
        let _ = shell.try_set_variable("COLUMNS", &columns.to_string());
        let _ = shell.try_set_variable("LINES", &lines.to_string());
    }
}

fn erase_prompt(prompt: &str, columns: usize) {
    let rows = theme::prompt_rows(prompt, columns);
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "\x1b[{rows}A\r\x1b[J");
    let _ = out.flush();
}

impl Session {
    fn start(shell: &mut Shell) -> Session {
        let mut editor = new_editor(shell);
        let interrupted = editor
            .helper()
            .map(ShellHelper::interrupted)
            .unwrap_or_default();
        let last_arguments = Arc::new(Mutex::new(Vec::new()));
        bind_editing_keys(&mut editor, &interrupted, &last_arguments);
        let history_path = history_file(shell);
        let _ = editor.load_history(&history_path);
        shell.history = editor
            .history()
            .iter()
            .map(|entry| entry.to_string())
            .collect();
        if let Ok(mut arguments) = last_arguments.lock() {
            arguments.extend(
                shell
                    .history
                    .iter()
                    .filter_map(|entry| history::last_argument(entry)),
            );
        }
        signals::catch_waking_prompt(libc::SIGTERM, false);
        Session {
            editor,
            history_path,
            interrupted,
            last_arguments,
            wakeup: prepare_wakeup(),
            applied_bindings: 0,
            vi_mode: false,
            warned_about_jobs: false,
            duration: Duration::ZERO,
            git_prompt: git::GitPrompt::default(),
            themes: theme::ThemeCache::default(),
        }
    }

    fn prepare(&mut self, shell: &mut Shell) {
        if shell.bindings_version != self.applied_bindings {
            apply_bindings(&mut self.editor, shell);
            self.applied_bindings = shell.bindings_version;
        }
        if shell.options.vi != self.vi_mode {
            self.vi_mode = shell.options.vi;
            self.editor.set_edit_mode(if self.vi_mode {
                EditMode::Vi
            } else {
                EditMode::Emacs
            });
        }
        if let Some(helper) = self.editor.helper_mut() {
            helper.update(completion_names(shell));
        }
        update_window_size(shell);
    }

    fn screen(&mut self, shell: &Shell, columns: usize) -> theme::Screen {
        let directory = std::env::current_dir()
            .map(|path| path.to_string_lossy().into_owned())
            .unwrap_or_default();
        let state = PromptState {
            git: self.git_prompt.info(&directory),
            status: shell.last_status,
            duration: self.duration,
            jobs: shell.jobs.len(),
        };
        let current_theme = self.themes.current(&theme::theme_path(&config_dir()));
        let screen = match &current_theme {
            Some(current_theme) => {
                let context = theme::PromptContext {
                    status: state.status,
                    duration: state.duration,
                    jobs: state.jobs,
                    git: state.git.as_ref(),
                    directory: &directory,
                };
                let rendered = match current_theme.engine {
                    theme::Engine::Starship => theme::starship(&context, columns)
                        .unwrap_or_else(|| theme::render(current_theme, &context)),
                    theme::Engine::Native => theme::render(current_theme, &context),
                };
                theme::layout(rendered, columns, current_theme.transient)
            }
            None => theme::Screen::plain(prompt::build(&state)),
        };
        theme::Screen {
            prompt: crate::text::without_colors(screen.prompt),
            right: screen.right.map(|right| helper::RightPrompt {
                colored: crate::text::without_colors(right.colored),
                ..right
            }),
            transient: screen.transient.map(crate::text::without_colors),
        }
    }

    fn arm_wakeup(&mut self) {
        let Some(wakeup) = &self.wakeup else {
            return;
        };
        signals::set_wake_input(libc::SIGHUP, Some(wakeup.empty_input));
        if self.git_prompt.waiting() {
            signals::set_wake_input(REDRAW_SIGNAL, Some(wakeup.empty_input));
        } else {
            self.git_prompt.stop_redraw();
        }
    }

    fn disarm_wakeup(&mut self) -> bool {
        signals::set_wake_input(libc::SIGHUP, None);
        signals::set_wake_input(REDRAW_SIGNAL, None);
        self.git_prompt.stop_redraw();
        let woken = signals::take_woken();
        if woken && let Some(wakeup) = &self.wakeup {
            let _ = sys::dup2(wakeup.terminal, 0);
        }
        woken
    }

    fn read(&mut self, prompt: &str) -> Input {
        self.interrupted.store(false, Ordering::SeqCst);
        self.arm_wakeup();
        if self.git_prompt.waiting() && self.git_prompt.arrived() {
            self.disarm_wakeup();
            return Input::Woken { drawn: false };
        }
        let line = self.editor.readline(prompt);
        if self.disarm_wakeup() {
            return Input::Woken { drawn: true };
        }
        Input::Line(line)
    }

    fn run(&mut self, shell: &mut Shell) {
        let mut redraw = false;
        loop {
            if !redraw {
                shell.reap_jobs();
                shell.run_hook("precmd", &[]);
            }
            self.prepare(shell);
            let columns = sys::terminal_width(1).unwrap_or(80);
            let mut screen = self.screen(shell, columns);
            if let Some(helper) = self.editor.helper_mut() {
                helper.set_right_prompt(screen.right.take());
            }
            let terminal = sys::terminal_state(0);
            let step = match self.read(&screen.prompt) {
                Input::Woken { drawn } => {
                    self.woken(shell, &screen.prompt, columns, terminal, drawn)
                }
                Input::Line(line) => self.handle(shell, line, &screen, columns),
            };
            redraw = false;
            match step {
                Step::Continue => {}
                Step::Redraw => redraw = true,
                Step::Stop => break,
            }
        }
    }

    fn woken(
        &mut self,
        shell: &mut Shell,
        prompt: &str,
        columns: usize,
        terminal: Option<libc::termios>,
        drawn: bool,
    ) -> Step {
        if let Some(state) = &terminal {
            sys::restore_terminal(0, state);
        }
        if signals::is_pending(libc::SIGHUP) {
            shell.handle_signals();
            if shell.flow == Flow::Exit {
                return Step::Stop;
            }
        }
        if drawn {
            erase_prompt(prompt, columns);
        }
        Step::Redraw
    }

    fn handle(
        &mut self,
        shell: &mut Shell,
        line: Result<String, ReadlineError>,
        screen: &theme::Screen,
        columns: usize,
    ) -> Step {
        match line {
            Ok(_) if self.interrupted.swap(false, Ordering::SeqCst) => {
                self.duration = Duration::ZERO;
                shell.last_status = 128 + libc::SIGINT;
                Step::Continue
            }
            Ok(text) => {
                if let Some(transient) = &screen.transient {
                    let highlighted = self
                        .editor
                        .helper()
                        .map(|helper| highlight::highlight(&text, |name| helper.is_command(name)))
                        .unwrap_or_else(|| text.clone());
                    theme::collapse(&screen.prompt, &text, transient, &highlighted, columns);
                }
                self.accept(shell, text)
            }
            Err(ReadlineError::Interrupted) => {
                self.duration = Duration::ZERO;
                Step::Continue
            }
            Err(ReadlineError::Eof) => self.end_of_input(shell),
            Err(error) => {
                if signals::any_pending() {
                    shell.handle_signals();
                    if shell.flow == Flow::Exit {
                        return Step::Stop;
                    }
                    return Step::Continue;
                }
                shell::error(error);
                Step::Stop
            }
        }
    }

    fn end_of_input(&mut self, shell: &mut Shell) -> Step {
        eprintln!("exit");
        if shell.jobs.len() > 0 && !self.warned_about_jobs {
            self.warned_about_jobs = true;
            shell::error("there are unfinished jobs; exit again to leave anyway");
            return Step::Continue;
        }
        Step::Stop
    }

    fn record(&mut self, shell: &mut Shell, entry: &str) {
        let entry = entry.trim();
        if entry.is_empty() || !self.editor.add_history_entry(entry).unwrap_or(false) {
            return;
        }
        shell.history.push(entry.to_string());
        if let Some(argument) = history::last_argument(entry)
            && let Ok(mut arguments) = self.last_arguments.lock()
        {
            arguments.push(argument);
        }
        let _ = self.editor.append_history(&self.history_path);
        sys::make_private(&self.history_path);
    }

    fn sync_history(&mut self, shell: &mut Shell, known: usize) {
        if shell.history_cleared {
            let _ = self.editor.clear_history();
            if let Ok(mut arguments) = self.last_arguments.lock() {
                arguments.clear();
            }
            shell.history_cleared = false;
            return;
        }
        if shell.history.len() <= known {
            return;
        }
        let added: Vec<String> = shell.history.drain(known..).collect();
        for entry in added {
            self.record(shell, &entry);
        }
    }

    fn accept(&mut self, shell: &mut Shell, line: String) -> Step {
        if line.trim().is_empty() {
            self.duration = Duration::ZERO;
            return Step::Continue;
        }
        let line = match keys::expand_first_word(&line, &shell.abbreviations) {
            Some(expanded) => {
                eprintln!("{expanded}");
                expanded
            }
            None => line,
        };
        let line = match history::expand(&line, &shell.history) {
            Ok(Some(expansion)) => {
                eprintln!("{}", expansion.text);
                if expansion.print_only {
                    self.record(shell, &expansion.text);
                    return Step::Continue;
                }
                expansion.text
            }
            Ok(None) => line,
            Err(error) => {
                shell::error(error);
                shell.last_status = 1;
                return Step::Continue;
            }
        };
        self.record(shell, &line);
        let known = shell.history.len();
        signals::discard(libc::SIGINT);
        shell.run_hook("preexec", std::slice::from_ref(&line));
        update_window_size(shell);
        let terminal = sys::terminal_state(0);
        let started = Instant::now();
        let directory = std::env::var("PWD").unwrap_or_default();
        shell.run_source(&line);
        self.duration = started.elapsed();
        if shell.last_status > 128
            && let Some(state) = &terminal
        {
            sys::restore_terminal(0, state);
        }
        self.git_prompt.invalidate();
        log_command(&line, &directory, self.duration, shell.last_status);
        if shell.flow == Flow::Interrupted && shell.last_status != 128 + libc::SIGTSTP {
            shell.last_status = 128 + libc::SIGINT;
        }
        if shell.flow == Flow::Exit {
            if shell.jobs.len() > 0 && !self.warned_about_jobs {
                self.warned_about_jobs = true;
                shell.flow = Flow::Normal;
                shell::error("there are unfinished jobs; exit again to leave anyway");
                return Step::Continue;
            }
            return Step::Stop;
        }
        self.sync_history(shell, known);
        self.warned_about_jobs = false;
        shell.flow = Flow::Normal;
        Step::Continue
    }

    fn finish(&mut self) {
        let _ = self.editor.append_history(&self.history_path);
        sys::make_private(&self.history_path);
    }
}

pub(super) fn interactive(shell: &mut Shell) {
    let mut session = Session::start(shell);
    session.run(shell);
    session.finish();
}

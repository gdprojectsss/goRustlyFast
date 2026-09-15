use std::io::Write;

use crate::repl::keys;
use crate::shell::{self, CompletionSpec, KeyBinding, Shell};

fn quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

pub(super) fn abbr(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    match args.first().map(String::as_str) {
        None | Some("-s") | Some("--show") => {
            for (name, expansion) in &shell.abbreviations {
                let _ = writeln!(out, "abbr {name} {}", quote(expansion));
            }
            0
        }
        Some("-l") | Some("--list") => {
            for name in shell.abbreviations.keys() {
                let _ = writeln!(out, "{name}");
            }
            0
        }
        Some("-e") | Some("--erase") => {
            let mut status = 0;
            for name in &args[1..] {
                if shell.abbreviations.remove(name).is_none() {
                    shell::error(format!("abbr: {name}: no such abbreviation"));
                    status = 1;
                }
            }
            status
        }
        Some(name) => {
            if args.len() < 2 || name.contains(char::is_whitespace) {
                shell::error("abbr: usage: abbr name expansion...");
                return 2;
            }
            shell
                .abbreviations
                .insert(name.to_string(), args[1..].join(" "));
            0
        }
    }
}

pub(super) fn bindkey(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    match args {
        [] => {
            for (spec, binding) in &shell.key_bindings {
                let line = match binding {
                    KeyBinding::Action(action) => format!("bindkey {} {action}", quote(spec)),
                    KeyBinding::Insert(text) => {
                        format!("bindkey -s {} {}", quote(spec), quote(text))
                    }
                };
                let _ = writeln!(out, "{line}");
            }
            0
        }
        [flag] if flag == "-l" => {
            for action in keys::ACTIONS {
                let _ = writeln!(out, "{action}");
            }
            0
        }
        [flag, spec, text] if flag == "-s" => bind(shell, spec, KeyBinding::Insert(text.clone())),
        [spec, action] => {
            if keys::action(action).is_none() {
                shell::error(format!("bindkey: {action}: unknown action, see bindkey -l"));
                return 1;
            }
            bind(shell, spec, KeyBinding::Action(action.clone()))
        }
        _ => {
            shell::error("bindkey: usage: bindkey [-l] | bindkey key action | bindkey -s key text");
            2
        }
    }
}

fn bind(shell: &mut Shell, spec: &str, binding: KeyBinding) -> i32 {
    if keys::key(spec).is_none() {
        shell::error(format!(
            "bindkey: {spec}: unknown key, use forms like ^G, \\ef, up or F5"
        ));
        return 1;
    }
    shell.key_bindings.retain(|(existing, _)| existing != spec);
    shell.key_bindings.push((spec.to_string(), binding));
    shell.bindings_version += 1;
    0
}

pub(super) fn complete(shell: &mut Shell, args: &[String], out: &mut dyn Write) -> i32 {
    if args.is_empty() || args == ["-p"] {
        for (command, spec) in &shell.completion_specs {
            let mut line = String::from("complete");
            if !spec.words.is_empty() {
                line.push_str(&format!(" -W {}", quote(&spec.words.join(" "))));
            }
            if let Some(source) = &spec.command {
                line.push_str(&format!(" -C {}", quote(source)));
            }
            if spec.directories {
                line.push_str(" -d");
            }
            if spec.files {
                line.push_str(" -f");
            }
            let _ = writeln!(out, "{line} {command}");
        }
        return 0;
    }

    let mut spec = CompletionSpec::default();
    let mut commands = Vec::new();
    let mut remove = false;
    let mut index = 0;
    while index < args.len() {
        let argument = args[index].as_str();
        index += 1;
        let mut value = || {
            let value = args.get(index).cloned();
            index += 1;
            value
        };
        match argument {
            "-W" | "-a" => match value() {
                Some(words) => spec
                    .words
                    .extend(words.split_whitespace().map(String::from)),
                None => return usage(),
            },
            "-C" => match value() {
                Some(source) => spec.command = Some(source),
                None => return usage(),
            },
            "-c" => match value() {
                Some(command) => commands.push(command),
                None => return usage(),
            },
            "-d" => spec.directories = true,
            "-f" => spec.files = true,
            "-r" => remove = true,
            "-F" => {
                shell::error("complete: -F is not supported, use -C 'command' to generate words");
                return 1;
            }
            flag if flag.starts_with('-') => {
                shell::error(format!("complete: {flag}: invalid option"));
                return 2;
            }
            command => commands.push(command.to_string()),
        }
    }
    if commands.is_empty() {
        return usage();
    }
    for command in commands {
        if remove {
            shell.completion_specs.remove(&command);
        } else {
            shell.completion_specs.insert(command, spec.clone());
        }
    }
    0
}

fn usage() -> i32 {
    shell::error("complete: usage: complete [-W words] [-C command] [-d] [-f] [-r] name...");
    2
}

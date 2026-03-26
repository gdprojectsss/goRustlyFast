use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::process::{Child, Command, Stdio};

use crate::builtins;
use crate::parser::{CommandLine, Connector, Pipeline};
use crate::shell;

pub fn execute(cmdline: &CommandLine, aliases: &mut HashMap<String, String>) -> i32 {
    let mut status = execute_pipeline(&cmdline.first, aliases);
    for (connector, pipeline) in &cmdline.rest {
        match connector {
            Connector::And if status != 0 => {}
            Connector::Or if status == 0 => {}
            _ => status = execute_pipeline(pipeline, aliases),
        }
    }
    status
}

fn execute_pipeline(pipeline: &Pipeline, aliases: &mut HashMap<String, String>) -> i32 {
    if pipeline.commands.len() == 1 {
        let cmd = &pipeline.commands[0];
        if builtins::is_builtin(&cmd.program) {
            let mut out: Box<dyn Write> = match cmd.stdout_file {
                Some(ref file) => match open_stdout(file, cmd.append) {
                    Ok(f) => Box::new(f),
                    Err(e) => {
                        shell::error(e);
                        return 1;
                    }
                },
                None => Box::new(std::io::stdout()),
            };
            return builtins::run(cmd, &mut *out, aliases);
        }
        return run_external(cmd);
    }
    run_pipeline(pipeline)
}

fn open_stdout(file: &str, append: bool) -> Result<File, String> {
    if append {
        OpenOptions::new()
            .append(true)
            .create(true)
            .open(file)
    } else {
        File::create(file)
    }
    .map_err(|e| format!("{}: {}", file, e))
}

fn run_external(cmd: &crate::parser::Command) -> i32 {
    let mut proc = Command::new(&cmd.program);
    proc.args(&cmd.args);

    if let Some(ref file) = cmd.stdin_file {
        match File::open(file) {
            Ok(f) => { proc.stdin(Stdio::from(f)); }
            Err(e) => {
                shell::error(format!("{}: {}", file, e));
                return 1;
            }
        }
    }

    if let Some(ref file) = cmd.stdout_file {
        match open_stdout(file, cmd.append) {
            Ok(f) => { proc.stdout(Stdio::from(f)); }
            Err(e) => {
                shell::error(e);
                return 1;
            }
        }
    }

    match proc.spawn() {
        Ok(mut child) => wait_child(&mut child),
        Err(e) => {
            shell::error(format!("{}: {}", cmd.program, e));
            127
        }
    }
}

fn run_pipeline(pipeline: &Pipeline) -> i32 {
    let mut children: Vec<Child> = Vec::new();
    let mut prev_stdout: Option<std::process::ChildStdout> = None;
    let last = pipeline.commands.len() - 1;

    for (i, cmd) in pipeline.commands.iter().enumerate() {
        let mut proc = Command::new(&cmd.program);
        proc.args(&cmd.args);

        if let Some(stdout) = prev_stdout.take() {
            proc.stdin(Stdio::from(stdout));
        } else if let Some(ref file) = cmd.stdin_file {
            match File::open(file) {
                Ok(f) => { proc.stdin(Stdio::from(f)); }
                Err(e) => {
                    shell::error(format!("{}: {}", file, e));
                    return 1;
                }
            }
        }

        if i < last {
            proc.stdout(Stdio::piped());
        } else if let Some(ref file) = cmd.stdout_file {
            match open_stdout(file, cmd.append) {
                Ok(f) => { proc.stdout(Stdio::from(f)); }
                Err(e) => {
                    shell::error(e);
                    return 1;
                }
            }
        }

        match proc.spawn() {
            Ok(mut child) => {
                if i < last {
                    prev_stdout = child.stdout.take();
                }
                children.push(child);
            }
            Err(e) => {
                shell::error(format!("{}: {}", cmd.program, e));
                for mut c in children {
                    let _ = c.wait();
                }
                return 127;
            }
        }
    }

    let mut status = 0;
    for mut child in children {
        status = wait_child(&mut child);
    }
    status
}

fn wait_child(child: &mut Child) -> i32 {
    match child.wait() {
        Ok(s) => s.code().unwrap_or(1),
        Err(e) => {
            shell::error(e);
            1
        }
    }
}

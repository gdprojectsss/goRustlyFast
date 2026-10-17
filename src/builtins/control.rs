use crate::shell::{self, Flow, Shell};

use super::parse_count;

pub(super) fn exit(shell: &mut Shell, args: &[String]) -> i32 {
    let code = match parse_count("exit", args, shell.last_status) {
        Ok(code) => code,
        Err(code) => code,
    };
    shell.flow = Flow::Exit;
    code & 0xff
}

pub(super) fn return_from(shell: &mut Shell, args: &[String]) -> i32 {
    if shell.locals.is_empty() && shell.source_depth == 0 {
        shell::error("return: can only be used in a function or sourced script");
        return 1;
    }
    let code = parse_count("return", args, shell.last_status).unwrap_or(2);
    shell.flow = Flow::Return;
    code & 0xff
}

pub(super) fn loop_control(shell: &mut Shell, command: &str, args: &[String]) -> i32 {
    if shell.loop_depth == 0 {
        shell::error(format!("{command}: only meaningful in a loop"));
        return 1;
    }
    let count = match parse_count(command, args, 1) {
        Ok(count) if count >= 1 => count as u32,
        Ok(_) => {
            shell::error(format!("{command}: loop count out of range"));
            return 1;
        }
        Err(code) => return code,
    };
    shell.flow = if command == "break" {
        Flow::Break(count)
    } else {
        Flow::Continue(count)
    };
    0
}

pub(super) fn let_command(shell: &mut Shell, args: &[String]) -> i32 {
    if args.is_empty() {
        shell::error("let: expression expected");
        return 1;
    }
    let mut last = 0;
    for expression in args {
        match crate::expand::evaluate_arithmetic(shell, expression) {
            Ok(value) => last = value,
            Err(message) => {
                shell::error(format!("let: {message}"));
                return 1;
            }
        }
    }
    i32::from(last == 0)
}

pub(super) fn caller(shell: &Shell, args: &[String], out: &mut dyn std::io::Write) -> i32 {
    let frames = shell.frames();
    let source = |index: usize| {
        frames
            .get(index)
            .map_or_else(|| "NULL".to_string(), |frame| frame.source.clone())
    };
    let Some(argument) = args.first() else {
        let Some(frame) = frames.first() else {
            return 1;
        };
        let _ = writeln!(out, "{} {}", frame.line, source(1));
        return 0;
    };
    let Ok(frame) = argument.parse::<usize>() else {
        let problem = if argument.starts_with('-') {
            "invalid option"
        } else {
            "invalid number"
        };
        shell::error(format!("caller: {argument}: {problem}"));
        eprintln!("caller: usage: caller [expr]");
        return 2;
    };
    match (frames.get(frame), frames.get(frame + 1)) {
        (Some(current), Some(calling)) => {
            let _ = writeln!(out, "{} {} {}", current.line, calling.name, calling.source);
            0
        }
        _ => 1,
    }
}

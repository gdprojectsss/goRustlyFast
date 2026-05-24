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

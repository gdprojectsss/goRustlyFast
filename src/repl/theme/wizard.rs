use std::io::Write;
use std::path::Path;
use std::time::Duration;

use crate::repl::git::GitInfo;
use crate::shell::{self, Shell};
use crate::sys;

use super::render::render;
use super::segments::PromptContext;
use super::{Icons, PRESETS, Style, Theme, theme_path};

fn ask(question: &str, choices: &[&str], default: usize) -> Option<usize> {
    loop {
        print!("{question} ");
        let _ = std::io::stdout().flush();
        let answer = sys::read_line(0)?;
        let answer = answer.trim().to_ascii_lowercase();
        if answer.is_empty() {
            return Some(default);
        }
        if let Some(index) = choices.iter().position(|choice| *choice == answer) {
            return Some(index);
        }
        println!("  Please answer with one of: {}", choices.join(", "));
    }
}

fn sample_git() -> GitInfo {
    GitInfo {
        branch: "main".into(),
        dirty: true,
        staged: 1,
        unstaged: 2,
        untracked: 0,
        ahead: 1,
        behind: 0,
    }
}

fn preview(theme: &Theme) -> String {
    let git = sample_git();
    let home = crate::bytes::env_var("HOME").unwrap_or_default();
    let directory = format!("{home}/projects/grfsh");
    let context = PromptContext {
        status: 0,
        duration: Duration::from_millis(3200),
        jobs: 1,
        git: Some(&git),
        directory: &directory,
    };
    let rendered = render(theme, &context);
    let columns = sys::terminal_width(1).unwrap_or(80).min(100);
    let last_line_width =
        crate::text::display_width(rendered.left.rsplit('\n').next().unwrap_or(""));
    let padding = columns.saturating_sub(last_line_width + rendered.right_width + 12);
    let mut text = String::new();
    for (index, line) in rendered.left.split('\n').enumerate() {
        if index > 0 {
            text.push('\n');
        }
        text.push_str("    ");
        text.push_str(line);
    }
    format!("{text}git status{}{}", " ".repeat(padding), rendered.right)
}

fn save(theme: &Theme, config_dir: &Path) -> i32 {
    let path = theme_path(config_dir);
    if let Some(parent) = path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        shell::error(format!(
            "configure-prompt: {}: {}",
            parent.display(),
            sys::error_message(&error)
        ));
        return 1;
    }
    if let Err(error) = std::fs::write(&path, theme.to_file()) {
        shell::error(format!(
            "configure-prompt: {}: {}",
            path.display(),
            sys::error_message(&error)
        ));
        return 1;
    }
    println!(
        "Saved to {}. Edit it any time, changes show at the next prompt.",
        path.display()
    );
    0
}

pub(crate) fn run(_shell: &mut Shell, config_dir: &Path, args: &[String]) -> i32 {
    match args {
        [flag] if flag == "--list" => {
            for name in PRESETS {
                println!("{name}");
            }
            return 0;
        }
        [flag, name] if flag == "--preset" => {
            let Some(theme) = Theme::preset(name) else {
                shell::error(format!(
                    "configure-prompt: unknown preset '{name}' (available: {})",
                    PRESETS.join(", ")
                ));
                return 1;
            };
            return save(&theme, config_dir);
        }
        [] => {}
        _ => {
            shell::error("configure-prompt: usage: configure-prompt [--list | --preset name]");
            return 2;
        }
    }
    if !sys::isatty(0) || !sys::isatty(1) {
        shell::error("configure-prompt: needs an interactive terminal");
        return 1;
    }
    println!("grfsh prompt setup. Press Enter to keep the suggested answer in brackets.\n");

    let Some(answer) = ask(
        "Does this look like a diamond in a box?  \u{e0b0}  [y/n]:",
        &["y", "n"],
        1,
    ) else {
        return 1;
    };
    let icons = if answer == 0 {
        Icons::Nerd
    } else {
        match ask(
            "Does this look like an arrow pointing right?  \u{276f}  [Y/n]:",
            &["y", "n"],
            0,
        ) {
            Some(0) => Icons::Unicode,
            Some(_) => Icons::Ascii,
            None => return 1,
        }
    };

    let mut theme = Theme {
        icons,
        ..Theme::preset("lean").expect("lean preset")
    };

    println!();
    let styles: Vec<(Style, &str)> = [
        (Style::Lean, "lean"),
        (Style::Classic, "classic"),
        (Style::Rainbow, "rainbow"),
    ]
    .into_iter()
    .filter(|(style, _)| *style != Style::Rainbow || icons == Icons::Nerd)
    .collect();
    let mut keys = Vec::new();
    for (index, (style, name)) in styles.iter().enumerate() {
        let candidate = Theme {
            style: *style,
            ..theme.clone()
        };
        println!("  ({}) {name}\n{}\n", index + 1, preview(&candidate));
        keys.push((index + 1).to_string());
    }
    let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
    let Some(choice) = ask("Prompt style [1]:", &key_refs, 0) else {
        return 1;
    };
    theme.style = styles[choice].0;

    theme.two_lines = match ask(
        "Prompt height: (1) one line  (2) two lines [1]:",
        &["1", "2"],
        0,
    ) {
        Some(answer) => answer == 1,
        None => return 1,
    };
    let languages = "venv rust node python go";
    let right = match ask(
        "Right side: (1) status, duration, jobs, languages, time  (2) without time  (3) nothing [2]:",
        &["1", "2", "3"],
        1,
    ) {
        Some(0) => format!("status duration jobs {languages} time"),
        Some(1) => format!("status duration jobs {languages}"),
        Some(_) => String::new(),
        None => return 1,
    };
    theme.right = right.split_whitespace().map(String::from).collect();
    theme.transient = match ask(
        "Transient prompt, so earlier prompts shrink to the prompt character? [Y/n]:",
        &["y", "n"],
        0,
    ) {
        Some(answer) => answer == 0,
        None => return 1,
    };

    println!("\nYour prompt:\n{}\n", preview(&theme));
    save(&theme, config_dir)
}

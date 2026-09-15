mod render;
mod segments;
pub(crate) mod wizard;

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::text::display_width;

use super::helper::RightPrompt;

pub(super) use render::{RenderedPrompt, render};
pub(super) use segments::PromptContext;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Style {
    Lean,
    Classic,
    Rainbow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum Icons {
    Nerd,
    Unicode,
    Ascii,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Engine {
    Native,
    Starship,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Theme {
    pub engine: Engine,
    pub style: Style,
    pub icons: Icons,
    pub two_lines: bool,
    pub transient: bool,
    pub left: Vec<String>,
    pub right: Vec<String>,
    pub colors: Vec<(String, String)>,
    pub prompt_char: Option<String>,
    pub dir_depth: usize,
    pub duration_threshold: f64,
    pub context_always: bool,
}

pub(crate) const PRESETS: &[&str] = &["lean", "classic", "rainbow", "pure", "minimal"];

impl Theme {
    pub(crate) fn preset(name: &str) -> Option<Theme> {
        let words = |text: &str| {
            text.split_whitespace()
                .map(String::from)
                .collect::<Vec<_>>()
        };
        let base = Theme {
            engine: Engine::Native,
            style: Style::Lean,
            icons: Icons::Unicode,
            two_lines: false,
            transient: false,
            left: words("context dir git"),
            right: words("status duration jobs venv rust node python go"),
            colors: Vec::new(),
            prompt_char: None,
            dir_depth: 3,
            duration_threshold: 2.0,
            context_always: false,
        };
        Some(match name {
            "lean" => base,
            "classic" => Theme {
                style: Style::Classic,
                right: words("status duration jobs venv rust node python go time"),
                ..base
            },
            "rainbow" => Theme {
                style: Style::Rainbow,
                icons: Icons::Nerd,
                right: words("status duration jobs venv rust node python go time"),
                ..base
            },
            "pure" => Theme {
                two_lines: true,
                transient: true,
                left: words("dir git duration"),
                right: Vec::new(),
                ..base
            },
            "minimal" => Theme {
                left: words("dir"),
                right: words("status git"),
                transient: true,
                ..base
            },
            _ => return None,
        })
    }

    pub(crate) fn parse(text: &str) -> Result<Theme, String> {
        let mut theme = Theme::preset("lean").expect("lean preset");
        let mut entries = Vec::new();
        for (number, line) in text.lines().enumerate() {
            let line = line.split(" # ").next().unwrap_or(line).trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                return Err(format!("line {}: expected key = value", number + 1));
            };
            entries.push((number + 1, key.trim().to_string(), value.trim().to_string()));
        }
        if let Some((number, _, value)) = entries.iter().find(|(_, key, _)| key == "preset") {
            theme = Theme::preset(value)
                .ok_or_else(|| format!("line {number}: unknown preset '{value}'"))?;
        }
        for (number, key, value) in entries {
            let invalid = || format!("line {number}: invalid value '{value}' for {key}");
            let flag = || match value.as_str() {
                "true" | "yes" | "on" => Ok(true),
                "false" | "no" | "off" => Ok(false),
                _ => Err(invalid()),
            };
            match key.as_str() {
                "preset" => {}
                "engine" => {
                    theme.engine = match value.as_str() {
                        "native" => Engine::Native,
                        "starship" => Engine::Starship,
                        _ => return Err(invalid()),
                    }
                }
                "style" => {
                    theme.style = match value.as_str() {
                        "lean" => Style::Lean,
                        "classic" => Style::Classic,
                        "rainbow" => Style::Rainbow,
                        _ => return Err(invalid()),
                    }
                }
                "icons" => {
                    theme.icons = match value.as_str() {
                        "nerd" => Icons::Nerd,
                        "unicode" => Icons::Unicode,
                        "ascii" => Icons::Ascii,
                        _ => return Err(invalid()),
                    }
                }
                "lines" => {
                    theme.two_lines = match value.as_str() {
                        "1" => false,
                        "2" => true,
                        _ => return Err(invalid()),
                    }
                }
                "transient" => theme.transient = flag()?,
                "context_always" => theme.context_always = flag()?,
                "left" => theme.left = value.split_whitespace().map(String::from).collect(),
                "right" => theme.right = value.split_whitespace().map(String::from).collect(),
                "prompt_char" => theme.prompt_char = Some(value.trim_matches('"').to_string()),
                "dir_depth" => theme.dir_depth = value.parse().map_err(|_| invalid())?,
                "duration_threshold" => {
                    theme.duration_threshold = value.parse().map_err(|_| invalid())?
                }
                color if color.starts_with("color.") => {
                    if render::color_code(&value, false).is_none() {
                        return Err(invalid());
                    }
                    theme
                        .colors
                        .push((color["color.".len()..].to_string(), value));
                }
                _ => return Err(format!("line {number}: unknown setting '{key}'")),
            }
        }
        for segment in theme.left.iter().chain(&theme.right) {
            if !segments::NAMES.contains(&segment.as_str()) {
                return Err(format!(
                    "unknown segment '{segment}' (available: {})",
                    segments::NAMES.join(" ")
                ));
            }
        }
        Ok(theme)
    }

    pub(crate) fn to_file(&self) -> String {
        let style = match self.style {
            Style::Lean => "lean",
            Style::Classic => "classic",
            Style::Rainbow => "rainbow",
        };
        let icons = match self.icons {
            Icons::Nerd => "nerd",
            Icons::Unicode => "unicode",
            Icons::Ascii => "ascii",
        };
        let mut text = String::from("# grfsh prompt theme, see `configure-prompt` to regenerate\n");
        text.push_str(&format!("style = {style}\nicons = {icons}\n"));
        text.push_str(&format!("lines = {}\n", if self.two_lines { 2 } else { 1 }));
        text.push_str(&format!("transient = {}\n", self.transient));
        text.push_str(&format!("left = {}\n", self.left.join(" ")));
        text.push_str(&format!("right = {}\n", self.right.join(" ")));
        text.push_str(&format!("dir_depth = {}\n", self.dir_depth));
        text.push_str(&format!(
            "duration_threshold = {}\n",
            self.duration_threshold
        ));
        if self.engine == Engine::Starship {
            text.push_str("engine = starship\n");
        }
        for (segment, color) in &self.colors {
            text.push_str(&format!("color.{segment} = {color}\n"));
        }
        text
    }
}

pub(super) struct Screen {
    pub prompt: String,
    pub right: Option<RightPrompt>,
    pub transient: Option<String>,
}

impl Screen {
    pub(super) fn plain(prompt: String) -> Screen {
        Screen {
            prompt,
            right: None,
            transient: None,
        }
    }
}

pub(super) fn layout(rendered: RenderedPrompt, columns: usize, transient: bool) -> Screen {
    let transient = transient.then_some(rendered.transient);
    if rendered.right.is_empty() {
        return Screen {
            prompt: rendered.left,
            right: None,
            transient,
        };
    }
    if let Some((first, rest)) = rendered.left.split_once('\n') {
        let used = display_width(first) + rendered.right_width;
        let prompt = if used < columns {
            format!(
                "{first}{}{}\n{rest}",
                " ".repeat(columns - used - 1),
                rendered.right
            )
        } else {
            rendered.left.clone()
        };
        return Screen {
            prompt,
            right: None,
            transient,
        };
    }
    Screen {
        right: Some(RightPrompt {
            width: rendered.right_width,
            left_width: display_width(&rendered.left),
            colored: rendered.right,
            columns,
        }),
        prompt: rendered.left,
        transient,
    }
}

fn rows(width: usize, columns: usize) -> usize {
    width.max(1).div_ceil(columns.max(1))
}

pub(super) fn collapse(
    prompt: &str,
    line: &str,
    transient: &str,
    highlighted: &str,
    columns: usize,
) {
    let prompt_lines: Vec<&str> = prompt.split('\n').collect();
    let input_lines: Vec<&str> = line.split('\n').collect();
    let mut total = 0;
    for prompt_line in &prompt_lines[..prompt_lines.len() - 1] {
        total += rows(display_width(prompt_line), columns);
    }
    let last_prompt = prompt_lines.last().map_or(0, |last| display_width(last));
    total += rows(last_prompt + display_width(input_lines[0]), columns);
    for input_line in &input_lines[1..] {
        total += rows(display_width(input_line), columns);
    }
    let mut out = std::io::stdout().lock();
    let _ = write!(out, "\x1b[{total}A\r\x1b[J{transient}{highlighted}\n");
    let _ = out.flush();
}

pub(super) fn starship(context: &PromptContext, columns: usize) -> Option<RenderedPrompt> {
    let run = |right: bool| -> Option<String> {
        let mut command = std::process::Command::new("starship");
        command
            .arg("prompt")
            .arg(format!("--status={}", context.status))
            .arg(format!("--cmd-duration={}", context.duration.as_millis()))
            .arg(format!("--jobs={}", context.jobs))
            .arg(format!("--terminal-width={columns}"))
            .env("STARSHIP_SHELL", "grfsh")
            .stdin(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        if right {
            command.arg("--right");
        }
        let output = command
            .output()
            .ok()
            .filter(|output| output.status.success())?;
        Some(String::from_utf8_lossy(&output.stdout).into_owned())
    };
    let left = run(false)?;
    let right = run(true).unwrap_or_default();
    let right = right.trim_end_matches('\n').to_string();
    Some(RenderedPrompt {
        transient: "\u{276f} ".into(),
        right_width: display_width(&right),
        right,
        left,
    })
}

pub(crate) fn theme_path(config_dir: &Path) -> PathBuf {
    config_dir.join("theme")
}

#[derive(Default)]
pub(super) struct ThemeCache {
    loaded: Option<(Option<SystemTime>, Option<String>, Option<Theme>)>,
}

impl ThemeCache {
    pub(super) fn current(&mut self, path: &Path) -> Option<Theme> {
        let preset = std::env::var("GRFSH_THEME")
            .ok()
            .filter(|name| !name.is_empty());
        let modified = std::fs::metadata(path)
            .and_then(|metadata| metadata.modified())
            .ok();
        if let Some((cached_time, cached_preset, theme)) = &self.loaded
            && *cached_time == modified
            && *cached_preset == preset
        {
            return theme.clone();
        }
        let theme = match std::fs::read_to_string(path) {
            Ok(text) => match Theme::parse(&text) {
                Ok(theme) => Some(theme),
                Err(message) => {
                    crate::shell::error(format!("{}: {message}", path.display()));
                    None
                }
            },
            Err(_) => preset.as_deref().and_then(Theme::preset),
        };
        self.loaded = Some((modified, preset, theme.clone()));
        theme
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_exist() {
        for name in PRESETS {
            assert!(Theme::preset(name).is_some(), "{name}");
        }
        assert!(Theme::preset("nope").is_none());
    }

    #[test]
    fn parse_overrides_a_preset() {
        let theme = Theme::parse(
            "preset = classic\nicons = ascii\nlines = 2\nleft = dir git\ncolor.dir = #ff8800\n",
        )
        .unwrap();
        assert_eq!(theme.style, Style::Classic);
        assert_eq!(theme.icons, Icons::Ascii);
        assert!(theme.two_lines);
        assert_eq!(theme.left, ["dir", "git"]);
        assert_eq!(theme.colors, [("dir".to_string(), "#ff8800".to_string())]);
    }

    #[test]
    fn parse_reports_problems() {
        assert!(
            Theme::parse("style = fancy")
                .unwrap_err()
                .contains("line 1")
        );
        assert!(
            Theme::parse("left = dir weather")
                .unwrap_err()
                .contains("weather")
        );
        assert!(Theme::parse("bogus").is_err());
        assert!(Theme::parse("color.dir = notacolor").is_err());
    }

    #[test]
    fn written_file_parses_back() {
        let theme = Theme::preset("rainbow").unwrap();
        assert_eq!(Theme::parse(&theme.to_file()).unwrap(), theme);
    }
}

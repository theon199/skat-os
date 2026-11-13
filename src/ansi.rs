use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use regex::Regex;
use std::sync::LazyLock;

static ANSI_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07\x1b]*(\x07|\x1b\\)|\x1b\([a-zA-Z]").unwrap()
});

/// Strips all ANSI escape sequences from a string
pub fn strip_ansi_codes(input: &str) -> String {
    ANSI_REGEX.replace_all(input, "").to_string()
}

/// Processes a terminal output stream handling carriage returns (`\r`),
/// line-clearing sequences (`\x1b[2K`), and newlines.
/// This ensures progress bars (e.g. `10%\r50%`) overwrite in place cleanly.
pub fn process_terminal_stream(raw: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    let mut current_line = String::new();

    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            if chars.peek() == Some(&'\n') {
                chars.next();
                lines.push(current_line);
                current_line = String::new();
            } else {
                // Bare \r: return to start of line (overwrite)
                current_line.clear();
            }
        } else if c == '\n' {
            lines.push(current_line);
            current_line = String::new();
        } else if c == '\x1b' {
            // Check for escape sequences
            let mut seq = String::from(c);
            let mut is_clear_line = false;

            if chars.peek() == Some(&'[') {
                seq.push(chars.next().unwrap());
                while let Some(&next_c) = chars.peek() {
                    seq.push(chars.next().unwrap());
                    if next_c.is_ascii_alphabetic() {
                        if seq == "\x1b[2K" || seq == "\x1b[K" {
                            is_clear_line = true;
                        }
                        break;
                    }
                }
            } else if chars.peek() == Some(&']') {
                // OSC sequence
                seq.push(chars.next().unwrap());
                while let Some(next_c) = chars.next() {
                    seq.push(next_c);
                    if next_c == '\x07' || (next_c == '\\' && seq.ends_with("\x1b\\")) {
                        break;
                    }
                }
            } else if chars.peek() == Some(&'(') {
                seq.push(chars.next().unwrap());
                if let Some(next_c) = chars.next() {
                    seq.push(next_c);
                }
            }

            if is_clear_line {
                current_line.clear();
            } else {
                current_line.push_str(&seq);
            }
        } else {
            current_line.push(c);
        }
    }

    lines.push(current_line);
    lines.join("\n")
}

/// Parses a string containing ANSI escape codes into styled Ratatui `Span`s.
pub fn parse_ansi_spans(input: &str) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut current_style = Style::default();
    let mut text_buf = String::new();

    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            // Flush buffered text
            if !text_buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut text_buf), current_style));
            }

            chars.next(); // consume '['
            let mut param_str = String::new();
            let mut final_cmd = ' ';

            while let Some(&next_c) = chars.peek() {
                chars.next();
                if next_c.is_ascii_alphabetic() {
                    final_cmd = next_c;
                    break;
                } else {
                    param_str.push(next_c);
                }
            }

            if final_cmd == 'm' {
                // SGR (Select Graphic Rendition) command
                apply_sgr_params(&param_str, &mut current_style);
            }
            // Non-SGR sequences (e.g. cursor moves, clear line) are consumed without style change
        } else if c == '\x1b' && chars.peek() == Some(&']') {
            // OSC sequence: consume up to BEL (\x07) or ST (\x1b\)
            if !text_buf.is_empty() {
                spans.push(Span::styled(std::mem::take(&mut text_buf), current_style));
            }
            chars.next(); // consume ']'
            while let Some(next_c) = chars.next() {
                if next_c == '\x07' {
                    break;
                }
                if next_c == '\x1b' && chars.peek() == Some(&'\\') {
                    chars.next();
                    break;
                }
            }
        } else if c == '\x1b' && chars.peek() == Some(&'(') {
            // Charset sequence \x1b(B
            chars.next(); // consume '('
            chars.next(); // consume charset char
        } else if c == '\r' {
            // Carriage return without newline: ignored or handled in stream processor
            continue;
        } else {
            text_buf.push(c);
        }
    }

    if !text_buf.is_empty() {
        spans.push(Span::styled(text_buf, current_style));
    }

    if spans.is_empty() {
        spans.push(Span::raw(String::new()));
    }

    spans
}

/// Parses an entire multiline string into a vector of Ratatui `Line`s
pub fn parse_ansi_to_lines(input: &str) -> Vec<Line<'static>> {
    let processed = process_terminal_stream(input);
    processed
        .lines()
        .map(|line| Line::from(parse_ansi_spans(line)))
        .collect()
}

fn apply_sgr_params(param_str: &str, style: &mut Style) {
    if param_str.is_empty() {
        *style = Style::default();
        return;
    }

    let params: Vec<u32> = param_str
        .split(';')
        .filter_map(|s| if s.is_empty() { Some(0) } else { s.parse::<u32>().ok() })
        .collect();

    let mut i = 0;
    while i < params.len() {
        match params[i] {
            0 => *style = Style::default(),
            1 => *style = style.add_modifier(Modifier::BOLD),
            2 => *style = style.add_modifier(Modifier::DIM),
            3 => *style = style.add_modifier(Modifier::ITALIC),
            4 => *style = style.add_modifier(Modifier::UNDERLINED),
            5 | 6 => *style = style.add_modifier(Modifier::RAPID_BLINK),
            7 => *style = style.add_modifier(Modifier::REVERSED),
            8 => *style = style.add_modifier(Modifier::HIDDEN),
            9 => *style = style.add_modifier(Modifier::CROSSED_OUT),
            22 => *style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            23 => *style = style.remove_modifier(Modifier::ITALIC),
            24 => *style = style.remove_modifier(Modifier::UNDERLINED),
            27 => *style = style.remove_modifier(Modifier::REVERSED),
            29 => *style = style.remove_modifier(Modifier::CROSSED_OUT),

            // Standard Foreground Colors (30-37)
            30 => *style = style.fg(Color::Black),
            31 => *style = style.fg(Color::Red),
            32 => *style = style.fg(Color::Green),
            33 => *style = style.fg(Color::Yellow),
            34 => *style = style.fg(Color::Blue),
            35 => *style = style.fg(Color::Magenta),
            36 => *style = style.fg(Color::Cyan),
            37 => *style = style.fg(Color::Gray),
            39 => *style = style.fg(Color::Reset),

            // Standard Background Colors (40-47)
            40 => *style = style.bg(Color::Black),
            41 => *style = style.bg(Color::Red),
            42 => *style = style.bg(Color::Green),
            43 => *style = style.bg(Color::Yellow),
            44 => *style = style.bg(Color::Blue),
            45 => *style = style.bg(Color::Magenta),
            46 => *style = style.bg(Color::Cyan),
            47 => *style = style.bg(Color::Gray),
            49 => *style = style.bg(Color::Reset),

            // Bright Foreground Colors (90-97)
            90 => *style = style.fg(Color::DarkGray),
            91 => *style = style.fg(Color::LightRed),
            92 => *style = style.fg(Color::LightGreen),
            93 => *style = style.fg(Color::LightYellow),
            94 => *style = style.fg(Color::LightBlue),
            95 => *style = style.fg(Color::LightMagenta),
            96 => *style = style.fg(Color::LightCyan),
            97 => *style = style.fg(Color::White),

            // Bright Background Colors (100-107)
            100 => *style = style.bg(Color::DarkGray),
            101 => *style = style.bg(Color::LightRed),
            102 => *style = style.bg(Color::LightGreen),
            103 => *style = style.bg(Color::LightYellow),
            104 => *style = style.bg(Color::LightBlue),
            105 => *style = style.bg(Color::LightMagenta),
            106 => *style = style.bg(Color::LightCyan),
            107 => *style = style.bg(Color::White),

            // Extended Foreground (38;5;n or 38;2;r;g;b)
            38 => {
                if i + 1 < params.len() {
                    match params[i + 1] {
                        5 => {
                            if i + 2 < params.len() {
                                *style = style.fg(Color::Indexed(params[i + 2] as u8));
                                i += 2;
                            }
                        }
                        2 => {
                            if i + 4 < params.len() {
                                let r = params[i + 2] as u8;
                                let g = params[i + 3] as u8;
                                let b = params[i + 4] as u8;
                                *style = style.fg(Color::Rgb(r, g, b));
                                i += 4;
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Extended Background (48;5;n or 48;2;r;g;b)
            48 => {
                if i + 1 < params.len() {
                    match params[i + 1] {
                        5 => {
                            if i + 2 < params.len() {
                                *style = style.bg(Color::Indexed(params[i + 2] as u8));
                                i += 2;
                            }
                        }
                        2 => {
                            if i + 4 < params.len() {
                                let r = params[i + 2] as u8;
                                let g = params[i + 3] as u8;
                                let b = params[i + 4] as u8;
                                *style = style.bg(Color::Rgb(r, g, b));
                                i += 4;
                            }
                        }
                        _ => {}
                    }
                }
            }

            _ => {}
        }
        i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ansi_colors_and_modifiers() {
        let input = "\x1b[32mSuccess\x1b[0m \x1b[1;31mError\x1b[0m";
        let spans = parse_ansi_spans(input);

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "Success");
        assert_eq!(spans[0].style, Style::default().fg(Color::Green));

        assert_eq!(spans[1].content, " ");
        assert_eq!(spans[1].style, Style::default());

        assert_eq!(spans[2].content, "Error");
        assert_eq!(
            spans[2].style,
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD)
        );
    }

    #[test]
    fn test_parse_256_and_rgb_colors() {
        let input = "\x1b[38;5;201mMagenta256\x1b[0m \x1b[38;2;100;150;200mTrueColor\x1b[0m";
        let spans = parse_ansi_spans(input);

        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content, "Magenta256");
        assert_eq!(spans[0].style, Style::default().fg(Color::Indexed(201)));

        assert_eq!(spans[2].content, "TrueColor");
        assert_eq!(spans[2].style, Style::default().fg(Color::Rgb(100, 150, 200)));
    }

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[1;32mCompiling\x1b[0m \x1b[33mcore\x1b[0m v0.1.0 (\x1b[4mhttps://crates.io\x1b[0m)";
        assert_eq!(strip_ansi_codes(input), "Compiling core v0.1.0 (https://crates.io)");
    }

    #[test]
    fn test_process_terminal_stream_progress_bar_overwrite() {
        let input = "Progress: [==>       ] 20%\rProgress: [======>   ] 60%\rProgress: [==========] 100%\nDone!";
        let processed = process_terminal_stream(input);
        assert_eq!(processed, "Progress: [==========] 100%\nDone!");
    }

    #[test]
    fn test_ignore_cursor_and_osc_sequences() {
        let input = "\x1b[?25l\x1b]0;my_window_title\x07\x1b[34mBlueText\x1b[0m\x1b[?25h";
        let spans = parse_ansi_spans(input);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].content, "BlueText");
        assert_eq!(spans[0].style, Style::default().fg(Color::Blue));
    }
}

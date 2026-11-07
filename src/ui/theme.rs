use ratatui::style::{Color, Modifier, Style};

pub struct Theme;

impl Theme {
    #[allow(dead_code)]
    pub const BORDER_ACTIVE: Color = Color::Cyan;
    #[allow(dead_code)]
    pub const BORDER_INACTIVE: Color = Color::DarkGray;

    pub const TITLE_ACTIVE: Style = Style::new()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    pub const TITLE_INACTIVE: Style = Style::new()
        .fg(Color::DarkGray);

    pub const STATUS_BAR: Style = Style::new()
        .bg(Color::Rgb(25, 30, 45))
        .fg(Color::White);

    pub const STATUS_BADGE_NORMAL: Style = Style::new()
        .bg(Color::Cyan)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    pub const STATUS_BADGE_RUNNING: Style = Style::new()
        .bg(Color::Yellow)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    pub const STATUS_BADGE_DIRTY: Style = Style::new()
        .bg(Color::Magenta)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    pub const SIDEBAR_ITEM: Style = Style::new()
        .fg(Color::Gray);

    pub const SIDEBAR_ITEM_SELECTED: Style = Style::new()
        .fg(Color::Cyan)
        .bg(Color::Rgb(40, 45, 60))
        .add_modifier(Modifier::BOLD);

    pub const SIDEBAR_BACKLINK: Style = Style::new()
        .fg(Color::Magenta);

    // Markdown styles
    pub const MD_H1: Style = Style::new()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    pub const MD_H2: Style = Style::new()
        .fg(Color::LightCyan)
        .add_modifier(Modifier::BOLD);

    pub const MD_H3: Style = Style::new()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    pub const MD_CODE_FENCE: Style = Style::new()
        .fg(Color::Green)
        .add_modifier(Modifier::BOLD);

    pub const MD_CODE_LINE: Style = Style::new()
        .fg(Color::LightGreen)
        .bg(Color::Rgb(20, 30, 25));

    pub const MD_OUTPUT_FENCE: Style = Style::new()
        .fg(Color::LightBlue)
        .add_modifier(Modifier::BOLD);

    pub const MD_OUTPUT_LINE: Style = Style::new()
        .fg(Color::White)
        .bg(Color::Rgb(20, 25, 35));

    pub const MD_WIKILINK: Style = Style::new()
        .fg(Color::LightMagenta)
        .add_modifier(Modifier::UNDERLINED);

    pub const MD_LIST_BULLET: Style = Style::new()
        .fg(Color::Yellow);

    pub const MD_BLOCKQUOTE: Style = Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::ITALIC);

    pub const PALETTE_INPUT: Style = Style::new()
        .fg(Color::White)
        .bg(Color::Rgb(30, 35, 50));

    pub const PALETTE_SELECTED: Style = Style::new()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);
}

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

    pub const STATUS_BADGE_HOST: Style = Style::new()
        .bg(Color::Blue)
        .fg(Color::White)
        .add_modifier(Modifier::BOLD);

    pub const STATUS_BADGE_CLIENT: Style = Style::new()
        .bg(Color::Green)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    pub const COLLAB_PEER_BADGE: Style = Style::new()
        .bg(Color::Rgb(50, 60, 80))
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    pub const STATUS_BADGE_TASKS: Style = Style::new()
        .bg(Color::Rgb(35, 65, 45))
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);

    pub const STATUS_BADGE_TASKS_COMPLETE: Style = Style::new()
        .bg(Color::Green)
        .fg(Color::Black)
        .add_modifier(Modifier::BOLD);

    pub const MD_TASK_CHECKED: Style = Style::new()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);

    pub const MD_TASK_UNCHECKED: Style = Style::new()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    pub const MD_TASK_COMPLETED_TEXT: Style = Style::new()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::CROSSED_OUT);

    // Form parameter widgets
    pub const PARAM_INPUT: Style = Style::new()
        .fg(Color::LightBlue)
        .bg(Color::Rgb(25, 35, 55))
        .add_modifier(Modifier::BOLD);

    pub const PARAM_SELECT: Style = Style::new()
        .fg(Color::LightYellow)
        .bg(Color::Rgb(45, 40, 20))
        .add_modifier(Modifier::BOLD);

    pub const PARAM_TOGGLE_ON: Style = Style::new()
        .fg(Color::LightGreen)
        .bg(Color::Rgb(20, 45, 30))
        .add_modifier(Modifier::BOLD);

    pub const PARAM_TOGGLE_OFF: Style = Style::new()
        .fg(Color::DarkGray)
        .bg(Color::Rgb(30, 30, 35))
        .add_modifier(Modifier::BOLD);

    pub const PARAM_FOCUSED: Style = Style::new()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    // Table styles
    pub const TABLE_HEADER: Style = Style::new()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    pub const TABLE_ROW_EVEN: Style = Style::new()
        .fg(Color::White)
        .bg(Color::Rgb(25, 30, 40));

    pub const TABLE_ROW_ODD: Style = Style::new()
        .fg(Color::Gray)
        .bg(Color::Rgb(18, 22, 30));

    pub const TABLE_ROW_SELECTED: Style = Style::new()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    // Chart styles
    pub const CHART_SPARKLINE: Style = Style::new()
        .fg(Color::Cyan);

    pub const CHART_BAR: Style = Style::new()
        .fg(Color::Green);

    pub const CHART_BAR_VALUE: Style = Style::new()
        .fg(Color::Black)
        .bg(Color::Green)
        .add_modifier(Modifier::BOLD);

    // Multi-Host Fleet Mesh styles
    pub const FLEET_TAB_ACTIVE: Style = Style::new()
        .fg(Color::Black)
        .bg(Color::Cyan)
        .add_modifier(Modifier::BOLD);

    pub const FLEET_TAB_INACTIVE: Style = Style::new()
        .fg(Color::Gray)
        .bg(Color::Rgb(25, 30, 45));

    pub const FLEET_HOST_OK: Style = Style::new()
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);

    pub const FLEET_HOST_ERR: Style = Style::new()
        .fg(Color::LightRed)
        .add_modifier(Modifier::BOLD);

    // Phase 7: Guardrails, Vault & Audit styles
    pub const STATUS_BADGE_AUDIT: Style = Style::new()
        .bg(Color::Rgb(20, 60, 45))
        .fg(Color::LightGreen)
        .add_modifier(Modifier::BOLD);

    pub const PARAM_SECRET: Style = Style::new()
        .fg(Color::LightRed)
        .bg(Color::Rgb(45, 20, 25))
        .add_modifier(Modifier::BOLD);

    pub const DANGER_BANNER: Style = Style::new()
        .fg(Color::White)
        .bg(Color::Red)
        .add_modifier(Modifier::BOLD);

    pub const DRY_RUN_BADGE: Style = Style::new()
        .fg(Color::Black)
        .bg(Color::Yellow)
        .add_modifier(Modifier::BOLD);

    pub const CONFIRM_MODAL_TITLE_DANGER: Style = Style::new()
        .fg(Color::Red)
        .add_modifier(Modifier::BOLD);

    pub const CONFIRM_MODAL_TITLE_WARN: Style = Style::new()
        .fg(Color::Yellow)
        .add_modifier(Modifier::BOLD);
}

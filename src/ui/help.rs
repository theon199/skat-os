use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};

use crate::ui::theme::Theme;

pub struct HelpView;

impl HelpView {
    pub fn render(frame: &mut Frame, area: Rect) {
        let modal_width = (area.width * 70 / 100).max(50).min(area.width);
        let modal_height = (area.height * 75 / 100).max(18).min(area.height);

        let x = (area.width.saturating_sub(modal_width)) / 2;
        let y = (area.height.saturating_sub(modal_height)) / 2;
        let modal_area = Rect::new(x, y, modal_width, modal_height);

        frame.render_widget(Clear, modal_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::TITLE_ACTIVE)
            .title(Span::styled(
                " skat-os Help & Cheatsheet ",
                Theme::TITLE_ACTIVE,
            ));

        let inner_area = block.inner(modal_area);
        frame.render_widget(block, modal_area);

        let help_text = vec![
            Line::from(Span::styled("Navigation & Windows", Theme::TITLE_ACTIVE)),
            Line::from(vec![
                Span::styled("  Ctrl+K        ", Theme::MD_H2),
                Span::raw("Open Unified Command Palette (jump docs, search, actions)"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+B        ", Theme::MD_H2),
                Span::raw("Toggle Sidebar (file list & reverse backlinks)"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+W / Tab  ", Theme::MD_H2),
                Span::raw("Switch focus between Sidebar and Editor"),
            ]),
            Line::from(vec![
                Span::styled("  F1 / ?        ", Theme::MD_H2),
                Span::raw("Toggle this Help cheatsheet"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+Q        ", Theme::MD_H2),
                Span::raw("Quit skat-os"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Document Editing & Playbooks",
                Theme::TITLE_ACTIVE,
            )),
            Line::from(vec![
                Span::styled("  Standard typing", Theme::MD_H2),
                Span::raw("Intuitive modeless editing (VS Code / Nano style)"),
            ]),
            Line::from(vec![
                Span::styled("  Space         ", Theme::MD_H2),
                Span::raw("Toggle task checklist item (- [ ] <-> - [x])"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+S        ", Theme::MD_H2),
                Span::raw("Save current document to disk"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+Z        ", Theme::MD_H2),
                Span::raw("Undo last edit"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+Y        ", Theme::MD_H2),
                Span::raw("Redo last edit"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Playbook Execution & Code Blocks",
                Theme::TITLE_ACTIVE,
            )),
            Line::from(vec![
                Span::styled("  Ctrl+R        ", Theme::MD_H2),
                Span::raw("Execute code block under cursor (interpolates previous outputs)"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+Shift+R  ", Theme::MD_H2),
                Span::raw("Run All Blocks sequentially (Playbook execution, F5)"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+C        ", Theme::MD_H2),
                Span::raw("Interrupt / cancel live streaming command ([interrupted])"),
            ]),
            Line::from(vec![
                Span::styled("  Ctrl+O        ", Theme::MD_H2),
                Span::raw("Collapse or expand command output block"),
            ]),
            Line::from(vec![
                Span::styled("  {{name.stdout}}", Theme::MD_H3),
                Span::raw("Chain outputs: interpolate prior block stdout or $SKAT_<NAME>_STDOUT"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Wiki Links & Knowledge Graph",
                Theme::TITLE_ACTIVE,
            )),
            Line::from(vec![
                Span::styled("  [[doc-name]]  ", Theme::MD_WIKILINK),
                Span::raw("Links to another document; backlinks appear in the sidebar"),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Press [Esc] or [F1] to return to your workspace.",
                Theme::TITLE_INACTIVE,
            )),
        ];

        let paragraph = Paragraph::new(help_text);
        frame.render_widget(paragraph, inner_area);
    }
}

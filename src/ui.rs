use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Position, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap},
};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{App, DeleteTarget, EditKind, Editor, Mode},
    theme::AppTheme,
};

const MIN_CARD_WIDTH: usize = 34;
const CARD_GAP: usize = 2;

pub fn render(frame: &mut Frame<'_>, app: &mut App, theme: &AppTheme) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(theme.background)),
        area,
    );

    let sections = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(3),
            Constraint::Length(2),
        ])
        .split(area);

    render_header(frame, sections[0], app, theme);
    render_dashboard(frame, sections[1], app, theme);
    render_footer(frame, sections[2], app, theme);

    match &app.mode {
        Mode::Editing(editor) | Mode::Searching(editor) => {
            render_editor(frame, area, editor, app.status.as_deref(), theme)
        }
        Mode::ConfirmDelete(target) => render_confirmation(frame, area, app, *target, theme),
        Mode::Help => render_help(frame, area, theme),
        Mode::Normal => {}
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let total = app.document.item_count();
    let done = app.document.done_count();
    let mut spans = vec![
        Span::styled(
            " WHO / ME ",
            Style::default()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            format!("{} identities", app.document.topics.len()),
            Style::default().fg(theme.bright_foreground),
        ),
        Span::styled(
            format!("  ·  {done}/{total} checked"),
            Style::default().fg(theme.muted),
        ),
    ];
    if !app.query.is_empty() {
        spans.extend([
            Span::raw("  "),
            Span::styled(
                format!(" / {} ", app.query),
                Style::default().fg(theme.background).bg(theme.accent),
            ),
        ]);
    }
    let header = Paragraph::new(Line::from(spans))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.muted)),
        )
        .style(Style::default().bg(theme.background).fg(theme.foreground))
        .alignment(Alignment::Left)
        .wrap(Wrap { trim: true });
    frame.render_widget(header, area);
}

fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &AppTheme) {
    if area.width < 8 || area.height < 2 {
        return;
    }
    let visible_topics = app.visible_topics();
    if visible_topics.is_empty() {
        let message = if app.document.topics.is_empty() {
            "No identities yet\n\nPress t to add the first part of who you are"
        } else {
            "Nothing matches this search\n\nPress Esc to clear it"
        };
        let empty = Paragraph::new(message)
            .style(Style::default().fg(theme.muted).bg(theme.background))
            .alignment(Alignment::Center)
            .block(Block::default().padding(Padding::top(area.height.saturating_sub(4) / 2)));
        frame.render_widget(empty, area);
        app.scroll = 0;
        return;
    }

    let available = area.width as usize;
    let columns = ((available + CARD_GAP) / (MIN_CARD_WIDTH + CARD_GAP))
        .max(1)
        .min(visible_topics.len());
    let card_width = (available - CARD_GAP * (columns - 1)) / columns;
    let used_width = card_width * columns + CARD_GAP * (columns - 1);
    let trailing = available.saturating_sub(used_width);

    let mut dashboard = Vec::<Line<'static>>::new();
    let mut selected_line = 0usize;
    for (row_number, row) in visible_topics.chunks(columns).enumerate() {
        if row_number > 0 {
            dashboard.push(Line::from(""));
        }
        let row_start = dashboard.len();
        let cards: Vec<Card> = row
            .iter()
            .map(|&topic| build_card(app, topic, card_width, theme))
            .collect();
        let row_height = cards.iter().map(|card| card.lines.len()).max().unwrap_or(0);

        for line_index in 0..row_height {
            let mut spans = Vec::new();
            for (column, card) in cards.iter().enumerate() {
                if column > 0 {
                    spans.push(Span::raw(" ".repeat(CARD_GAP)));
                }
                if let Some(line) = card.lines.get(line_index) {
                    spans.extend(line.spans.clone());
                } else {
                    spans.push(Span::raw(" ".repeat(card_width)));
                }
            }
            if trailing > 0 {
                spans.push(Span::raw(" ".repeat(trailing)));
            }
            dashboard.push(Line::from(spans));
        }

        if let Some((_, card)) = row
            .iter()
            .zip(cards.iter())
            .find(|(topic, _)| **topic == app.selected_topic)
        {
            selected_line = row_start + card.selected_line;
        }
    }

    let viewport_height = area.height as usize;
    if selected_line < app.scroll {
        app.scroll = selected_line;
    } else if selected_line >= app.scroll + viewport_height {
        app.scroll = selected_line + 1 - viewport_height;
    }
    let maximum_scroll = dashboard.len().saturating_sub(viewport_height);
    app.scroll = app.scroll.min(maximum_scroll);

    frame.render_widget(
        Paragraph::new(dashboard)
            .style(Style::default().bg(theme.background).fg(theme.foreground))
            .scroll((app.scroll.min(u16::MAX as usize) as u16, 0)),
        area,
    );
}

struct Card {
    lines: Vec<Line<'static>>,
    selected_line: usize,
}

fn build_card(app: &App, topic_index: usize, width: usize, theme: &AppTheme) -> Card {
    let topic = &app.document.topics[topic_index];
    let selected = topic_index == app.selected_topic;
    let border_color = if selected { theme.accent } else { theme.muted };
    let border_style = Style::default().fg(border_color).bg(theme.panel);
    let panel_style = Style::default().fg(theme.foreground).bg(theme.panel);
    let inner_width = width.saturating_sub(2);
    let title_capacity = inner_width.saturating_sub(3);
    let title = truncate_to_width(&topic.name, title_capacity);
    let title_width = UnicodeWidthStr::width(title.as_str());
    let dash_count = inner_width.saturating_sub(title_width + 2);
    let top = format!("╭─ {title}{}╮", "─".repeat(dash_count));
    let mut lines = vec![Line::from(Span::styled(top, border_style))];
    let mut selected_line = 0;
    let visible_items = app.visible_items(topic_index);

    if visible_items.is_empty() {
        let text = if topic.items.is_empty() {
            "No entries · press a to add one"
        } else {
            "No matching entries"
        };
        lines.push(content_line(
            vec![Span::styled(
                pad_to_width(
                    &truncate_to_width(text, inner_width.saturating_sub(2)),
                    inner_width.saturating_sub(2),
                ),
                Style::default().fg(theme.muted).bg(theme.panel),
            )],
            inner_width,
            border_style,
            panel_style,
        ));
    } else {
        let text_width = inner_width.saturating_sub(4).max(1);
        for item_index in visible_items {
            let item = &topic.items[item_index];
            let wrapped = wrap_text(&item.text, text_width);
            let is_selected = selected && app.selected_item == Some(item_index);
            if is_selected {
                selected_line = lines.len();
            }
            for (part_index, part) in wrapped.iter().enumerate() {
                let background = if is_selected {
                    theme.selection
                } else {
                    theme.panel
                };
                let base = Style::default()
                    .fg(if item.done {
                        theme.muted
                    } else {
                        theme.foreground
                    })
                    .bg(background);
                let text_style = if item.done {
                    base.add_modifier(Modifier::CROSSED_OUT)
                } else if is_selected {
                    base.fg(theme.bright_foreground)
                } else {
                    base
                };
                let marker = if part_index == 0 {
                    if item.done { "✓ " } else { "○ " }
                } else {
                    "  "
                };
                let marker_style = Style::default()
                    .fg(if item.done { theme.green } else { theme.muted })
                    .bg(background);
                let side_style = Style::default().fg(border_color).bg(background);
                lines.push(Line::from(vec![
                    Span::styled("│", side_style),
                    Span::styled(" ", Style::default().bg(background)),
                    Span::styled(marker, marker_style),
                    Span::styled(pad_to_width(part, text_width), text_style),
                    Span::styled(" ", Style::default().bg(background)),
                    Span::styled("│", side_style),
                ]));
            }
        }
    }

    lines.push(Line::from(Span::styled(
        format!("╰{}╯", "─".repeat(inner_width)),
        border_style,
    )));
    Card {
        lines,
        selected_line,
    }
}

fn content_line(
    content: Vec<Span<'static>>,
    inner_width: usize,
    border_style: Style,
    panel_style: Style,
) -> Line<'static> {
    let content_width: usize = content
        .iter()
        .map(|span| UnicodeWidthStr::width(span.content.as_ref()))
        .sum();
    let remaining = inner_width.saturating_sub(content_width + 2);
    let mut spans = vec![
        Span::styled("│", border_style),
        Span::styled(" ", panel_style),
    ];
    spans.extend(content);
    spans.push(Span::styled(" ".repeat(remaining + 1), panel_style));
    spans.push(Span::styled("│", border_style));
    Line::from(spans)
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let hints = match app.mode {
        Mode::Normal => {
            "↑↓ entries  ←→ topics  t topic  a entry  Enter edit  Space check  / search  ? help  q quit"
        }
        Mode::Editing(_) => "Enter save  Esc cancel  ←→ move cursor",
        Mode::Searching(_) => "Type to filter  Enter keep  Esc clear",
        Mode::ConfirmDelete(_) => "y / Enter confirm  n / Esc cancel",
        Mode::Help => "Esc / ? close",
    };
    let status = app.status.as_deref().unwrap_or("");
    let line = Line::from(vec![
        Span::styled(hints, Style::default().fg(theme.muted)),
        Span::raw(if status.is_empty() { "" } else { "  ·  " }),
        Span::styled(
            status,
            Style::default().fg(if status.starts_with("Could not") {
                theme.red
            } else {
                theme.green
            }),
        ),
    ]);
    frame.render_widget(
        Paragraph::new(line)
            .style(Style::default().bg(theme.background))
            .wrap(Wrap { trim: true }),
        area,
    );
}

fn render_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    editor: &Editor,
    status: Option<&str>,
    theme: &AppTheme,
) {
    let popup = centered_rect(area, 72, 7);
    frame.render_widget(Clear, popup);
    let title = match editor.kind {
        EditKind::NewTopic => " New identity ",
        EditKind::RenameTopic(_) => " Rename identity ",
        EditKind::AddItem(_) => " New entry ",
        EditKind::EditItem(_, _) => " Edit entry ",
        EditKind::Search => " Search everything ",
    };
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent))
        .padding(Padding::new(1, 1, 1, 0))
        .style(Style::default().bg(theme.panel).fg(theme.foreground));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let input_width = inner.width.saturating_sub(1) as usize;
    let (visible, cursor_column) = visible_input(editor, input_width);
    frame.render_widget(
        Paragraph::new(visible).style(
            Style::default()
                .bg(theme.selection)
                .fg(theme.bright_foreground),
        ),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    if let Some(message) = status {
        frame.render_widget(
            Paragraph::new(message).style(Style::default().fg(theme.red).bg(theme.panel)),
            Rect::new(inner.x, inner.y.saturating_add(2), inner.width, 1),
        );
    }
    frame.set_cursor_position(Position::new(
        inner.x.saturating_add(cursor_column as u16),
        inner.y,
    ));
}

fn render_confirmation(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    target: DeleteTarget,
    theme: &AppTheme,
) {
    let popup = centered_rect(area, 62, 7);
    frame.render_widget(Clear, popup);
    let description = match target {
        DeleteTarget::Topic(topic) => app
            .document
            .topics
            .get(topic)
            .map(|topic| format!("Delete identity {:?} and all of its entries?", topic.name))
            .unwrap_or_else(|| "Delete this identity?".into()),
        DeleteTarget::Item(topic, item) => app
            .document
            .topics
            .get(topic)
            .and_then(|topic| topic.items.get(item))
            .map(|item| format!("Delete entry {:?}?", item.text))
            .unwrap_or_else(|| "Delete this entry?".into()),
    };
    let paragraph = Paragraph::new(vec![
        Line::from(description),
        Line::from(""),
        Line::from(vec![
            Span::styled(
                " Enter / y ",
                Style::default().fg(theme.background).bg(theme.red),
            ),
            Span::raw(" confirm    "),
            Span::styled(
                " Esc / n ",
                Style::default().fg(theme.background).bg(theme.muted),
            ),
            Span::raw(" cancel"),
        ]),
    ])
    .wrap(Wrap { trim: true })
    .block(
        Block::default()
            .title(" Confirm deletion ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.red))
            .padding(Padding::horizontal(1)),
    )
    .style(Style::default().fg(theme.foreground).bg(theme.panel));
    frame.render_widget(paragraph, popup);
}

fn render_help(frame: &mut Frame<'_>, area: Rect, theme: &AppTheme) {
    let popup = centered_rect(area, 78, 23);
    frame.render_widget(Clear, popup);
    let rows = [
        ("↑ / ↓", "Move between an identity title and its entries"),
        ("← / → or Tab", "Move between identities"),
        ("t / a", "Add an identity / add an entry"),
        ("Enter", "Edit the selected title or entry"),
        ("Space", "Check or uncheck the selected entry"),
        ("Delete", "Delete with confirmation"),
        ("Ctrl + ↑ / ↓", "Reorder the selected entry"),
        ("Ctrl + ← / →", "Reorder the selected identity"),
        ("/", "Search identity names and entry text"),
        ("Esc", "Clear search or close the current view"),
        ("q", "Quit"),
    ];
    let mut lines = vec![Line::from("")];
    for (key, description) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<18}"), Style::default().fg(theme.accent)),
            Span::styled(description, Style::default().fg(theme.foreground)),
        ]));
    }
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "Changes are saved immediately. Press Esc or ? to close.",
            Style::default().fg(theme.muted),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .block(
                Block::default()
                    .title(" Keyboard guide ")
                    .title_style(
                        Style::default()
                            .fg(theme.accent)
                            .add_modifier(Modifier::BOLD),
                    )
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.accent))
                    .padding(Padding::horizontal(2)),
            )
            .style(Style::default().bg(theme.panel)),
        popup,
    );
}

fn centered_rect(area: Rect, preferred_width: u16, preferred_height: u16) -> Rect {
    let width = preferred_width.min(area.width.saturating_sub(2)).max(1);
    let height = preferred_height.min(area.height.saturating_sub(2)).max(1);
    Rect::new(
        area.x + area.width.saturating_sub(width) / 2,
        area.y + area.height.saturating_sub(height) / 2,
        width,
        height,
    )
}

fn visible_input(editor: &Editor, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }
    let before: String = editor.input.chars().take(editor.cursor).collect();
    let cursor_width = UnicodeWidthStr::width(before.as_str());
    let skip_width = cursor_width.saturating_sub(width.saturating_sub(1));
    let mut skipped = 0;
    let mut start = 0;
    for (byte, character) in editor.input.char_indices() {
        if skipped >= skip_width {
            start = byte;
            break;
        }
        skipped += character.width().unwrap_or(0);
        start = byte + character.len_utf8();
    }
    let visible = truncate_to_width(&editor.input[start..], width);
    (visible, cursor_width.saturating_sub(skipped).min(width - 1))
}

fn wrap_text(value: &str, width: usize) -> Vec<String> {
    if value.is_empty() || width == 0 {
        return vec![String::new()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in value.split_whitespace() {
        let separator = usize::from(!current.is_empty());
        if UnicodeWidthStr::width(current.as_str()) + separator + UnicodeWidthStr::width(word)
            <= width
        {
            if separator == 1 {
                current.push(' ');
            }
            current.push_str(word);
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
        }
        let mut rest = word;
        while UnicodeWidthStr::width(rest) > width {
            let byte = byte_at_width(rest, width);
            if byte == 0 {
                let next = rest
                    .char_indices()
                    .nth(1)
                    .map(|(byte, _)| byte)
                    .unwrap_or(rest.len());
                lines.push("…".into());
                rest = &rest[next..];
                continue;
            }
            lines.push(rest[..byte].to_owned());
            rest = &rest[byte..];
        }
        current.push_str(rest);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn truncate_to_width(value: &str, width: usize) -> String {
    if UnicodeWidthStr::width(value) <= width {
        return value.to_owned();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".into();
    }
    let byte = byte_at_width(value, width - 1);
    format!("{}…", &value[..byte])
}

fn byte_at_width(value: &str, width: usize) -> usize {
    let mut used = 0;
    for (byte, character) in value.char_indices() {
        let character_width = character.width().unwrap_or(0);
        if used + character_width > width {
            return byte;
        }
        used += character_width;
    }
    value.len()
}

fn pad_to_width(value: &str, width: usize) -> String {
    let used = UnicodeWidthStr::width(value);
    format!("{value}{}", " ".repeat(width.saturating_sub(used)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DATA_VERSION, Document, Item, Topic};
    use ratatui::{Terminal, backend::TestBackend};

    fn sample_app() -> App {
        App::new(Document {
            version: DATA_VERSION,
            topics: vec![
                Topic {
                    name: "Developer".into(),
                    items: vec![
                        Item {
                            text: "Build thoughtful terminal interfaces".into(),
                            done: false,
                        },
                        Item {
                            text: "Ship it".into(),
                            done: true,
                        },
                    ],
                },
                Topic {
                    name: "Mountaineer".into(),
                    items: vec![Item {
                        text: "Climb safely".into(),
                        done: false,
                    }],
                },
            ],
        })
    }

    #[test]
    fn wraps_wide_and_long_text_without_exceeding_width() {
        for line in wrap_text("hello extraordinary界界 world", 8) {
            assert!(UnicodeWidthStr::width(line.as_str()) <= 8, "{line:?}");
        }
    }

    #[test]
    fn renders_standard_and_narrow_terminals() {
        for (width, height) in [(100, 30), (38, 16)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = sample_app();
            terminal
                .draw(|frame| render(frame, &mut app, &AppTheme::default()))
                .unwrap();
            let buffer = terminal.backend().buffer();
            let rendered = buffer
                .content
                .iter()
                .map(|cell| cell.symbol())
                .collect::<String>();
            assert!(rendered.contains("WHO / ME"));
            assert!(rendered.contains("Developer"));
        }
    }

    #[test]
    fn renders_empty_and_search_empty_states() {
        let backend = TestBackend::new(60, 15);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(Document::default());
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        let rendered = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect::<String>();
        assert!(rendered.contains("No identities yet"));
    }
}

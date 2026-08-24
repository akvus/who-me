use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Offset, Position, Rect, Size},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph, Shadow, Wrap},
};
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{App, DeleteTarget, EditKind, Editor, Mode, StatusPicker},
    model::IdentityStatus,
    theme::{AppTheme, TopicVisual},
};

const CARD_GAP_X: u16 = 2;
const CARD_GAP_Y: u16 = 1;
const CARD_MIN_HEIGHT: u16 = 7;

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
            Constraint::Length(1),
        ])
        .split(area);

    render_header(frame, sections[0], app, theme);
    render_dashboard(frame, sections[1], app, theme);
    render_footer(frame, sections[2], app, theme);

    match &app.mode {
        Mode::Editing(editor) | Mode::Searching(editor) => {
            render_editor(frame, area, editor, app.status.as_deref(), theme)
        }
        Mode::SelectingStatus(picker) => render_status_picker(frame, area, *picker, theme),
        Mode::ConfirmDelete(target) => render_confirmation(frame, area, app, *target, theme),
        Mode::Help => render_help(frame, area, theme),
        Mode::Normal => {}
    }
}

fn render_header(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let block = Block::default()
        .borders(Borders::BOTTOM)
        .border_style(Style::default().fg(theme.muted))
        .style(Style::default().bg(theme.background));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let count = format!(
        "{} {}",
        app.document.topics.len(),
        if app.document.topics.len() == 1 {
            "identity"
        } else {
            "identities"
        }
    );
    let count_width = display_width(&count)
        .saturating_add(2)
        .min(inner.width as usize) as u16;
    let [brand_area, count_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(count_width)]).areas(inner);

    let mut brand = vec![
        Span::styled(
            " WHO / ME ",
            Style::default()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::styled(
            "a map of what makes you, you",
            Style::default().fg(theme.dark_foreground),
        ),
    ];
    if !app.query.is_empty() {
        brand.extend([
            Span::raw("  "),
            Span::styled(
                format!(" / {} ", app.query),
                Style::default().fg(theme.background).bg(theme.accent),
            ),
        ]);
    }
    frame.render_widget(
        Paragraph::new(Line::from(brand))
            .style(Style::default().bg(theme.background))
            .wrap(Wrap { trim: true }),
        brand_area,
    );
    frame.render_widget(
        Paragraph::new(count)
            .style(Style::default().fg(theme.muted).bg(theme.background))
            .alignment(Alignment::Right),
        count_area,
    );
}

fn render_dashboard(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &AppTheme) {
    if area.width < 8 || area.height < 2 {
        return;
    }
    let visible_topics = app.visible_topics();
    if visible_topics.is_empty() {
        render_empty_state(frame, area, app, theme);
        app.scroll = 0;
        return;
    }

    let viewport = Rect::new(
        area.x.saturating_add(1),
        area.y,
        area.width.saturating_sub(2),
        area.height,
    );
    let canvas_width = viewport.width.saturating_sub(1).max(1);
    let layout = dashboard_layout(app, canvas_width);
    let mut scroll_view = ScrollView::new(Size::new(canvas_width, layout.height.max(1)))
        .vertical_scrollbar_visibility(ScrollbarVisibility::Automatic)
        .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

    for placement in &layout.cards {
        render_card(&mut scroll_view, app, placement, theme);
    }

    let viewport_height = area.height;
    if layout.selected_y < app.scroll {
        app.scroll = layout.selected_y;
    } else if layout.selected_y >= app.scroll.saturating_add(viewport_height) {
        app.scroll = layout
            .selected_y
            .saturating_add(1)
            .saturating_sub(viewport_height);
    }
    app.scroll = app
        .scroll
        .min(layout.height.saturating_sub(viewport_height));

    let mut state = ScrollViewState::with_offset(Position::new(0, app.scroll));
    frame.render_stateful_widget(scroll_view, viewport, &mut state);
    app.scroll = state.offset().y;
}

#[derive(Clone, Copy, Debug)]
struct CardPlacement {
    topic: usize,
    area: Rect,
}

#[derive(Clone, Copy)]
struct CardAppearance<'a> {
    focused: bool,
    visual: TopicVisual,
    theme: &'a AppTheme,
    background: ratatui::style::Color,
}

#[derive(Debug)]
struct DashboardLayout {
    cards: Vec<CardPlacement>,
    height: u16,
    selected_y: u16,
}

fn dashboard_layout(app: &App, width: u16) -> DashboardLayout {
    let topics = app.visible_topics();
    let columns = dashboard_columns(width).min(topics.len()).max(1);
    let gaps = CARD_GAP_X.saturating_mul(columns.saturating_sub(1) as u16);
    let usable_width = width.saturating_sub(1).saturating_sub(gaps);
    let card_width = (usable_width / columns as u16).max(6);
    let used_width = card_width
        .saturating_mul(columns as u16)
        .saturating_add(gaps);
    let remainder = width.saturating_sub(1).saturating_sub(used_width);

    let mut cards = Vec::with_capacity(topics.len());
    let mut y = 0u16;
    let mut selected_y = 0u16;
    for row in topics.chunks(columns) {
        let row_height = row
            .iter()
            .map(|&topic| card_height(app, topic, card_width))
            .max()
            .unwrap_or(CARD_MIN_HEIGHT);
        for (column, &topic) in row.iter().enumerate() {
            let bonus = u16::from(column + 1 == columns).min(remainder);
            let x = (card_width + CARD_GAP_X).saturating_mul(column as u16);
            let area = Rect::new(x, y, card_width.saturating_add(bonus), row_height);
            if topic == app.selected_topic {
                selected_y = y.saturating_add(card_selected_line(app, topic, area.width));
            }
            cards.push(CardPlacement { topic, area });
        }
        y = y.saturating_add(row_height).saturating_add(CARD_GAP_Y);
    }

    DashboardLayout {
        cards,
        height: y.max(1),
        selected_y,
    }
}

fn dashboard_columns(width: u16) -> usize {
    match width {
        168.. => 4,
        120..=167 => 3,
        72..=119 => 2,
        _ => 1,
    }
}

fn card_height(app: &App, topic: usize, width: u16) -> u16 {
    let text_width = width.saturating_sub(8).max(1) as usize;
    let visible_items = app.visible_items(topic);
    let list_height = if visible_items.is_empty() {
        1
    } else {
        visible_items
            .iter()
            .map(|&item| wrap_text(&app.document.topics[topic].items[item].text, text_width).len())
            .sum()
    };
    (list_height as u16).saturating_add(4).max(CARD_MIN_HEIGHT)
}

fn card_selected_line(app: &App, topic: usize, width: u16) -> u16 {
    let Some(selected) = app.selected_item else {
        return 0;
    };
    let text_width = width.saturating_sub(8).max(1) as usize;
    let before: usize = app
        .visible_items(topic)
        .into_iter()
        .take_while(|&item| item != selected)
        .map(|item| wrap_text(&app.document.topics[topic].items[item].text, text_width).len())
        .sum();
    3u16.saturating_add(before as u16)
}

fn render_card(
    scroll_view: &mut ScrollView,
    app: &App,
    placement: &CardPlacement,
    theme: &AppTheme,
) {
    let topic = &app.document.topics[placement.topic];
    let focused = placement.topic == app.selected_topic;
    let visual = theme.topic_visual(&topic.name);
    let background = if focused {
        theme.panel
    } else {
        theme.dark_background
    };
    let border_color = if focused { visual.color } else { theme.muted };
    let title_name = truncate_to_width(
        &topic.name,
        placement.area.width.saturating_sub(15) as usize,
    );
    let title_style = Style::default()
        .fg(if focused {
            theme.bright_foreground
        } else {
            theme.dark_foreground
        })
        .bg(background)
        .add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::DIM
        });
    let symbol_style = Style::default()
        .fg(visual.color)
        .bg(background)
        .add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::DIM
        });
    let total = topic.items.len();
    let done = topic.items.iter().filter(|item| item.done).count();
    let appearance = CardAppearance {
        focused,
        visual,
        theme,
        background,
    };

    let mut block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(border_color).bg(background))
        .style(Style::default().bg(background))
        .padding(Padding::horizontal(1))
        .title_top(
            Line::from(vec![
                Span::styled(format!(" {} ", visual.symbol), symbol_style),
                Span::styled(format!("{title_name} "), title_style),
            ])
            .left_aligned(),
        )
        .title_top(
            Line::from(Span::styled(
                format!(" {done}/{total} "),
                Style::default()
                    .fg(if focused {
                        theme.muted
                    } else {
                        theme.dark_foreground
                    })
                    .bg(background),
            ))
            .right_aligned(),
        )
        .title_bottom(
            Line::from(Span::styled(
                format!(" {} ", topic.status.label()),
                Style::default()
                    .fg(identity_status_color(topic.status, theme))
                    .bg(background)
                    .add_modifier(if focused {
                        Modifier::BOLD
                    } else {
                        Modifier::DIM
                    }),
            ))
            .left_aligned(),
        );

    if focused && placement.area.width >= 34 {
        let hint = if app.selected_item.is_some() {
            " Space check · ↵ edit "
        } else {
            " a add · ↵ rename "
        };
        block = block.title_bottom(
            Line::from(Span::styled(
                hint,
                Style::default().fg(theme.muted).bg(background),
            ))
            .right_aligned(),
        );
    }
    if focused {
        block = block.shadow(
            Shadow::medium_shade()
                .fg(theme.darker_background)
                .offset(Offset::new(1, 1)),
        );
    }

    let inner = block.inner(placement.area);
    scroll_view.render_widget(block, placement.area);
    if inner.is_empty() {
        return;
    }

    render_progress(scroll_view, inner, done, total, appearance);
    let list_area = Rect::new(
        inner.x,
        inner.y.saturating_add(2),
        inner.width,
        inner.height.saturating_sub(2),
    );
    let lines = card_item_lines(app, placement.topic, inner.width, appearance);
    scroll_view.render_widget(
        Paragraph::new(lines).style(Style::default().bg(background)),
        list_area,
    );
}

fn render_progress(
    scroll_view: &mut ScrollView,
    inner: Rect,
    done: usize,
    total: usize,
    appearance: CardAppearance<'_>,
) {
    let CardAppearance {
        focused,
        visual,
        theme,
        background,
    } = appearance;
    if total == 0 {
        scroll_view.render_widget(
            Paragraph::new(Span::styled(
                "0 entries",
                Style::default().fg(theme.dark_foreground).bg(background),
            )),
            Rect::new(inner.x, inner.y, inner.width, 1),
        );
        return;
    }
    let width = inner.width as usize;
    let filled = (done * width + total / 2) / total;
    let filled_style = Style::default()
        .fg(if focused { visual.color } else { theme.muted })
        .bg(background);
    let empty_style = Style::default()
        .fg(theme.muted)
        .bg(background)
        .add_modifier(Modifier::DIM);
    scroll_view.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled("━".repeat(filled), filled_style),
            Span::styled("─".repeat(width.saturating_sub(filled)), empty_style),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
}

fn identity_status_color(status: IdentityStatus, theme: &AppTheme) -> ratatui::style::Color {
    match status {
        IdentityStatus::Aspiring => theme.yellow,
        IdentityStatus::Active => theme.green,
        IdentityStatus::Former => theme.muted,
    }
}

fn card_item_lines(
    app: &App,
    topic_index: usize,
    width: u16,
    appearance: CardAppearance<'_>,
) -> Vec<Line<'static>> {
    let CardAppearance {
        focused,
        visual,
        theme,
        background,
    } = appearance;
    let topic = &app.document.topics[topic_index];
    let visible_items = app.visible_items(topic_index);
    if visible_items.is_empty() {
        let text = if topic.items.is_empty() {
            "Press a to add the first entry"
        } else {
            "No matching entries"
        };
        return vec![Line::from(Span::styled(
            truncate_to_width(text, width as usize),
            Style::default().fg(theme.dark_foreground).bg(background),
        ))];
    }

    let text_width = width.saturating_sub(4).max(1) as usize;
    let mut lines = Vec::new();
    for item_index in visible_items {
        let item = &topic.items[item_index];
        let selected = focused && app.selected_item == Some(item_index);
        let line_background = if selected {
            theme.selection
        } else {
            background
        };
        for (part_index, part) in wrap_text(&item.text, text_width).iter().enumerate() {
            let cursor = if selected && part_index == 0 {
                "› "
            } else {
                "  "
            };
            let marker = if part_index == 0 {
                if item.done { "✓ " } else { "○ " }
            } else {
                "  "
            };
            let cursor_style = Style::default()
                .fg(if selected { visual.color } else { theme.muted })
                .bg(line_background)
                .add_modifier(if selected {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                });
            let marker_style = Style::default()
                .fg(if item.done {
                    theme.green
                } else if focused {
                    theme.muted
                } else {
                    theme.dark_foreground
                })
                .bg(line_background);
            let mut text_style = Style::default()
                .fg(if selected {
                    theme.bright_foreground
                } else if focused && !item.done {
                    theme.foreground
                } else {
                    theme.dark_foreground
                })
                .bg(line_background);
            if item.done {
                text_style = text_style.add_modifier(Modifier::CROSSED_OUT);
            }
            lines.push(Line::from(vec![
                Span::styled(cursor, cursor_style),
                Span::styled(marker, marker_style),
                Span::styled(pad_to_width(part, text_width), text_style),
            ]));
        }
    }
    lines
}

fn render_empty_state(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let searching = !app.query.is_empty();
    let content = if searching {
        vec![
            Line::from(Span::styled("◇", Style::default().fg(theme.accent))),
            Line::from(""),
            Line::from(Span::styled(
                "Nothing matches this search",
                Style::default()
                    .fg(theme.bright_foreground)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Press Esc to see every identity again",
                Style::default().fg(theme.muted),
            )),
        ]
    } else {
        vec![
            Line::from(vec![
                Span::styled("◆", Style::default().fg(theme.accent)),
                Span::raw("   "),
                Span::styled("▲", Style::default().fg(theme.cyan)),
                Span::raw("   "),
                Span::styled("●", Style::default().fg(theme.magenta)),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                "Your identities live here",
                Style::default()
                    .fg(theme.bright_foreground)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                "Press t to name the first part of who you are",
                Style::default().fg(theme.muted),
            )),
        ]
    };
    frame.render_widget(
        Paragraph::new(content)
            .style(Style::default().bg(theme.background))
            .alignment(Alignment::Center)
            .block(Block::default().padding(Padding::top(area.height.saturating_sub(5) / 2))),
        area,
    );
}

fn render_footer(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let status = app.status.as_deref().unwrap_or("");
    let status_width = display_width(status).min(area.width.saturating_sub(1) as usize) as u16;
    let [hint_area, status_area] = Layout::horizontal([
        Constraint::Min(0),
        Constraint::Length(if status.is_empty() {
            0
        } else {
            status_width.saturating_add(1)
        }),
    ])
    .areas(area);
    let hints = footer_hints(&app.mode, hint_area.width);
    frame.render_widget(
        Paragraph::new(hints).style(Style::default().fg(theme.muted).bg(theme.background)),
        hint_area,
    );
    if !status.is_empty() {
        frame.render_widget(
            Paragraph::new(truncate_to_width(status, status_area.width as usize))
                .style(
                    Style::default()
                        .fg(if status.starts_with("Could not") {
                            theme.red
                        } else {
                            theme.green
                        })
                        .bg(theme.background),
                )
                .alignment(Alignment::Right),
            status_area,
        );
    }
}

fn footer_hints(mode: &Mode, width: u16) -> &'static str {
    match mode {
        Mode::Normal if width >= 105 => {
            "↑↓ entries  ←→ identities  t new  a add  s status  ↵ edit  Space check  / search  ? help  q quit"
        }
        Mode::Normal if width >= 72 => {
            "↑↓ navigate  t new  a add  s status  ↵ edit  / search  ? help  q quit"
        }
        Mode::Normal if width >= 48 => "↑↓←→ navigate  t new  s status  / search  ? help",
        Mode::Normal => "t new  / search  ? help  q quit",
        Mode::Editing(_) => "↵ save  Esc cancel  ←→ cursor",
        Mode::Searching(_) => "type to filter  ↵ keep  Esc clear",
        Mode::SelectingStatus(_) => "↑↓ choose  ↵ save  Esc cancel",
        Mode::ConfirmDelete(_) => "↵ / y confirm  Esc / n cancel",
        Mode::Help => "Esc / ? close",
    }
}

fn render_status_picker(frame: &mut Frame<'_>, area: Rect, picker: StatusPicker, theme: &AppTheme) {
    let popup = centered_rect(area, 42, 9);
    frame.render_widget(Clear, popup);
    let block = overlay_block(" Identity status ", theme.accent, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let lines = IdentityStatus::ALL
        .map(|status| {
            let selected = status == picker.selected;
            Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default()
                        .fg(identity_status_color(status, theme))
                        .bg(if selected {
                            theme.selection
                        } else {
                            theme.panel
                        })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
                Span::styled(
                    pad_to_width(status.label(), inner.width.saturating_sub(2) as usize),
                    Style::default()
                        .fg(identity_status_color(status, theme))
                        .bg(if selected {
                            theme.selection
                        } else {
                            theme.panel
                        })
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ])
        })
        .into_iter()
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.panel)),
        Rect::new(inner.x, inner.y, inner.width, inner.height.min(3)),
    );
}

fn render_editor(
    frame: &mut Frame<'_>,
    area: Rect,
    editor: &Editor,
    status: Option<&str>,
    theme: &AppTheme,
) {
    let is_search = matches!(editor.kind, EditKind::Search);
    let popup = if is_search {
        top_centered_rect(area, 72, 5)
    } else {
        centered_rect(area, 72, 7)
    };
    frame.render_widget(Clear, popup);
    let title = match editor.kind {
        EditKind::NewTopic => " New identity ",
        EditKind::RenameTopic(_) => " Rename identity ",
        EditKind::AddItem(_) => " New entry ",
        EditKind::EditItem(_, _) => " Edit entry ",
        EditKind::Search => " / Search everything ",
    };
    let block = overlay_block(title, theme.accent, theme);
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
    if !is_search && let Some(message) = status {
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
    let block = overlay_block(" Confirm deletion ", theme.red, theme);
    frame.render_widget(
        Paragraph::new(vec![
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
        .block(block)
        .style(Style::default().fg(theme.foreground).bg(theme.panel)),
        popup,
    );
}

fn render_help(frame: &mut Frame<'_>, area: Rect, theme: &AppTheme) {
    let popup = centered_rect(area, 78, 24);
    frame.render_widget(Clear, popup);
    let rows = [
        ("↑ / ↓", "Move between an identity title and its entries"),
        ("← / → or Tab", "Move between identities"),
        ("t / a", "Add an identity / add an entry"),
        ("s", "Choose Aspiring, Active, or Former status"),
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
            "Changes save immediately · Esc or ? closes this guide",
            Style::default().fg(theme.muted),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(lines)
            .block(overlay_block(" Keyboard guide ", theme.accent, theme))
            .style(Style::default().bg(theme.panel)),
        popup,
    );
}

fn overlay_block<'a>(title: &'a str, accent: ratatui::style::Color, theme: &AppTheme) -> Block<'a> {
    Block::default()
        .title(title)
        .title_style(Style::default().fg(accent).add_modifier(Modifier::BOLD))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(accent))
        .padding(Padding::new(1, 1, 1, 0))
        .style(Style::default().bg(theme.panel).fg(theme.foreground))
        .shadow(
            Shadow::medium_shade()
                .fg(theme.darker_background)
                .offset(Offset::new(1, 1)),
        )
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

fn top_centered_rect(area: Rect, preferred_width: u16, preferred_height: u16) -> Rect {
    let mut popup = centered_rect(area, preferred_width, preferred_height);
    popup.y = area
        .y
        .saturating_add(3)
        .min(area.bottom().saturating_sub(popup.height));
    popup
}

fn visible_input(editor: &Editor, width: usize) -> (String, usize) {
    if width == 0 {
        return (String::new(), 0);
    }
    let before: String = editor.input.chars().take(editor.cursor).collect();
    let cursor_width = display_width(&before);
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
        if display_width(&current) + separator + display_width(word) <= width {
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
        while display_width(rest) > width {
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
    if display_width(value) <= width {
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
    format!(
        "{value}{}",
        " ".repeat(width.saturating_sub(display_width(value)))
    )
}

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DATA_VERSION, Document, IdentityStatus, Item, Topic};
    use ratatui::{Terminal, backend::TestBackend};

    fn sample_app() -> App {
        App::new(Document {
            version: DATA_VERSION,
            topics: vec![
                Topic {
                    name: "Developer".into(),
                    status: IdentityStatus::Active,
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
                    status: IdentityStatus::Aspiring,
                    items: vec![Item {
                        text: "Climb safely".into(),
                        done: false,
                    }],
                },
                Topic {
                    name: "Writer".into(),
                    status: IdentityStatus::Former,
                    items: vec![Item {
                        text: "Notice the precise word".into(),
                        done: false,
                    }],
                },
                Topic {
                    name: "Friend".into(),
                    status: IdentityStatus::Active,
                    items: Vec::new(),
                },
            ],
        })
    }

    fn rendered(terminal: &Terminal<TestBackend>) -> String {
        terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect()
    }

    #[test]
    fn wraps_wide_and_long_text_without_exceeding_width() {
        for width in 1..=8 {
            for line in wrap_text("hello extraordinary界界 e\u{301} world", width) {
                assert!(display_width(&line) <= width, "width {width}: {line:?}");
            }
        }
    }

    #[test]
    fn uses_the_documented_adaptive_column_thresholds() {
        assert_eq!(dashboard_columns(60), 1);
        assert_eq!(dashboard_columns(72), 2);
        assert_eq!(dashboard_columns(119), 2);
        assert_eq!(dashboard_columns(120), 3);
        assert_eq!(dashboard_columns(168), 4);
    }

    #[test]
    fn renders_all_supported_layout_sizes() {
        for (width, height) in [(60, 18), (80, 24), (120, 32), (180, 45)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = sample_app();
            terminal
                .draw(|frame| render(frame, &mut app, &AppTheme::default()))
                .unwrap();
            let output = rendered(&terminal);
            assert!(output.contains("WHO / ME"));
            assert!(output.contains("Developer"));
            assert!(output.contains("Mountaineer"));
            assert!(output.contains("Active"));
        }
    }

    #[test]
    fn auto_scrolls_to_the_selected_card() {
        let backend = TestBackend::new(60, 9);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = sample_app();
        app.selected_topic = app.document.topics.len() - 1;
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        assert!(app.scroll > 0);
    }

    #[test]
    fn renders_empty_search_and_overlay_states() {
        let backend = TestBackend::new(80, 24);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(Document::default());
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        assert!(rendered(&terminal).contains("Your identities live here"));

        app = sample_app();
        app.query = "not present".into();
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        assert!(rendered(&terminal).contains("Nothing matches this search"));

        app.mode = Mode::Help;
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        assert!(rendered(&terminal).contains("Keyboard guide"));

        app.mode = Mode::SelectingStatus(StatusPicker {
            topic: 0,
            selected: IdentityStatus::Former,
        });
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        let output = rendered(&terminal);
        assert!(output.contains("Identity status"));
        assert!(output.contains("Aspiring"));
        assert!(output.contains("Former"));
    }
}

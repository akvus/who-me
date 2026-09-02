use chrono::{Datelike, NaiveDate};
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Layout, Offset, Position, Rect, Size},
    style::{Modifier, Style, Stylize},
    text::{Line, Span},
    widgets::{
        Block, BorderType, Borders, Cell, Clear, Padding, Paragraph, Row, Shadow, Table, Wrap,
    },
};
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::{
    app::{
        App, CalendarFocus, CharacteristicField, CharacteristicForm, DeleteTarget, EditKind,
        Editor, Feature, JudgementField, JudgementFocus, JudgementForm, Mode, MoodPicker,
        SettingsState, StatisticsPeriod, StatusPicker, days_in_month,
    },
    model::{IdentityStatus, MoodRating, Sentiment},
    sync::SyncStatus,
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
    match app.feature {
        Feature::Identities => render_dashboard(frame, sections[1], app, theme),
        Feature::Calendar => render_calendar(frame, sections[1], app, theme),
        Feature::Statistics => render_statistics(frame, sections[1], app, theme),
        Feature::Judgements => render_judgements(frame, sections[1], app, theme),
    }
    render_footer(frame, sections[2], app, theme);

    match &app.mode {
        Mode::Editing(editor) | Mode::Searching(editor) => {
            render_editor(frame, area, editor, app.status.as_deref(), theme)
        }
        Mode::SelectingStatus(picker) => render_status_picker(frame, area, *picker, theme),
        Mode::SelectingMood(picker) => render_mood_picker(frame, area, *picker, theme),
        Mode::EditingJudgement(form) => render_judgement_form(frame, area, form, app, theme),
        Mode::EditingCharacteristic(form) => {
            render_characteristic_form(frame, area, form, app, theme)
        }
        Mode::ConfirmDelete(target) => render_confirmation(frame, area, app, *target, theme),
        Mode::Settings(settings) => render_settings(frame, area, app, settings, theme),
        Mode::Help => render_help(frame, area, app, theme),
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

    let count = match app.feature {
        Feature::Identities => format!(
            "{} {}",
            app.document.topics.len(),
            if app.document.topics.len() == 1 {
                "identity"
            } else {
                "identities"
            }
        ),
        Feature::Calendar => {
            let entries: usize = app
                .document
                .calendar
                .days
                .iter()
                .map(|day| day.entries.len())
                .sum();
            format!(
                "{entries} {}",
                if entries == 1 { "entry" } else { "entries" }
            )
        }
        Feature::Statistics => {
            let statistics = app.mood_statistics();
            format!(
                "{} rated {}",
                statistics.rated_days,
                if statistics.rated_days == 1 {
                    "day"
                } else {
                    "days"
                }
            )
        }
        Feature::Judgements => {
            let count = app.document.judgements.len();
            format!(
                "{count} {}",
                if count == 1 {
                    "judgement"
                } else {
                    "judgements"
                }
            )
        }
    };
    let sync = app.sync_status.label();
    let summary = format!("{sync} · {count}");
    let count_width = display_width(&summary)
        .saturating_add(2)
        .min(inner.width as usize) as u16;
    let [brand_area, count_area] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(count_width)]).areas(inner);

    let compact_tabs = brand_area.width < 76;
    let mut brand = vec![
        Span::styled(
            " WHO / ME ",
            Style::default()
                .fg(theme.background)
                .bg(theme.accent)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        feature_tab(
            if compact_tabs { "1 ID" } else { "1 Identities" },
            app.feature == Feature::Identities,
            theme,
        ),
        Span::raw(" "),
        feature_tab(
            if compact_tabs { "2 CAL" } else { "2 Calendar" },
            app.feature == Feature::Calendar,
            theme,
        ),
        Span::raw(" "),
        feature_tab(
            if compact_tabs {
                "3 STATS"
            } else {
                "3 Statistics"
            },
            app.feature == Feature::Statistics,
            theme,
        ),
        Span::raw(" "),
        feature_tab(
            if compact_tabs {
                "4 JUDGE"
            } else {
                "4 Judgements"
            },
            app.feature == Feature::Judgements,
            theme,
        ),
    ];
    if app.feature == Feature::Identities && !app.query.is_empty() {
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
        Paragraph::new(Line::from(vec![
            Span::styled(
                sync,
                Style::default()
                    .fg(sync_status_color(&app.sync_status, theme))
                    .bg(theme.background),
            ),
            Span::styled(
                format!(" · {count}"),
                Style::default().fg(theme.muted).bg(theme.background),
            ),
        ]))
        .alignment(Alignment::Right),
        count_area,
    );
}

fn feature_tab(label: &str, active: bool, theme: &AppTheme) -> Span<'static> {
    Span::styled(
        format!(" {label} "),
        Style::default()
            .fg(if active {
                theme.background
            } else {
                theme.dark_foreground
            })
            .bg(if active { theme.cyan } else { theme.background })
            .add_modifier(if active {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    )
}

fn render_calendar(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &AppTheme) {
    if area.width < 14 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("Calendar needs a little more room")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            area,
        );
        return;
    }

    let (grid_area, entries_area) = if area.width >= 90 {
        let [grid, gap, entries] = Layout::horizontal([
            Constraint::Percentage(66),
            Constraint::Length(1),
            Constraint::Min(26),
        ])
        .areas(area);
        let _ = gap;
        (grid, entries)
    } else {
        let grid_height = area.height.clamp(9, 16);
        let [grid, gap, entries] = Layout::vertical([
            Constraint::Length(grid_height),
            Constraint::Length(1),
            Constraint::Min(3),
        ])
        .areas(area);
        let _ = gap;
        (grid, entries)
    };
    render_month_grid(frame, grid_area, app, theme);
    render_day_entries(frame, entries_area, app, theme);
}

fn render_month_grid(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let [title_area, weekdays_area, days_area] = Layout::vertical([
        Constraint::Length(2),
        Constraint::Length(1),
        Constraint::Min(6),
    ])
    .areas(area);
    let month_name = app.displayed_month.format("%B %Y").to_string();
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" [ ", Style::default().fg(theme.accent)),
            Span::styled("Previous", Style::default().fg(theme.muted)),
            Span::styled("    ", Style::default().fg(theme.accent)),
            Span::styled(
                month_name,
                Style::default()
                    .fg(theme.bright_foreground)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled("    ", Style::default().fg(theme.accent)),
            Span::styled("Next", Style::default().fg(theme.muted)),
            Span::styled(" ]", Style::default().fg(theme.accent)),
        ]))
        .alignment(Alignment::Center)
        .style(Style::default().bg(theme.background)),
        title_area,
    );

    let columns = Layout::horizontal([Constraint::Ratio(1, 7); 7]).split(weekdays_area);
    for (column, label) in columns
        .iter()
        .zip(["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"])
    {
        frame.render_widget(
            Paragraph::new(label)
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            *column,
        );
    }

    let rows = Layout::vertical([Constraint::Ratio(1, 6); 6]).split(days_area);
    let offset = app.displayed_month.weekday().num_days_from_monday() as usize;
    let total_days = days_in_month(app.displayed_month) as usize;
    for (slot, cell) in rows
        .iter()
        .flat_map(|row| {
            Layout::horizontal([Constraint::Ratio(1, 7); 7])
                .split(*row)
                .to_vec()
        })
        .enumerate()
    {
        let Some(day_number) = slot
            .checked_sub(offset)
            .map(|day| day + 1)
            .filter(|day| *day <= total_days)
        else {
            continue;
        };
        let date = app
            .displayed_month
            .with_day(day_number as u32)
            .expect("calendar day is valid");
        render_calendar_day(frame, cell, app, date, theme);
    }
}

fn render_calendar_day(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    date: NaiveDate,
    theme: &AppTheme,
) {
    let selected = date == app.selected_date;
    let today = date == app.today;
    let focused = selected && app.calendar_focus == CalendarFocus::Grid;
    let background = if selected {
        theme.selection
    } else {
        theme.dark_background
    };
    let border = if focused {
        theme.accent
    } else if today {
        theme.green
    } else if let Some(mood) = app.document.calendar_day(date).and_then(|day| day.mood) {
        mood_color(mood, theme)
    } else {
        theme.muted
    };
    let day = app.document.calendar_day(date);
    let entries = day.map(|day| day.entries.as_slice()).unwrap_or(&[]);
    let bordered = area.height >= 6 && area.width >= 7;
    let block = if bordered {
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(border).bg(background))
            .style(Style::default().bg(background))
    } else {
        Block::default().style(Style::default().bg(background))
    };
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if inner.is_empty() {
        return;
    }
    let date_text = date.day().to_string();
    let mut heading = vec![Span::styled(
        date_text.clone(),
        Style::default()
            .fg(if today {
                theme.green
            } else {
                theme.bright_foreground
            })
            .bg(background)
            .add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    )];
    if let Some(mood) = day.and_then(|day| day.mood) {
        let mood_width = (inner.width as usize).saturating_sub(display_width(&date_text));
        heading.push(Span::styled(
            truncate_to_width(&format!(" · {} {}", mood.value(), mood.label()), mood_width),
            Style::default().fg(mood_color(mood, theme)).bg(background),
        ));
    }
    let mut lines = vec![Line::from(heading)];
    let entry_width = inner.width.saturating_sub(2) as usize;
    for entry in entries.iter().take(inner.height.saturating_sub(1) as usize) {
        let marker = if entry.done { "✓ " } else { "○ " };
        let mut text_style = Style::default()
            .fg(if entry.done {
                theme.dark_foreground
            } else {
                theme.foreground
            })
            .bg(background);
        if entry.done {
            text_style = text_style.add_modifier(Modifier::CROSSED_OUT);
        }
        lines.push(Line::from(vec![
            Span::styled(
                marker,
                Style::default()
                    .fg(if entry.done { theme.green } else { theme.muted })
                    .bg(background),
            ),
            Span::styled(truncate_to_width(&entry.text, entry_width), text_style),
        ]));
    }
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(background)),
        inner,
    );
}

fn render_day_entries(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &AppTheme) {
    if area.is_empty() {
        return;
    }
    let focused = app.calendar_focus == CalendarFocus::Entries;
    let mood = app
        .document
        .calendar_day(app.selected_date)
        .and_then(|day| day.mood)
        .map(|mood| format!("{} {}", mood.value(), mood.label()))
        .unwrap_or_else(|| "Unrated".into());
    let title = format!(
        " {} · {} · {} ",
        app.selected_date.format("%A, %B %-d"),
        mood,
        app.selected_calendar_entries().len()
    );
    let block = Block::default()
        .title(title)
        .title_style(
            Style::default()
                .fg(if focused { theme.accent } else { theme.muted })
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .padding(Padding::horizontal(1))
        .style(Style::default().bg(theme.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let entry_count = app.selected_calendar_entries().len();
    if entry_count == 0 {
        frame.render_widget(
            Paragraph::new("Press a to add the first entry")
                .style(Style::default().fg(theme.dark_foreground).bg(theme.panel)),
            inner,
        );
        app.calendar_scroll = 0;
        return;
    }
    let visible_height = inner.height as usize;
    if let Some(selected) = app.selected_calendar_entry {
        if selected < app.calendar_scroll as usize {
            app.calendar_scroll = selected as u16;
        } else if selected >= app.calendar_scroll as usize + visible_height {
            app.calendar_scroll = selected.saturating_add(1).saturating_sub(visible_height) as u16;
        }
    }
    app.calendar_scroll = app
        .calendar_scroll
        .min(entry_count.saturating_sub(visible_height) as u16);
    let lines = app
        .selected_calendar_entries()
        .iter()
        .enumerate()
        .skip(app.calendar_scroll as usize)
        .take(visible_height)
        .map(|(index, entry)| {
            let selected = focused && app.selected_calendar_entry == Some(index);
            let background = if selected {
                theme.selection
            } else {
                theme.panel
            };
            Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(theme.accent).bg(background),
                ),
                Span::styled(
                    if entry.done { "✓ " } else { "○ " },
                    Style::default()
                        .fg(if entry.done { theme.green } else { theme.muted })
                        .bg(background),
                ),
                Span::styled(
                    pad_to_width(
                        &truncate_to_width(&entry.text, inner.width.saturating_sub(4) as usize),
                        inner.width.saturating_sub(4) as usize,
                    ),
                    Style::default()
                        .fg(if entry.done {
                            theme.dark_foreground
                        } else {
                            theme.foreground
                        })
                        .bg(background)
                        .add_modifier(if entry.done {
                            Modifier::CROSSED_OUT
                        } else {
                            Modifier::empty()
                        }),
                ),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.panel)),
        inner,
    );
}

fn render_statistics(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    if area.width < 20 || area.height < 8 {
        frame.render_widget(
            Paragraph::new("Statistics needs a little more room")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            area,
        );
        return;
    }

    let outer = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    let block = Block::default()
        .title(format!(
            " Mood statistics · {} ",
            app.statistics_period.label()
        ))
        .title_style(
            Style::default()
                .fg(theme.accent)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.accent))
        .padding(Padding::new(2, 2, 1, 1))
        .style(Style::default().bg(theme.panel));
    let inner = block.inner(outer);
    frame.render_widget(block, outer);

    let statistics = app.mood_statistics();
    let selected_period = app.statistics_period;
    let period_line = Line::from(vec![
        statistics_tab(
            "m",
            "Last 30 days",
            selected_period == StatisticsPeriod::Month,
            theme,
        ),
        Span::raw("  "),
        statistics_tab(
            "y",
            "Last 365 days",
            selected_period == StatisticsPeriod::Year,
            theme,
        ),
        Span::raw("  "),
        statistics_tab(
            "f",
            "Forever",
            selected_period == StatisticsPeriod::Forever,
            theme,
        ),
    ]);
    let mut lines = vec![period_line, Line::from("")];
    let Some(average) = statistics.average() else {
        lines.extend([
            Line::from(Span::styled(
                "No mood ratings in this period yet",
                Style::default()
                    .fg(theme.bright_foreground)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Open Calendar, select a day, and press r to rate it.",
                Style::default().fg(theme.muted),
            )),
        ]);
        frame.render_widget(
            Paragraph::new(lines)
                .wrap(Wrap { trim: true })
                .style(Style::default().bg(theme.panel)),
            inner,
        );
        return;
    };

    lines.extend([
        Line::from(vec![
            Span::styled(
                format!("{average:.2}"),
                Style::default()
                    .fg(mood_average_color(average, theme))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(" / 5 average", Style::default().fg(theme.foreground)),
            Span::styled(
                format!(
                    "   ·   {} rated {}",
                    statistics.rated_days,
                    if statistics.rated_days == 1 {
                        "day"
                    } else {
                        "days"
                    }
                ),
                Style::default().fg(theme.muted),
            ),
        ]),
        Line::from(""),
    ]);
    let label_width = 14usize;
    let reserved = label_width.saturating_add(14);
    let bar_width = (inner.width as usize).saturating_sub(reserved).max(1);
    for mood in MoodRating::ALL.into_iter().rev() {
        let count = statistics.counts[mood.value() as usize - 1];
        let percentage = count * 100 / statistics.rated_days;
        let filled = count * bar_width / statistics.rated_days;
        lines.push(Line::from(vec![
            Span::styled(
                format!("{} {:<10}", mood.value(), mood.label()),
                Style::default().fg(mood_color(mood, theme)),
            ),
            Span::styled(
                "█".repeat(filled),
                Style::default().fg(mood_color(mood, theme)),
            ),
            Span::styled(
                "░".repeat(bar_width.saturating_sub(filled)),
                Style::default().fg(theme.muted),
            ),
            Span::styled(
                format!("  {count:>3}  {percentage:>3}%"),
                Style::default().fg(theme.foreground),
            ),
        ]));
        lines.push(Line::from(""));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme.panel))
            .wrap(Wrap { trim: false }),
        inner,
    );
}

fn statistics_tab(key: &str, label: &str, selected: bool, theme: &AppTheme) -> Span<'static> {
    Span::styled(
        format!(" {key} {label} "),
        Style::default()
            .fg(if selected {
                theme.background
            } else {
                theme.muted
            })
            .bg(if selected { theme.accent } else { theme.panel })
            .add_modifier(if selected {
                Modifier::BOLD
            } else {
                Modifier::empty()
            }),
    )
}

fn mood_color(mood: MoodRating, theme: &AppTheme) -> ratatui::style::Color {
    match mood.value() {
        1 => theme.red,
        2 => theme.yellow,
        3 => theme.muted,
        4 => theme.cyan,
        5 => theme.green,
        _ => theme.muted,
    }
}

fn mood_average_color(average: f64, theme: &AppTheme) -> ratatui::style::Color {
    let rounded = average.round().clamp(1.0, 5.0) as u8;
    mood_color(
        MoodRating::try_from(rounded).expect("average is clamped to mood range"),
        theme,
    )
}

fn render_judgements(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &AppTheme) {
    if area.width < 24 || area.height < 10 {
        frame.render_widget(
            Paragraph::new("Judgements needs a little more room")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.background)),
            area,
        );
        return;
    }
    if app.document.judgements.is_empty() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(Span::styled(
                    "Compare what you expected with what you learned",
                    Style::default()
                        .fg(theme.bright_foreground)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press n to create your first judgement",
                    Style::default().fg(theme.muted),
                )),
            ])
            .alignment(Alignment::Center)
            .block(Block::default().padding(Padding::top(area.height.saturating_sub(3) / 2)))
            .style(Style::default().bg(theme.background)),
            area,
        );
        app.judgement_scroll = 0;
        app.characteristic_scroll = 0;
        return;
    }

    let outer = Rect::new(
        area.x.saturating_add(1),
        area.y.saturating_add(1),
        area.width.saturating_sub(2),
        area.height.saturating_sub(2),
    );
    if area.width >= 100 {
        let [list_area, detail_area] = Layout::horizontal([
            Constraint::Length((outer.width / 4).clamp(24, 34)),
            Constraint::Min(50),
        ])
        .spacing(1)
        .areas(outer);
        render_judgement_list(frame, list_area, app, theme);
        let [summary_area, table_area] =
            Layout::vertical([Constraint::Length(7), Constraint::Min(4)])
                .spacing(1)
                .areas(detail_area);
        render_judgement_summary(frame, summary_area, app, theme);
        render_characteristic_table(frame, table_area, app, theme);
    } else {
        let list_height = (outer.height / 4).clamp(4, 7);
        let [list_area, detail_area] =
            Layout::vertical([Constraint::Length(list_height), Constraint::Min(5)])
                .spacing(1)
                .areas(outer);
        render_judgement_list(frame, list_area, app, theme);
        let summary_height = detail_area.height.min(7);
        let [summary_area, characteristic_area] =
            Layout::vertical([Constraint::Length(summary_height), Constraint::Min(0)])
                .areas(detail_area);
        render_judgement_summary(frame, summary_area, app, theme);
        render_characteristic_stack(frame, characteristic_area, app, theme);
    }
}

fn render_judgement_list(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &AppTheme) {
    let focused = app.judgement_focus == JudgementFocus::Judgements;
    let block = Block::default()
        .title(" Judgements ")
        .title_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .padding(Padding::horizontal(1))
        .style(Style::default().bg(theme.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let visible = inner.height as usize;
    if app.selected_judgement < app.judgement_scroll as usize {
        app.judgement_scroll = app.selected_judgement as u16;
    } else if app.selected_judgement >= app.judgement_scroll as usize + visible {
        app.judgement_scroll = app
            .selected_judgement
            .saturating_add(1)
            .saturating_sub(visible) as u16;
    }
    app.judgement_scroll = app
        .judgement_scroll
        .min(app.document.judgements.len().saturating_sub(visible) as u16);
    let lines = app
        .document
        .judgements
        .iter()
        .enumerate()
        .skip(app.judgement_scroll as usize)
        .take(visible)
        .map(|(index, judgement)| {
            let selected = index == app.selected_judgement;
            let background = if selected {
                theme.selection
            } else {
                theme.panel
            };
            Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(theme.accent).bg(background),
                ),
                Span::styled(
                    pad_to_width(
                        &truncate_to_width(&judgement.name, inner.width.saturating_sub(2) as usize),
                        inner.width.saturating_sub(2) as usize,
                    ),
                    Style::default()
                        .fg(if selected {
                            theme.bright_foreground
                        } else {
                            theme.foreground
                        })
                        .bg(background)
                        .add_modifier(if selected {
                            Modifier::BOLD
                        } else {
                            Modifier::empty()
                        }),
                ),
            ])
        })
        .collect::<Vec<_>>();
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.panel)),
        inner,
    );
}

fn render_judgement_summary(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let Some(judgement) = app.selected_judgement() else {
        return;
    };
    let statistics = app.judgement_statistics();
    let block = Block::default()
        .title(format!(" {} ", judgement.name))
        .title_style(
            Style::default()
                .fg(theme.bright_foreground)
                .add_modifier(Modifier::BOLD),
        )
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(theme.muted))
        .padding(Padding::horizontal(1))
        .style(Style::default().bg(theme.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let context = if judgement.follow_up.is_empty() {
        "Follow-up not specified"
    } else {
        judgement.follow_up.as_str()
    };
    let lines = vec![
        Line::from(Span::styled(
            truncate_to_width(context, inner.width as usize),
            Style::default().fg(theme.muted),
        )),
        sentiment_distribution_line(
            "Before",
            statistics.before_counts,
            statistics.characteristics,
            theme,
        ),
        sentiment_distribution_line(
            "After ",
            statistics.after_counts,
            statistics.verified,
            theme,
        ),
        Line::from(Span::styled(
            format!(
                "{} of {} verified",
                statistics.verified, statistics.characteristics
            ),
            Style::default().fg(theme.dark_foreground),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme.panel))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn sentiment_distribution_line(
    label: &str,
    counts: [usize; 3],
    total: usize,
    theme: &AppTheme,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{label}  "),
        Style::default().fg(theme.foreground),
    )];
    for (index, sentiment) in Sentiment::ALL.into_iter().enumerate() {
        let count = counts[index];
        let percentage = (total > 0).then(|| count * 100 / total);
        spans.push(Span::styled(
            match percentage {
                Some(percentage) => {
                    format!(
                        "{} {} {count} ({percentage}%)  ",
                        sentiment.symbol(),
                        sentiment.label()
                    )
                }
                None => format!("{} {} 0 (—)  ", sentiment.symbol(), sentiment.label()),
            },
            Style::default().fg(sentiment_color(sentiment, theme)),
        ));
    }
    Line::from(spans)
}

fn render_characteristic_table(frame: &mut Frame<'_>, area: Rect, app: &mut App, theme: &AppTheme) {
    let focused = app.judgement_focus == JudgementFocus::Characteristics;
    let count = app
        .selected_judgement()
        .map_or(0, |judgement| judgement.characteristics.len());
    let block = Block::default()
        .title(format!(" Characteristics · {count} "))
        .title_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .style(Style::default().bg(theme.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if count == 0 {
        frame.render_widget(
            Paragraph::new("No characteristics yet · press a to add one")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.panel)),
            inner,
        );
        app.characteristic_scroll = 0;
        return;
    }
    let visible = inner.height.saturating_sub(1) as usize;
    let selected = app.selected_characteristic.unwrap_or(0).min(count - 1);
    if selected < app.characteristic_scroll as usize {
        app.characteristic_scroll = selected as u16;
    } else if selected >= app.characteristic_scroll as usize + visible {
        app.characteristic_scroll = selected.saturating_add(1).saturating_sub(visible) as u16;
    }
    app.characteristic_scroll = app
        .characteristic_scroll
        .min(count.saturating_sub(visible) as u16);
    let judgement = &app.document.judgements[app.selected_judgement];
    let rows = judgement
        .characteristics
        .iter()
        .enumerate()
        .skip(app.characteristic_scroll as usize)
        .take(visible)
        .map(|(index, characteristic)| {
            let selected = index == selected;
            let background = if selected {
                theme.selection
            } else {
                theme.panel
            };
            let after_text = characteristic
                .after
                .as_ref()
                .map_or("Not verified", |after| after.text.as_str());
            let after_rating = characteristic
                .after
                .as_ref()
                .map(|after| after.rating.label())
                .unwrap_or("—");
            Row::new(vec![
                Cell::from(format!(
                    "{}{}",
                    if selected { "› " } else { "  " },
                    characteristic.name
                )),
                Cell::from(characteristic.before.text.as_str()),
                Cell::from(characteristic.before.rating.label()).style(
                    Style::default()
                        .fg(sentiment_color(characteristic.before.rating, theme))
                        .bg(background),
                ),
                Cell::from(after_text),
                Cell::from(after_rating).style(
                    Style::default()
                        .fg(characteristic
                            .after
                            .as_ref()
                            .map_or(theme.muted, |after| sentiment_color(after.rating, theme)))
                        .bg(background),
                ),
            ])
            .style(Style::default().fg(theme.foreground).bg(background))
        });
    let header = Row::new(["Characteristic", "Before", "Rating", "After", "Rating"])
        .style(
            Style::default()
                .fg(theme.accent)
                .bg(theme.panel)
                .add_modifier(Modifier::BOLD),
        )
        .bottom_margin(0);
    frame.render_widget(
        Table::new(
            rows,
            [
                Constraint::Percentage(18),
                Constraint::Percentage(27),
                Constraint::Length(10),
                Constraint::Percentage(27),
                Constraint::Length(10),
            ],
        )
        .header(header)
        .column_spacing(1)
        .style(Style::default().bg(theme.panel)),
        inner,
    );
}

fn render_characteristic_stack(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    if area.is_empty() {
        return;
    }
    let count = app
        .selected_judgement()
        .map_or(0, |judgement| judgement.characteristics.len());
    let focused = app.judgement_focus == JudgementFocus::Characteristics;
    let title = app.selected_characteristic.map_or_else(
        || format!(" Characteristics · {count} "),
        |selected| format!(" Characteristic {}/{} ", selected + 1, count),
    );
    let block = Block::default()
        .title(title)
        .title_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if focused { theme.accent } else { theme.muted }))
        .padding(Padding::horizontal(1))
        .style(Style::default().bg(theme.panel));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(characteristic) = app.selected_characteristic() else {
        frame.render_widget(
            Paragraph::new("No characteristics yet · press a to add one")
                .alignment(Alignment::Center)
                .style(Style::default().fg(theme.muted).bg(theme.panel)),
            inner,
        );
        return;
    };
    let mut lines = vec![
        Line::from(Span::styled(
            characteristic.name.clone(),
            Style::default()
                .fg(theme.bright_foreground)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("Before  ", Style::default().fg(theme.accent)),
            Span::styled(
                characteristic.before.rating.label(),
                Style::default().fg(sentiment_color(characteristic.before.rating, theme)),
            ),
        ]),
        Line::from(characteristic.before.text.clone()),
        Line::from(""),
    ];
    if let Some(after) = &characteristic.after {
        lines.extend([
            Line::from(vec![
                Span::styled("After   ", Style::default().fg(theme.accent)),
                Span::styled(
                    after.rating.label(),
                    Style::default().fg(sentiment_color(after.rating, theme)),
                ),
            ]),
            Line::from(after.text.clone()),
        ]);
    } else {
        lines.push(Line::from(Span::styled(
            "After   Not verified · press v",
            Style::default().fg(theme.muted),
        )));
    }
    frame.render_widget(
        Paragraph::new(lines)
            .style(Style::default().bg(theme.panel))
            .wrap(Wrap { trim: true }),
        inner,
    );
}

fn sentiment_color(sentiment: Sentiment, theme: &AppTheme) -> ratatui::style::Color {
    match sentiment {
        Sentiment::Positive => theme.green,
        Sentiment::Neutral => theme.muted,
        Sentiment::Negative => theme.red,
    }
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
    let hints = footer_hints(app, hint_area.width);
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

fn footer_hints(app: &App, width: u16) -> &'static str {
    match (&app.mode, app.feature) {
        (Mode::Normal, Feature::Calendar) if width >= 100 => {
            "↑↓←→ days  Tab grid/list  [ ] month  r rate  a add  ↵ edit  Space check  3 statistics"
        }
        (Mode::Normal, Feature::Calendar) if width >= 65 => {
            "↑↓←→ days  Tab focus  [ ] month  r rate  a add  3 statistics"
        }
        (Mode::Normal, Feature::Calendar) => "[ ] month  r rate  a add  3 stats  ? help",
        (Mode::Normal, Feature::Statistics) => {
            "m 30 days  y 365 days  f forever  2 calendar  ? help  q quit"
        }
        (Mode::Normal, Feature::Judgements) if width >= 95 => {
            "↑↓ navigate  Tab focus  n new  a add characteristic  v verify  ↵ edit  d delete  Ctrl+↑↓ reorder"
        }
        (Mode::Normal, Feature::Judgements) if width >= 60 => {
            "↑↓ navigate  Tab focus  n new  a add  v verify  ↵ edit  d delete"
        }
        (Mode::Normal, Feature::Judgements) => "n new  a add  v verify  1–4 switch  ? help",
        (Mode::Normal, Feature::Identities) if width >= 105 => {
            "↑↓ rows  ←→ identities  t new  a add  s status  ↵ edit  Space check  / search  2 calendar"
        }
        (Mode::Normal, Feature::Identities) if width >= 80 => {
            "↑↓ navigate  t new  a add  s status  ↵ edit  / search  2 calendar"
        }
        (Mode::Normal, Feature::Identities) if width >= 58 => {
            "↑↓←→ navigate  t new  s status  / search  2 calendar"
        }
        (Mode::Normal, Feature::Identities) => "t new  2 calendar  ? help  q quit",
        (Mode::Editing(_), _) => "↵ save  Esc cancel  ←→ cursor",
        (Mode::Searching(_), _) => "type to filter  ↵ keep  Esc clear",
        (Mode::SelectingStatus(_), _) => "↑↓ choose  ↵ save  Esc cancel",
        (Mode::SelectingMood(_), _) => "1–5 save  ↑↓ choose  c clear  Esc cancel",
        (Mode::EditingJudgement(_), _) => "Tab fields  ↵ save  Esc cancel",
        (Mode::EditingCharacteristic(_), _) => {
            "Tab fields  +/0/- rate  Ctrl+x clear after  ↵ save  Esc cancel"
        }
        (Mode::ConfirmDelete(_), _) => "↵ / y confirm  Esc / n cancel",
        (Mode::Settings(settings), _) if settings.editing => "↵ connect  Esc cancel  ←→ cursor",
        (Mode::Settings(_), _) => "e edit  r sync  x disconnect  g / Esc close",
        (Mode::Help, _) => "Esc / ? close",
    }
}

fn sync_status_color(status: &SyncStatus, theme: &AppTheme) -> ratatui::style::Color {
    match status {
        SyncStatus::Synced => theme.green,
        SyncStatus::Connecting | SyncStatus::Syncing | SyncStatus::Pending => theme.yellow,
        SyncStatus::Offline(_) | SyncStatus::LocalOnly => theme.muted,
        SyncStatus::Error(_) | SyncStatus::Conflict(_) => theme.red,
    }
}

fn render_settings(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    settings: &SettingsState,
    theme: &AppTheme,
) {
    let popup = centered_rect(area, 78, 20);
    frame.render_widget(Clear, popup);
    let block = overlay_block(" GitHub sync settings ", theme.accent, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let label_style = Style::default().fg(theme.muted).bg(theme.panel);
    let value_style = Style::default().fg(theme.foreground).bg(theme.panel);
    let status_style = Style::default()
        .fg(sync_status_color(&app.sync_status, theme))
        .bg(theme.panel)
        .add_modifier(Modifier::BOLD);
    let repository = if settings.repository.is_empty() {
        "Not configured"
    } else {
        settings.repository.as_str()
    };
    let detail = app
        .sync_status
        .detail()
        .unwrap_or("Local saves always continue, including while GitHub is unavailable.");
    let mut lines = vec![
        Line::from(vec![
            Span::styled("Status      ", label_style),
            Span::styled(app.sync_status.label(), status_style),
        ]),
        Line::from(vec![
            Span::styled("Repository  ", label_style),
            Span::styled(
                truncate_to_width(repository, inner.width.saturating_sub(12) as usize),
                value_style,
            ),
        ]),
        Line::from(vec![
            Span::styled("Branch      ", label_style),
            Span::styled(app.sync_branch.as_deref().unwrap_or("—"), value_style),
        ]),
        Line::from(""),
        Line::from(Span::styled(
            truncate_to_width(detail, inner.width as usize),
            Style::default().fg(theme.dark_foreground).bg(theme.panel),
        )),
        Line::from(""),
    ];

    if settings.editing {
        let editor = Editor {
            kind: EditKind::Search,
            input: settings.repository.clone(),
            cursor: settings.cursor,
        };
        let (visible, _) = visible_input(&editor, inner.width.saturating_sub(1) as usize);
        lines.extend([
            Line::from(Span::styled(
                "GitHub HTTPS or SSH URL:",
                Style::default().fg(theme.accent).bg(theme.panel),
            )),
            Line::from(Span::styled(
                pad_to_width(&visible, inner.width as usize),
                Style::default()
                    .fg(theme.bright_foreground)
                    .bg(theme.selection),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "Enter connects · credentials come from Git or SSH",
                label_style,
            )),
        ]);
    } else if settings.confirm_disconnect {
        lines.extend([
            Line::from(Span::styled(
                "Disconnect and archive the local sync clone?",
                Style::default().fg(theme.red).bg(theme.panel),
            )),
            Line::from(""),
            Line::from("Enter / y confirm    Esc / n cancel"),
        ]);
    } else if matches!(app.sync_status, SyncStatus::Conflict(_)) {
        lines.extend([
            Line::from(Span::styled(
                "Both snapshots will be backed up before resolution.",
                label_style,
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled("l", Style::default().fg(theme.accent)),
                Span::raw(" keep Local    "),
                Span::styled("h", Style::default().fg(theme.accent)),
                Span::raw(" use GitHub"),
            ]),
        ]);
    } else {
        lines.extend([
            Line::from("e edit/connect repository URL"),
            Line::from("r synchronize now"),
            Line::from("x disconnect and archive clone"),
            Line::from(""),
            Line::from(Span::styled(
                "The repository must already exist and be dedicated to who-me.",
                label_style,
            )),
        ]);
    }

    frame.render_widget(
        Paragraph::new(lines)
            .style(value_style)
            .wrap(Wrap { trim: true }),
        inner,
    );
    if settings.editing {
        let editor = Editor {
            kind: EditKind::Search,
            input: settings.repository.clone(),
            cursor: settings.cursor,
        };
        let (_, cursor) = visible_input(&editor, inner.width.saturating_sub(1) as usize);
        frame.set_cursor_position(Position::new(
            inner.x.saturating_add(cursor as u16),
            inner.y.saturating_add(7),
        ));
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

fn render_mood_picker(frame: &mut Frame<'_>, area: Rect, picker: MoodPicker, theme: &AppTheme) {
    let popup = centered_rect(area, 52, 13);
    frame.render_widget(Clear, popup);
    let block = overlay_block(" Rate this day ", mood_color(picker.selected, theme), theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let mut lines = MoodRating::ALL
        .map(|mood| {
            let selected = mood == picker.selected;
            let background = if selected {
                theme.selection
            } else {
                theme.panel
            };
            Line::from(vec![
                Span::styled(
                    if selected { "› " } else { "  " },
                    Style::default().fg(mood_color(mood, theme)).bg(background),
                ),
                Span::styled(
                    format!("{}  {:<10}", mood.value(), mood.label()),
                    Style::default()
                        .fg(mood_color(mood, theme))
                        .bg(background)
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
    lines.extend([
        Line::from(""),
        Line::from(Span::styled(
            "1–5 save · ↑↓ choose · c clear · Esc cancel",
            Style::default().fg(theme.muted).bg(theme.panel),
        )),
    ]);
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.panel)),
        inner,
    );
}

fn render_judgement_form(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &JudgementForm,
    app: &App,
    theme: &AppTheme,
) {
    let popup = centered_rect(area, 76, 11);
    frame.render_widget(Clear, popup);
    let title = if form.target.is_some() {
        " Edit judgement "
    } else {
        " New judgement "
    };
    let block = overlay_block(title, theme.accent, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let width = inner.width.saturating_sub(1) as usize;
    let name_editor = Editor {
        kind: EditKind::Search,
        input: form.name.clone(),
        cursor: form.name_cursor,
    };
    let follow_up_editor = Editor {
        kind: EditKind::Search,
        input: form.follow_up.clone(),
        cursor: form.follow_up_cursor,
    };
    let (name, name_cursor) = visible_input(&name_editor, width);
    let (follow_up, follow_up_cursor) = visible_input(&follow_up_editor, width);
    let field_style = |selected| {
        Style::default()
            .fg(theme.bright_foreground)
            .bg(if selected {
                theme.selection
            } else {
                theme.dark_background
            })
    };
    let lines = vec![
        Line::from(Span::styled("Name", Style::default().fg(theme.accent))),
        Line::from(Span::styled(
            pad_to_width(&name, inner.width as usize),
            field_style(form.field == JudgementField::Name),
        )),
        Line::from(Span::styled(
            "Follow-up context (optional)",
            Style::default().fg(theme.accent),
        )),
        Line::from(Span::styled(
            pad_to_width(&follow_up, inner.width as usize),
            field_style(form.field == JudgementField::FollowUp),
        )),
        Line::from(""),
        Line::from(Span::styled(
            "Tab switch field · Enter save · Esc cancel",
            Style::default().fg(theme.muted),
        )),
        Line::from(Span::styled(
            app.status.as_deref().unwrap_or(""),
            Style::default().fg(theme.red),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.panel)),
        inner,
    );
    let (cursor, row) = match form.field {
        JudgementField::Name => (name_cursor, 1),
        JudgementField::FollowUp => (follow_up_cursor, 3),
    };
    frame.set_cursor_position(Position::new(
        inner.x.saturating_add(cursor as u16),
        inner.y.saturating_add(row),
    ));
}

fn render_characteristic_form(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &CharacteristicForm,
    app: &App,
    theme: &AppTheme,
) {
    let popup = centered_rect(area, 84, 18);
    frame.render_widget(Clear, popup);
    let title = if form.target.is_some() {
        " Edit characteristic "
    } else {
        " New characteristic "
    };
    let block = overlay_block(title, theme.accent, theme);
    let inner = block.inner(popup);
    frame.render_widget(block, popup);
    let width = inner.width.saturating_sub(1) as usize;
    let editors = [
        Editor {
            kind: EditKind::Search,
            input: form.name.clone(),
            cursor: form.name_cursor,
        },
        Editor {
            kind: EditKind::Search,
            input: form.before_text.clone(),
            cursor: form.before_cursor,
        },
        Editor {
            kind: EditKind::Search,
            input: form.after_text.clone(),
            cursor: form.after_cursor,
        },
    ];
    let [
        (name, name_cursor),
        (before, before_cursor),
        (after, after_cursor),
    ] = editors.map(|editor| visible_input(&editor, width));
    let input_style = |selected| {
        Style::default()
            .fg(theme.bright_foreground)
            .bg(if selected {
                theme.selection
            } else {
                theme.dark_background
            })
    };
    let lines = vec![
        Line::from(Span::styled(
            "Characteristic",
            Style::default().fg(theme.accent),
        )),
        Line::from(Span::styled(
            pad_to_width(&name, inner.width as usize),
            input_style(form.field == CharacteristicField::Name),
        )),
        Line::from(Span::styled(
            "Before text",
            Style::default().fg(theme.accent),
        )),
        Line::from(Span::styled(
            pad_to_width(&before, inner.width as usize),
            input_style(form.field == CharacteristicField::BeforeText),
        )),
        sentiment_picker_line(
            "Before rating",
            form.before_rating,
            form.field == CharacteristicField::BeforeRating,
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "After text (optional until verified)",
            Style::default().fg(theme.accent),
        )),
        Line::from(Span::styled(
            pad_to_width(&after, inner.width as usize),
            input_style(form.field == CharacteristicField::AfterText),
        )),
        sentiment_picker_line(
            "After rating ",
            form.after_rating,
            form.field == CharacteristicField::AfterRating,
            theme,
        ),
        Line::from(""),
        Line::from(Span::styled(
            "Tab fields · +/0/- rate · Ctrl+x clear after · Enter save · Esc cancel",
            Style::default().fg(theme.muted),
        )),
        Line::from(Span::styled(
            app.status.as_deref().unwrap_or(""),
            Style::default().fg(theme.red),
        )),
    ];
    frame.render_widget(
        Paragraph::new(lines).style(Style::default().bg(theme.panel)),
        inner,
    );
    let cursor = match form.field {
        CharacteristicField::Name => Some((name_cursor, 1)),
        CharacteristicField::BeforeText => Some((before_cursor, 3)),
        CharacteristicField::AfterText => Some((after_cursor, 7)),
        CharacteristicField::BeforeRating | CharacteristicField::AfterRating => None,
    };
    if let Some((column, row)) = cursor {
        frame.set_cursor_position(Position::new(
            inner.x.saturating_add(column as u16),
            inner.y.saturating_add(row),
        ));
    }
}

fn sentiment_picker_line(
    label: &str,
    selected: Sentiment,
    focused: bool,
    theme: &AppTheme,
) -> Line<'static> {
    let mut spans = vec![Span::styled(
        format!("{label}  "),
        Style::default().fg(theme.accent),
    )];
    for sentiment in Sentiment::ALL {
        let active = sentiment == selected;
        spans.push(Span::styled(
            format!(" {} {} ", sentiment.symbol(), sentiment.label()),
            Style::default()
                .fg(if active {
                    theme.background
                } else {
                    sentiment_color(sentiment, theme)
                })
                .bg(if active {
                    sentiment_color(sentiment, theme)
                } else if focused {
                    theme.selection
                } else {
                    theme.panel
                })
                .add_modifier(if active {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        ));
        spans.push(Span::raw(" "));
    }
    Line::from(spans)
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
        EditKind::AddCalendarEntry(_) => " New calendar entry ",
        EditKind::EditCalendarEntry(_, _) => " Edit calendar entry ",
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
        DeleteTarget::CalendarEntry(date, entry) => app
            .document
            .calendar_day(date)
            .and_then(|day| day.entries.get(entry))
            .map(|entry| format!("Delete {:?} from {}?", entry.text, date))
            .unwrap_or_else(|| "Delete this calendar entry?".into()),
        DeleteTarget::Judgement(judgement) => app
            .document
            .judgements
            .get(judgement)
            .map(|judgement| {
                format!(
                    "Delete judgement {:?} and all of its characteristics?",
                    judgement.name
                )
            })
            .unwrap_or_else(|| "Delete this judgement?".into()),
        DeleteTarget::Characteristic(judgement, characteristic) => app
            .document
            .judgements
            .get(judgement)
            .and_then(|judgement| judgement.characteristics.get(characteristic))
            .map(|characteristic| format!("Delete characteristic {:?}?", characteristic.name))
            .unwrap_or_else(|| "Delete this characteristic?".into()),
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

fn render_help(frame: &mut Frame<'_>, area: Rect, app: &App, theme: &AppTheme) {
    let popup = centered_rect(area, 78, 25);
    frame.render_widget(Clear, popup);
    let rows = match app.feature {
        Feature::Identities => [
            ("↑ / ↓", "Move through identity titles and entries"),
            ("← / → or Tab", "Move between identities"),
            ("t / a", "Add an identity / add an entry"),
            ("s", "Choose the identity status"),
            ("Enter / Space", "Edit / check the selected entry"),
            ("d", "Delete with confirmation"),
            ("Ctrl + arrows", "Reorder entries or identities"),
            ("/", "Search identities and entries"),
        ],
        Feature::Calendar => [
            ("Arrow keys", "Move between days or day entries"),
            ("Tab", "Switch focus between grid and checklist"),
            ("[ / ]", "Show the previous / next month"),
            ("r / a", "Rate the day / add an entry"),
            ("Enter / Space", "Edit / check the selected entry"),
            ("d", "Delete with confirmation"),
            ("Ctrl + ↑ / ↓", "Reorder the selected entry"),
            ("Esc", "Return focus to the month grid"),
        ],
        Feature::Statistics => [
            ("m", "Show mood ratings from the last 30 days"),
            ("y", "Show mood ratings from the last 365 days"),
            ("f", "Show all mood ratings through today"),
            ("2", "Open Calendar to add or change ratings"),
            ("1", "Open Identities"),
            ("g", "Open GitHub sync settings"),
            ("?", "Close this keyboard guide"),
            ("q", "Quit"),
        ],
        Feature::Judgements => [
            ("↑ / ↓", "Navigate the focused list"),
            ("Tab", "Switch between judgements and characteristics"),
            ("n / a", "Add a judgement / characteristic"),
            ("v", "Add or update the selected after observation"),
            ("Enter", "Edit the focused judgement or characteristic"),
            ("d", "Delete with confirmation"),
            ("Ctrl + ↑ / ↓", "Reorder the focused list"),
            ("Esc", "Return focus to judgements"),
        ],
    };
    let mut lines = vec![Line::from("")];
    for (key, description) in rows {
        lines.push(Line::from(vec![
            Span::styled(format!("{key:<18}"), Style::default().fg(theme.accent)),
            Span::styled(description, Style::default().fg(theme.foreground)),
        ]));
    }
    lines.extend([
        Line::from(""),
        Line::from(vec![
            Span::styled(
                format!("{:<18}", "1 / 2 / 3 / 4"),
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                "Switch Identities / Calendar / Statistics / Judgements",
                Style::default().fg(theme.foreground),
            ),
        ]),
        Line::from(vec![
            Span::styled(
                format!("{:<18}", "g / q"),
                Style::default().fg(theme.accent),
            ),
            Span::styled(
                "GitHub settings / quit",
                Style::default().fg(theme.foreground),
            ),
        ]),
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
    use crate::model::{
        Calendar, Characteristic, DATA_VERSION, Document, IdentityStatus, Item, Judgement,
        Observation, Sentiment, Topic,
    };
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
            calendar: Calendar::default(),
            judgements: Vec::new(),
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

        app.mode = Mode::Settings(SettingsState {
            repository: "https://github.com/person/private".into(),
            cursor: 33,
            editing: false,
            confirm_disconnect: false,
        });
        app.sync_repository = Some("https://github.com/person/private".into());
        app.sync_status = SyncStatus::Synced;
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        let output = rendered(&terminal);
        assert!(output.contains("GitHub sync settings"));
        assert!(output.contains("Synced"));
    }

    #[test]
    fn renders_calendar_in_wide_and_stacked_layouts() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        for (width, height) in [(120, 32), (70, 28)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = App::new_at(Document::default(), today);
            app.feature = Feature::Calendar;
            app.document.ensure_calendar_day(today).entries.push(Item {
                text: "Submit report".into(),
                done: false,
            });
            app.document.calendar_day_mut(today).unwrap().mood =
                Some(MoodRating::try_from(4).unwrap());
            terminal
                .draw(|frame| render(frame, &mut app, &AppTheme::default()))
                .unwrap();
            let output = rendered(&terminal);
            assert!(output.contains(if width >= 90 { "2 Calendar" } else { "2 CAL" }));
            assert!(output.contains("August 2026"));
            assert!(output.contains("Submit report"));
            assert!(output.contains("4 Good"));
            assert!(output.contains("Mon"));
            assert!(output.contains("Sun"));
        }
    }

    #[test]
    fn renders_mood_picker_and_statistics_periods() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 28).unwrap();
        let backend = TestBackend::new(100, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new_at(Document::default(), today);
        app.document.ensure_calendar_day(today).mood = Some(MoodRating::try_from(5).unwrap());
        app.feature = Feature::Statistics;

        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        let output = rendered(&terminal);
        assert!(output.contains("3 Statistics"));
        assert!(output.contains("Mood statistics"));
        assert!(output.contains("5 Happy"));
        assert!(output.contains("1 rated day"));

        app.feature = Feature::Calendar;
        app.mode = Mode::SelectingMood(MoodPicker {
            date: today,
            selected: MoodRating::try_from(2).unwrap(),
        });
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        let output = rendered(&terminal);
        assert!(output.contains("Rate this day"));
        assert!(output.contains("2  Bad"));
        assert!(output.contains("c clear"));
    }

    #[test]
    fn renders_judgements_as_a_table_and_a_narrow_stack() {
        for (width, height) in [(120, 30), (72, 30)] {
            let backend = TestBackend::new(width, height);
            let mut terminal = Terminal::new(backend).unwrap();
            let mut app = App::new(Document::default());
            app.feature = Feature::Judgements;
            app.document.judgements.push(Judgement {
                name: "New role".into(),
                follow_up: "After one year".into(),
                characteristics: vec![
                    Characteristic {
                        name: "Autonomy".into(),
                        before: Observation {
                            text: "Expected freedom".into(),
                            rating: Sentiment::Positive,
                        },
                        after: Some(Observation {
                            text: "Some constraints".into(),
                            rating: Sentiment::Negative,
                        }),
                    },
                    Characteristic {
                        name: "Learning".into(),
                        before: Observation {
                            text: "Expected growth".into(),
                            rating: Sentiment::Positive,
                        },
                        after: None,
                    },
                ],
            });
            app.selected_characteristic = Some(0);
            app.judgement_focus = JudgementFocus::Characteristics;

            terminal
                .draw(|frame| render(frame, &mut app, &AppTheme::default()))
                .unwrap();
            let output = rendered(&terminal);
            assert!(output.contains(if width >= 100 {
                "4 Judgements"
            } else {
                "4 JUDGE"
            }));
            assert!(output.contains("New role"));
            assert!(output.contains("After one year"));
            assert!(output.contains("Autonomy"));
            assert!(output.contains("Expected freedom"));
            assert!(output.contains("Some constraints"));
            assert!(output.contains("1 of 2 verified"));
            if width >= 100 {
                assert!(output.contains("Not verified"));
            } else {
                assert!(output.contains("Characteristic 1/2"));
            }
        }
    }

    #[test]
    fn renders_judgement_forms_and_empty_state() {
        let backend = TestBackend::new(90, 28);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new(Document::default());
        app.feature = Feature::Judgements;
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        assert!(rendered(&terminal).contains("first judgement"));

        app.mode = Mode::EditingJudgement(JudgementForm {
            target: None,
            name: "Laptop".into(),
            name_cursor: 6,
            follow_up: "After research".into(),
            follow_up_cursor: 14,
            field: JudgementField::FollowUp,
        });
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        let output = rendered(&terminal);
        assert!(output.contains("New judgement"));
        assert!(output.contains("After research"));

        app.mode = Mode::EditingCharacteristic(CharacteristicForm {
            judgement: 0,
            target: None,
            name: "Battery".into(),
            name_cursor: 7,
            before_text: "Expected all day".into(),
            before_cursor: 16,
            before_rating: Sentiment::Positive,
            after_text: String::new(),
            after_cursor: 0,
            after_rating: Sentiment::Neutral,
            has_after: false,
            field: CharacteristicField::BeforeRating,
        });
        terminal
            .draw(|frame| render(frame, &mut app, &AppTheme::default()))
            .unwrap();
        let output = rendered(&terminal);
        assert!(output.contains("New characteristic"));
        assert!(output.contains("Expected all day"));
        assert!(output.contains("Positive"));
    }

    #[test]
    fn month_tiles_show_entry_text_in_compact_rows() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 3).unwrap();
        let backend = TestBackend::new(70, 20);
        let mut terminal = Terminal::new(backend).unwrap();
        let mut app = App::new_at(Document::default(), today);
        let day = app.document.ensure_calendar_day(today);
        day.mood = Some(MoodRating::try_from(4).unwrap());
        day.entries.extend([
            Item {
                text: "Walk dog".into(),
                done: true,
            },
            Item {
                text: "Read book".into(),
                done: false,
            },
        ]);

        terminal
            .draw(|frame| {
                let area = frame.area();
                render_month_grid(frame, area, &app, &AppTheme::default());
            })
            .unwrap();

        let output = rendered(&terminal);
        assert!(output.contains("4 Good"));
        assert!(output.contains("Walk dog"));
    }
}

mod filter_bar;
mod popups;
mod row;
mod row_collector;

use ratatui::{
    Frame,
    layout::Rect,
    style::Style,
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

use crate::state::{AppState, Focus, RepoFilter};

use super::text::{display_width, truncate_to_width};

struct PaneLayout {
    filter_area: Rect,
    secondary_area: Rect,
    list_area: Rect,
}

impl PaneLayout {
    fn compute(area: Rect) -> Self {
        let filter_area = Rect {
            x: area.x,
            y: area.y,
            width: area.width,
            height: 1.min(area.height),
        };
        let secondary_area = Rect {
            x: area.x,
            y: area.y + 1,
            width: area.width,
            height: 1.min(area.height.saturating_sub(1)),
        };
        let list_area = Rect {
            x: area.x,
            y: area.y + 2,
            width: area.width,
            height: area.height.saturating_sub(2),
        };
        Self {
            filter_area,
            secondary_area,
            list_area,
        }
    }
}

pub(super) fn render_repo_popup(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let theme = &state.theme;
    let repos = state.repo_names();
    if repos.is_empty() {
        return;
    }

    let max_name_len = repos.iter().map(|r| display_width(r)).max().unwrap_or(3);
    // Width: padding(1 left + 1 right) + name + borders(2)
    let popup_width = (max_name_len + 4).min(area.width as usize).max(10) as u16;
    let popup_height = (repos.len() as u16 + 2).min(area.height.saturating_sub(2)); // +2 for borders

    // Right-aligned, below the 2-row header
    let popup_x = area.x + area.width.saturating_sub(popup_width);
    let popup_y = area.y + 2;

    let popup_rect = Rect::new(popup_x, popup_y, popup_width, popup_height);
    state.popup.set_repo_area(Some(popup_rect));

    frame.render_widget(Clear, popup_rect);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.accent));
    let inner = block.inner(popup_rect);
    frame.render_widget(block, popup_rect);

    let inner_width = inner.width as usize;
    for (i, name) in repos.iter().enumerate() {
        if i >= inner.height as usize {
            break;
        }

        let is_highlighted = i == state.repo_popup_selected();
        let is_current = match &state.global.repo_filter {
            RepoFilter::All => i == 0,
            RepoFilter::Repo(n) => *n == *name,
        };

        let truncated = truncate_to_width(name, inner_width.saturating_sub(1));
        let text = format!(" {}", truncated);
        let text_dw = display_width(&text);
        let padding = " ".repeat(inner_width.saturating_sub(text_dw));

        let style = if is_highlighted {
            Style::default()
                .fg(theme.text_active)
                .bg(theme.selection_bg)
        } else if is_current {
            Style::default().fg(theme.text_active)
        } else {
            Style::default().fg(theme.text_muted)
        };

        let line_rect = Rect::new(inner.x, inner.y + i as u16, inner.width, 1);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                format!("{}{}", text, padding),
                style,
            ))),
            line_rect,
        );
    }
}

fn render_filter_bar_into(frame: &mut Frame, state: &AppState, area: Rect) {
    let line = filter_bar::render_filter_bar(state);
    frame.render_widget(Paragraph::new(vec![line]), area);
}

fn render_secondary_header_into(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let (line, notices_btn_col, repo_btn_col) =
        filter_bar::render_secondary_header(state, area.width);
    state.notices.button_col = notices_btn_col;
    state.layout.repo_button_col = repo_btn_col;
    frame.render_widget(Paragraph::new(vec![line]), area);
}

fn compute_scroll_offset(state: &mut AppState, total_lines: usize, list_area: Rect) -> usize {
    state.scrolls.panes.total_lines = total_lines;
    state.scrolls.panes.visible_height = list_area.height as usize;

    // Auto-scroll to keep selected agent visible
    if state.focus_state.sidebar_focused && state.focus_state.focus == Focus::Panes {
        let mut first_line: Option<usize> = None;
        let mut last_line: Option<usize> = None;
        for (i, mapping) in state.layout.line_to_row.iter().enumerate() {
            if *mapping == Some(state.global.selected_pane_row) {
                if first_line.is_none() {
                    first_line = Some(i);
                }
                last_line = Some(i);
            }
        }
        if let (Some(first), Some(last)) = (first_line, last_line) {
            let visible_h = list_area.height as usize;
            let offset = state.scrolls.panes.offset;
            if first < offset {
                state.scrolls.panes.offset = first.saturating_sub(1);
            } else if last >= offset + visible_h {
                state.scrolls.panes.offset = (last + 1).saturating_sub(visible_h);
            }
        }
    }

    state.scrolls.panes.offset
}

fn render_pane_rows(
    frame: &mut Frame,
    lines: Vec<Line<'static>>,
    scroll_offset: usize,
    list_area: Rect,
) {
    let paragraph = Paragraph::new(lines).scroll((scroll_offset as u16, 0));
    frame.render_widget(paragraph, list_area);
}

fn render_flash_banner_into(frame: &mut Frame, state: &mut AppState, area: Rect) {
    // Render flash banner (spawn / remove feedback) before popups so
    // popups stay on top.
    if let Some(text) = state.take_flash() {
        let flash_y = area.y + area.height.saturating_sub(1);
        let flash_rect = Rect::new(area.x, flash_y, area.width, 1);
        frame.render_widget(Clear, flash_rect);
        let theme = &state.theme;
        let color = if text.contains("failed") {
            theme.status_error
        } else {
            theme.accent
        };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(text, Style::default().fg(color)))),
            flash_rect,
        );
    }
}

pub fn draw_agents(frame: &mut Frame, state: &mut AppState, area: Rect) {
    let layout = PaneLayout::compute(area);
    render_filter_bar_into(frame, state, layout.filter_area);
    render_secondary_header_into(frame, state, layout.secondary_area);

    let row_collector::CollectedRows { lines, line_to_row } =
        row_collector::collect(state, layout.list_area.width);
    state.layout.line_to_row = line_to_row;
    let scroll_offset = compute_scroll_offset(state, lines.len(), layout.list_area);
    let _ = scroll_offset;
    render_pane_rows(frame, lines, scroll_offset, layout.list_area);

    render_flash_banner_into(frame, state, area);
    popups::render_if_open(frame, state, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pane_layout_splits_area_into_filter_secondary_list() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 20,
        };
        let layout = PaneLayout::compute(area);
        assert_eq!(layout.filter_area.x, 0);
        assert_eq!(layout.filter_area.y, 0);
        assert_eq!(layout.filter_area.width, 40);
        assert_eq!(layout.filter_area.height, 1);
        assert_eq!(layout.secondary_area.y, 1);
        assert_eq!(layout.secondary_area.height, 1);
        assert_eq!(layout.list_area.y, 2);
        assert_eq!(layout.list_area.height, 18);
        assert_eq!(layout.list_area.width, 40);
    }

    #[test]
    fn pane_layout_handles_tiny_area() {
        // Only 1 row available — filter gets it, secondary and list collapse to 0.
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 1,
        };
        let layout = PaneLayout::compute(area);
        assert_eq!(layout.filter_area.height, 1);
        assert_eq!(layout.secondary_area.height, 0);
        assert_eq!(layout.list_area.height, 0);
    }

    #[test]
    fn pane_layout_handles_zero_height() {
        let area = Rect {
            x: 0,
            y: 0,
            width: 40,
            height: 0,
        };
        let layout = PaneLayout::compute(area);
        assert_eq!(layout.filter_area.height, 0);
        assert_eq!(layout.secondary_area.height, 0);
        assert_eq!(layout.list_area.height, 0);
    }

    #[test]
    fn pane_layout_respects_non_zero_origin() {
        let area = Rect {
            x: 5,
            y: 10,
            width: 30,
            height: 15,
        };
        let layout = PaneLayout::compute(area);
        assert_eq!(layout.filter_area.x, 5);
        assert_eq!(layout.filter_area.y, 10);
        assert_eq!(layout.secondary_area.x, 5);
        assert_eq!(layout.secondary_area.y, 11);
        assert_eq!(layout.list_area.x, 5);
        assert_eq!(layout.list_area.y, 12);
        assert_eq!(layout.list_area.height, 13);
    }
}

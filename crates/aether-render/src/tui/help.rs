//! Help overlay showing all keybindings.
//!
//! Toggled with `?` key. When visible, any key press dismisses it.

use ratatui::buffer::Buffer;
use ratatui::layout::{Alignment, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::palette::Palette;

/// Floating help popup displaying keybinding reference.
#[derive(Debug, Default)]
pub(crate) struct HelpOverlay {
    /// Whether the overlay is currently shown.
    visible: bool,
}

impl HelpOverlay {
    /// Toggle visibility on/off.
    pub(crate) fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Whether the overlay is currently visible.
    pub(crate) fn is_visible(&self) -> bool {
        self.visible
    }

    /// Dismiss the overlay (set visible to false).
    pub(crate) fn dismiss(&mut self) {
        self.visible = false;
    }

    /// Render the help overlay centered within `area`.
    ///
    /// Uses `Clear` to wipe the region, then draws a bordered popup
    /// with a two-column keybinding reference.
    pub(crate) fn render(&self, area: Rect, buf: &mut Buffer) {
        if !self.visible {
            return;
        }

        let popup = centered_rect(area, 60, 70);

        // Clear the popup region.
        Clear.render_ref(popup, buf);

        // Fill background.
        for y in popup.y..popup.bottom() {
            for x in popup.x..popup.right() {
                buf[(x, y)].set_bg(Palette::BG);
            }
        }

        let lines = help_lines();

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Palette::HEALTHY))
            .title(" Keybindings — Press any key to close ")
            .title_alignment(Alignment::Center)
            .style(Style::default().bg(Palette::BG));

        let paragraph = Paragraph::new(lines)
            .block(block)
            .wrap(Wrap { trim: false })
            .style(Style::default().fg(Palette::DATA).bg(Palette::BG));

        paragraph.render_ref(popup, buf);
    }
}

/// Build the two-column help text lines.
fn help_lines() -> Vec<Line<'static>> {
    let ks = Style::default().fg(Palette::HEALTHY);
    let ds = Style::default().fg(Palette::DATA);
    let hs = Style::default().fg(Palette::WARNING);
    let dim = Style::default().fg(Color::Rgb(100, 100, 120));
    let sep = "  ──────────────────────────────";

    vec![
        Line::from(""),
        Line::from(Span::styled("  Navigation", hs)),
        Line::from(Span::styled(sep, dim)),
        help_row("  h/j/k/l", "Move left/down/up/right", ks, ds),
        help_row("  ↑/↓", "Move up/down", ks, ds),
        Line::from(""),
        Line::from(Span::styled("  Tabs", hs)),
        Line::from(Span::styled(sep, dim)),
        help_row("  F1", "Overview", ks, ds),
        help_row("  F2", "World 3D", ks, ds),
        help_row("  F3", "Network", ks, ds),
        help_row("  F4", "Arbiter", ks, ds),
        Line::from(""),
        Line::from(Span::styled("  Actions", hs)),
        Line::from(Span::styled(sep, dim)),
        help_row("  Enter", "Select item", ks, ds),
        help_row("  Esc", "Back / deselect", ks, ds),
        help_row("  q", "Quit", ks, ds),
        Line::from(""),
        Line::from(Span::styled("  Modes", hs)),
        Line::from(Span::styled(sep, dim)),
        help_row("  :", "Command mode", ks, ds),
        help_row("  /", "Search mode", ks, ds),
        help_row("  ?", "Help (this overlay)", ks, ds),
        Line::from(""),
        Line::from(Span::styled("  Commands", hs)),
        Line::from(Span::styled(sep, dim)),
        help_row("  :kill <pid>", "Kill a process", ks, ds),
        help_row("  :sort <col>", "Sort by column", ks, ds),
        help_row("  :q", "Quit", ks, ds),
    ]
}

/// Build a single help row: key + description with padding.
fn help_row(key: &str, desc: &str, key_style: Style, desc_style: Style) -> Line<'static> {
    let padded_key = format!("{:<16}", key);
    Line::from(vec![
        Span::styled(padded_key, key_style),
        Span::styled(desc.to_string(), desc_style),
    ])
}

/// Compute a centered rectangle with the given percentage of `outer`.
fn centered_rect(outer: Rect, percent_w: u16, percent_h: u16) -> Rect {
    let w = outer.width.saturating_mul(percent_w) / 100;
    let h = outer.height.saturating_mul(percent_h) / 100;
    let x = outer.x + (outer.width.saturating_sub(w)) / 2;
    let y = outer.y + (outer.height.saturating_sub(h)) / 2;
    Rect::new(x, y, w, h)
}

/// Extension trait to render a widget reference into a buffer area.
///
/// Avoids consuming the widget, matching ratatui's `WidgetRef` pattern.
trait RenderRef {
    fn render_ref(&self, area: Rect, buf: &mut Buffer);
}

impl RenderRef for Clear {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        for y in area.y..area.bottom() {
            for x in area.x..area.right() {
                buf[(x, y)].reset();
            }
        }
    }
}

impl RenderRef for Paragraph<'_> {
    fn render_ref(&self, area: Rect, buf: &mut Buffer) {
        use ratatui::widgets::Widget;
        self.clone().render(area, buf);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_not_visible() {
        let overlay = HelpOverlay::default();
        assert!(!overlay.is_visible());
    }

    #[test]
    fn test_toggle_changes_visibility() {
        let mut overlay = HelpOverlay::default();
        overlay.toggle();
        assert!(overlay.is_visible());
        overlay.toggle();
        assert!(!overlay.is_visible());
    }

    #[test]
    fn test_dismiss_hides_overlay() {
        let mut overlay = HelpOverlay::default();
        overlay.toggle();
        assert!(overlay.is_visible());
        overlay.dismiss();
        assert!(!overlay.is_visible());
    }

    #[test]
    fn test_centered_rect_dimensions() {
        let outer = Rect::new(0, 0, 100, 50);
        let inner = centered_rect(outer, 60, 70);
        assert_eq!(inner.width, 60);
        assert_eq!(inner.height, 35);
        assert_eq!(inner.x, 20);
        assert_eq!(inner.y, 7);
    }

    #[test]
    fn test_centered_rect_zero_area() {
        let outer = Rect::new(0, 0, 0, 0);
        let inner = centered_rect(outer, 60, 70);
        assert_eq!(inner.width, 0);
        assert_eq!(inner.height, 0);
    }

    #[test]
    fn test_render_noop_when_hidden() {
        let overlay = HelpOverlay::default();
        let area = Rect::new(0, 0, 80, 24);
        let mut buf = Buffer::empty(area);
        // Should not panic or change buffer meaningfully.
        overlay.render(area, &mut buf);
    }

    #[test]
    fn test_render_writes_content_when_visible() {
        let mut overlay = HelpOverlay::default();
        overlay.toggle();
        let area = Rect::new(0, 0, 80, 40);
        let mut buf = Buffer::empty(area);
        overlay.render(area, &mut buf);

        // Check that the popup border was drawn (top-left corner of popup area).
        let popup = centered_rect(area, 60, 70);
        let cell = &buf[(popup.x, popup.y)];
        // Border cell should have the HEALTHY color.
        assert_eq!(cell.fg, Palette::HEALTHY);
    }

    #[test]
    fn test_help_lines_not_empty() {
        let lines = help_lines();
        assert!(!lines.is_empty());
    }
}

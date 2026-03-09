//! TUI application struct and main event loop.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use aether_core::WorldGraph;

use crate::RenderError;

use super::overview::OverviewTab;
use super::widgets::sparklines::SystemSparklines;

/// Active tab in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    /// F1: Process table with sparklines.
    #[default]
    Overview,
    /// F2: 3D process graph viewport.
    World3D,
    /// F3: Network connection list.
    Network,
    /// F4: AI action approval panel.
    Arbiter,
}

impl Tab {
    /// All tabs in display order.
    pub(crate) const ALL: [Tab; 4] = [Tab::Overview, Tab::World3D, Tab::Network, Tab::Arbiter];

    /// Human-readable label for the tab bar.
    pub fn label(self) -> &'static str {
        match self {
            Tab::Overview => "Overview [F1]",
            Tab::World3D => "World 3D [F2]",
            Tab::Network => "Network [F3]",
            Tab::Arbiter => "Arbiter [F4]",
        }
    }
}

/// Main TUI application state and controller.
///
/// Owns the event loop, routes input to the active tab,
/// and delegates rendering to per-tab draw functions.
pub struct App {
    /// Currently active tab.
    current_tab: Tab,
    /// Shared world graph (read-only from render side).
    world: Arc<RwLock<WorldGraph>>,
    /// Set to `true` to exit the event loop.
    should_quit: bool,
    /// Target frame interval (default 16ms ≈ 60fps).
    tick_rate: Duration,
    /// State for the Overview (F1) tab.
    overview: OverviewTab,
    /// When `true`, next key press selects a sort column.
    sort_pending: bool,
    /// Rolling sparkline history for the Overview tab.
    sparklines: SystemSparklines,
    /// Frames elapsed since last sparkline sample (used for 1-second tick).
    sparkline_tick: u32,
}

impl App {
    /// Create a new application with the given shared world graph.
    pub fn new(world: Arc<RwLock<WorldGraph>>) -> Self {
        Self {
            current_tab: Tab::default(),
            world,
            should_quit: false,
            tick_rate: Duration::from_millis(16),
            overview: OverviewTab::default(),
            sort_pending: false,
            sparklines: SystemSparklines::default(),
            sparkline_tick: 0,
        }
    }

    /// Run the main event loop until the user quits.
    ///
    /// Polls crossterm events with `tick_rate` timeout, handles input,
    /// and redraws the frame each iteration.
    pub async fn run(
        &mut self,
        terminal: &mut ratatui::Terminal<impl ratatui::backend::Backend>,
    ) -> Result<(), RenderError> {
        /// Approximate frames per sparkline sample (~1 second at 60fps).
        const SPARKLINE_INTERVAL: u32 = 60;

        while !self.should_quit {
            // Sample sparkline history approximately once per second.
            self.sparkline_tick += 1;
            if self.sparkline_tick >= SPARKLINE_INTERVAL {
                self.sparkline_tick = 0;
                if let Ok(world) = self.world.read() {
                    self.sparklines.update(&world);
                }
            }

            terminal.draw(|frame| self.draw(frame))?;

            if event::poll(self.tick_rate)? {
                if let Event::Key(key) = event::read()? {
                    self.handle_key(key);
                }
            }
        }
        Ok(())
    }

    /// Dispatch a key event to the appropriate handler.
    fn handle_key(&mut self, key: KeyEvent) {
        // Sort-mode: next key selects the sort column.
        if self.sort_pending {
            self.sort_pending = false;
            if self.current_tab == Tab::Overview && self.overview.handle_sort_key(key.code) {
                return;
            }
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::F(1) => self.current_tab = Tab::Overview,
            KeyCode::F(2) => self.current_tab = Tab::World3D,
            KeyCode::F(3) => self.current_tab = Tab::Network,
            KeyCode::F(4) => self.current_tab = Tab::Arbiter,
            KeyCode::Char('s') if self.current_tab == Tab::Overview => {
                self.sort_pending = true;
            }
            _ if self.current_tab == Tab::Overview => {
                if let Ok(world) = self.world.read() {
                    let count = world.process_count();
                    let sorted_pids = super::overview::collect_sorted_pids(
                        &world,
                        self.overview.sort_column(),
                        self.overview.sort_ascending(),
                    );
                    self.overview.handle_key(key.code, count, &sorted_pids);
                }
            }
            _ => {}
        }
    }

    /// Draw the full UI frame: tab bar, active tab content, and status bar.
    fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),
                Constraint::Min(0),
                Constraint::Length(1),
            ])
            .split(frame.area());

        let buf = frame.buffer_mut();
        super::tabs::render_tab_bar(chunks[0], buf, self.current_tab);

        if let Ok(world) = self.world.read() {
            let buf = frame.buffer_mut();
            super::tabs::render_status_bar(chunks[2], buf, &world);
        }

        self.draw_tab_content(frame, chunks[1]);
    }

    /// Render the active tab content.
    fn draw_tab_content(&self, frame: &mut Frame, area: ratatui::layout::Rect) {
        match self.current_tab {
            Tab::Overview => {
                if let Ok(world) = self.world.read() {
                    let chunks = Layout::default()
                        .direction(Direction::Vertical)
                        .constraints([Constraint::Length(5), Constraint::Min(0)])
                        .split(area);

                    let buf = frame.buffer_mut();
                    self.sparklines.render(chunks[0], buf);
                    self.overview.render(chunks[1], buf, &world);
                }
            }
            _ => {
                let content = match self.current_tab {
                    Tab::World3D => "3D process graph viewport (TODO)",
                    Tab::Network => "Network connection list (TODO)",
                    Tab::Arbiter => "AI action approval panel (TODO)",
                    Tab::Overview => unreachable!(),
                };
                let paragraph = Paragraph::new(Line::from(Span::raw(content)))
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(self.current_tab.label()),
                    )
                    .style(Style::default().fg(Color::White));
                frame.render_widget(paragraph, area);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tab_default_is_overview() {
        assert_eq!(Tab::default(), Tab::Overview);
    }

    #[test]
    fn test_tab_labels_not_empty() {
        for tab in &Tab::ALL {
            assert!(!tab.label().is_empty());
        }
    }

    #[test]
    fn test_app_new_defaults() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let app = App::new(world);
        assert_eq!(app.current_tab, Tab::Overview);
        assert!(!app.should_quit);
        assert_eq!(app.tick_rate, Duration::from_millis(16));
    }

    #[test]
    fn test_handle_key_quit_q() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let mut app = App::new(world);

        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_key_quit_ctrl_c() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let mut app = App::new(world);

        app.handle_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.should_quit);
    }

    #[test]
    fn test_handle_key_tab_switching() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let mut app = App::new(world);

        app.handle_key(KeyEvent::new(KeyCode::F(2), KeyModifiers::NONE));
        assert_eq!(app.current_tab, Tab::World3D);

        app.handle_key(KeyEvent::new(KeyCode::F(3), KeyModifiers::NONE));
        assert_eq!(app.current_tab, Tab::Network);

        app.handle_key(KeyEvent::new(KeyCode::F(4), KeyModifiers::NONE));
        assert_eq!(app.current_tab, Tab::Arbiter);

        app.handle_key(KeyEvent::new(KeyCode::F(1), KeyModifiers::NONE));
        assert_eq!(app.current_tab, Tab::Overview);
    }

    #[test]
    fn test_handle_key_unknown_ignored() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let mut app = App::new(world);

        app.handle_key(KeyEvent::new(KeyCode::Char('x'), KeyModifiers::NONE));
        assert_eq!(app.current_tab, Tab::Overview);
        assert!(!app.should_quit);
    }
}

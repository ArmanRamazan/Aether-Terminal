//! TUI application struct and main event loop.

use std::sync::{Arc, RwLock};
use std::time::Duration;

use crossterm::event::{self, Event, KeyEvent};
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use aether_core::WorldGraph;

use crate::RenderError;

use super::help::HelpOverlay;
use super::input::{InputAction, InputHandler};
use super::network::NetworkTab;
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
    /// State for the Network (F3) tab.
    network: NetworkTab,
    /// When `true`, next key press selects a sort column.
    sort_pending: bool,
    /// Rolling sparkline history for the Overview tab.
    sparklines: SystemSparklines,
    /// Frames elapsed since last sparkline sample (used for 1-second tick).
    sparkline_tick: u32,
    /// Vim-style modal input handler.
    input: InputHandler,
    /// Help overlay toggled with `?`.
    help: HelpOverlay,
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
            network: NetworkTab::default(),
            sort_pending: false,
            sparklines: SystemSparklines::default(),
            sparkline_tick: 0,
            input: InputHandler::default(),
            help: HelpOverlay::default(),
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
                    self.network.update(&world);
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
        // Help overlay intercept: any key dismisses it.
        if self.help.is_visible() {
            self.help.dismiss();
            return;
        }

        // Sort-mode intercept: next key selects the sort column.
        if self.sort_pending {
            self.sort_pending = false;
            if self.current_tab == Tab::Overview && self.overview.handle_sort_key(key.code) {
                return;
            }
        }

        let action = self.input.handle_key(key);
        self.dispatch_action(action);
    }

    /// Translate an [`InputAction`] into state changes.
    fn dispatch_action(&mut self, action: InputAction) {
        match action {
            InputAction::None => {}
            InputAction::CancelInput => {
                if self.current_tab == Tab::Network {
                    self.network.clear_filter();
                }
            }
            InputAction::Quit => self.should_quit = true,
            InputAction::SwitchTab(tab) => self.current_tab = tab,
            InputAction::Navigate(dir) => self.navigate(dir),
            InputAction::Select => self.select_current(),
            InputAction::Deselect => self.deselect_current(),
            InputAction::EnterSort if self.current_tab == Tab::Overview => {
                self.sort_pending = true;
            }
            InputAction::ExecuteCommand(cmd) => self.execute_command(&cmd),
            InputAction::Search(query) => {
                if self.current_tab == Tab::Network {
                    self.network.set_filter(query);
                }
                // TODO: filter process table by query on Overview tab
            }
            InputAction::NextMatch | InputAction::PrevMatch => {
                // TODO: cycle through search matches
            }
            InputAction::ToggleHelp => {
                self.help.toggle();
            }
            _ => {}
        }
    }

    /// Navigate within the active tab.
    fn navigate(&mut self, dir: super::input::Direction) {
        use super::input::Direction;
        let code = match dir {
            Direction::Down => crossterm::event::KeyCode::Char('j'),
            Direction::Up => crossterm::event::KeyCode::Char('k'),
            _ => return,
        };
        match self.current_tab {
            Tab::Overview => {
                if let Ok(world) = self.world.read() {
                    let count = world.process_count();
                    let sorted_pids = super::overview::collect_sorted_pids(
                        &world,
                        self.overview.sort_column(),
                        self.overview.sort_ascending(),
                    );
                    self.overview.handle_key(code, count, &sorted_pids);
                }
            }
            Tab::Network => {
                if let Ok(world) = self.world.read() {
                    let count = self.network.row_count(&world);
                    self.network.handle_key(code, count);
                }
            }
            _ => {}
        }
    }

    /// Select / confirm in the active tab.
    fn select_current(&mut self) {
        if self.current_tab == Tab::Overview {
            if let Ok(world) = self.world.read() {
                let sorted_pids = super::overview::collect_sorted_pids(
                    &world,
                    self.overview.sort_column(),
                    self.overview.sort_ascending(),
                );
                self.overview.handle_key(
                    crossterm::event::KeyCode::Enter,
                    world.process_count(),
                    &sorted_pids,
                );
            }
        }
    }

    /// Deselect / go back in the active tab.
    fn deselect_current(&mut self) {
        if self.current_tab == Tab::Overview {
            if let Ok(world) = self.world.read() {
                let sorted_pids = super::overview::collect_sorted_pids(
                    &world,
                    self.overview.sort_column(),
                    self.overview.sort_ascending(),
                );
                self.overview.handle_key(
                    crossterm::event::KeyCode::Esc,
                    world.process_count(),
                    &sorted_pids,
                );
            }
        }
    }

    /// Execute a command string from Command mode.
    fn execute_command(&mut self, cmd: &str) {
        let parts: Vec<&str> = cmd.split_whitespace().collect();
        match parts.first().copied() {
            Some("q") | Some("quit") => self.should_quit = true,
            Some("kill") => {
                // TODO: send kill signal to pid from parts[1]
            }
            Some("sort") => {
                // TODO: parse sort column from parts[1]
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
            super::tabs::render_status_bar(chunks[2], buf, &world, &self.input);
        }

        self.draw_tab_content(frame, chunks[1]);

        // Help overlay renders on top of everything.
        let area = frame.area();
        let buf = frame.buffer_mut();
        self.help.render(area, buf);
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
            Tab::Network => {
                if let Ok(world) = self.world.read() {
                    let buf = frame.buffer_mut();
                    self.network.render(area, buf, &world);
                }
            }
            _ => {
                let content = match self.current_tab {
                    Tab::World3D => "3D process graph viewport (TODO)",
                    Tab::Arbiter => "AI action approval panel (TODO)",
                    _ => unreachable!(),
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
    use crossterm::event::{KeyCode, KeyModifiers};

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

    #[test]
    fn test_command_quit() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let mut app = App::new(world);

        // Enter command mode, type "q", press Enter
        app.handle_key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        app.handle_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE));
        assert!(app.should_quit);
    }

    #[test]
    fn test_help_toggle_and_dismiss() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let mut app = App::new(world);

        // '?' toggles help on
        app.handle_key(KeyEvent::new(KeyCode::Char('?'), KeyModifiers::NONE));
        assert!(app.help.is_visible());

        // Any key dismisses it (e.g. 'j' should NOT navigate)
        let tab_before = app.current_tab;
        app.handle_key(KeyEvent::new(KeyCode::Char('j'), KeyModifiers::NONE));
        assert!(!app.help.is_visible());
        assert_eq!(app.current_tab, tab_before);
    }

    #[test]
    fn test_colon_enters_command_mode() {
        let world = Arc::new(RwLock::new(WorldGraph::new()));
        let mut app = App::new(world);

        app.handle_key(KeyEvent::new(KeyCode::Char(':'), KeyModifiers::NONE));
        assert_eq!(app.input.mode(), super::super::input::InputMode::Command);
        // 'q' in command mode should NOT quit (it's a buffer character)
        app.handle_key(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE));
        assert!(!app.should_quit);
    }
}

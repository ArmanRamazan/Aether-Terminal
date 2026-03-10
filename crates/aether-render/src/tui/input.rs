//! Vim-style input modes: Normal, Command (`:`), and Search (`/`).
//!
//! [`InputHandler`] owns the current mode and input buffer, translating
//! raw [`KeyEvent`]s into semantic [`InputAction`]s that the [`App`]
//! event loop can dispatch.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::app::Tab;

/// Active input mode (displayed in the status bar).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum InputMode {
    /// Normal navigation — hjkl, tab switching, etc.
    #[default]
    Normal,
    /// Command-line input after pressing `:`.
    Command,
    /// Search input after pressing `/`.
    Search,
}

/// Cardinal direction for hjkl navigation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Direction {
    /// `h` — left.
    Left,
    /// `j` — down.
    Down,
    /// `k` — up.
    Up,
    /// `l` — right.
    Right,
}

/// Semantic action returned by [`InputHandler::handle_key`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum InputAction {
    /// No meaningful action for this key.
    None,
    /// Quit the application.
    Quit,
    /// Switch to the given tab.
    SwitchTab(Tab),
    /// Navigate in a direction.
    Navigate(Direction),
    /// Execute a command string (from `:` mode).
    ExecuteCommand(String),
    /// Execute a search query (from `/` mode).
    Search(String),
    /// Cancel current input mode and return to Normal.
    CancelInput,
    /// Scroll viewport up (reserved for page-scroll keybinds).
    #[allow(dead_code)]
    ScrollUp,
    /// Scroll viewport down (reserved for page-scroll keybinds).
    #[allow(dead_code)]
    ScrollDown,
    /// Select / confirm the current item.
    Select,
    /// Deselect / go back.
    Deselect,
    /// Enter sort-column selection mode.
    EnterSort,
    /// Toggle help overlay.
    ToggleHelp,
    /// Jump to next search match.
    NextMatch,
    /// Jump to previous search match.
    PrevMatch,
}

/// Vim-style input handler with modal editing.
///
/// Maintains the current [`InputMode`] and a text buffer for Command
/// and Search modes. Call [`handle_key`] each frame to get an [`InputAction`].
#[derive(Debug)]
pub(crate) struct InputHandler {
    /// Current input mode.
    mode: InputMode,
    /// Text buffer for Command/Search modes.
    buffer: String,
    /// Whether a search was performed (enables n/N in Normal mode).
    has_search: bool,
}

impl Default for InputHandler {
    fn default() -> Self {
        Self {
            mode: InputMode::Normal,
            buffer: String::new(),
            has_search: false,
        }
    }
}

impl InputHandler {
    /// Current input mode.
    #[allow(dead_code)]
    pub(crate) fn mode(&self) -> InputMode {
        self.mode
    }

    /// Current buffer contents (for Command/Search display).
    #[allow(dead_code)]
    pub(crate) fn buffer(&self) -> &str {
        &self.buffer
    }

    /// Format the mode indicator for the status bar.
    ///
    /// Returns strings like `"[NORMAL]"`, `"[COMMAND] :kill "`, `"[SEARCH] /firefox"`.
    pub(crate) fn status_text(&self) -> String {
        match self.mode {
            InputMode::Normal => "[NORMAL]".to_string(),
            InputMode::Command => format!("[COMMAND] :{}", self.buffer),
            InputMode::Search => format!("[SEARCH] /{}", self.buffer),
        }
    }

    /// Process a key event and return the corresponding action.
    pub(crate) fn handle_key(&mut self, key: KeyEvent) -> InputAction {
        match self.mode {
            InputMode::Normal => self.handle_normal(key),
            InputMode::Command => self.handle_command(key),
            InputMode::Search => self.handle_search(key),
        }
    }

    /// Handle keys in Normal mode.
    fn handle_normal(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Char('q') => InputAction::Quit,
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                InputAction::Quit
            }
            KeyCode::Char('h') => InputAction::Navigate(Direction::Left),
            KeyCode::Char('j') | KeyCode::Down => InputAction::Navigate(Direction::Down),
            KeyCode::Char('k') | KeyCode::Up => InputAction::Navigate(Direction::Up),
            KeyCode::Char('l') => InputAction::Navigate(Direction::Right),
            KeyCode::F(1) => InputAction::SwitchTab(Tab::Overview),
            KeyCode::F(2) => InputAction::SwitchTab(Tab::World3D),
            KeyCode::F(3) => InputAction::SwitchTab(Tab::Network),
            KeyCode::F(4) => InputAction::SwitchTab(Tab::Arbiter),
            KeyCode::F(5) => InputAction::SwitchTab(Tab::Rules),
            KeyCode::Enter => InputAction::Select,
            KeyCode::Esc => InputAction::Deselect,
            KeyCode::Char('s') => InputAction::EnterSort,
            KeyCode::Char(':') => {
                self.mode = InputMode::Command;
                self.buffer.clear();
                InputAction::None
            }
            KeyCode::Char('/') => {
                self.mode = InputMode::Search;
                self.buffer.clear();
                InputAction::None
            }
            KeyCode::Char('?') => InputAction::ToggleHelp,
            KeyCode::Char('n') if self.has_search => InputAction::NextMatch,
            KeyCode::Char('N') if self.has_search => InputAction::PrevMatch,
            _ => InputAction::None,
        }
    }

    /// Handle keys in Command mode (`:` prefix).
    fn handle_command(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Enter => {
                self.mode = InputMode::Normal;
                let cmd = self.buffer.drain(..).collect();
                InputAction::ExecuteCommand(cmd)
            }
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                self.buffer.clear();
                InputAction::CancelInput
            }
            KeyCode::Backspace => {
                self.buffer.pop();
                if self.buffer.is_empty() {
                    self.mode = InputMode::Normal;
                    InputAction::CancelInput
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char(c) => {
                self.buffer.push(c);
                InputAction::None
            }
            _ => InputAction::None,
        }
    }

    /// Handle keys in Search mode (`/` prefix).
    fn handle_search(&mut self, key: KeyEvent) -> InputAction {
        match key.code {
            KeyCode::Enter => {
                self.mode = InputMode::Normal;
                self.has_search = !self.buffer.is_empty();
                let query = self.buffer.drain(..).collect();
                InputAction::Search(query)
            }
            KeyCode::Esc => {
                self.mode = InputMode::Normal;
                self.buffer.clear();
                InputAction::CancelInput
            }
            KeyCode::Backspace => {
                self.buffer.pop();
                if self.buffer.is_empty() {
                    self.mode = InputMode::Normal;
                    InputAction::CancelInput
                } else {
                    InputAction::None
                }
            }
            KeyCode::Char(c) => {
                self.buffer.push(c);
                InputAction::None
            }
            _ => InputAction::None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    #[test]
    fn test_default_mode_is_normal() {
        let handler = InputHandler::default();
        assert_eq!(handler.mode(), InputMode::Normal);
        assert!(handler.buffer().is_empty());
    }

    #[test]
    fn test_normal_quit_q() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('q'))),
            InputAction::Quit
        );
    }

    #[test]
    fn test_normal_quit_ctrl_c() {
        let mut handler = InputHandler::default();
        assert_eq!(handler.handle_key(ctrl('c')), InputAction::Quit);
    }

    #[test]
    fn test_normal_hjkl_navigation() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('h'))),
            InputAction::Navigate(Direction::Left)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('j'))),
            InputAction::Navigate(Direction::Down)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('k'))),
            InputAction::Navigate(Direction::Up)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('l'))),
            InputAction::Navigate(Direction::Right)
        );
    }

    #[test]
    fn test_normal_arrow_keys() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Down)),
            InputAction::Navigate(Direction::Down)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::Up)),
            InputAction::Navigate(Direction::Up)
        );
    }

    #[test]
    fn test_normal_tab_switching() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::F(1))),
            InputAction::SwitchTab(Tab::Overview)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::F(2))),
            InputAction::SwitchTab(Tab::World3D)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::F(3))),
            InputAction::SwitchTab(Tab::Network)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::F(4))),
            InputAction::SwitchTab(Tab::Arbiter)
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::F(5))),
            InputAction::SwitchTab(Tab::Rules)
        );
    }

    #[test]
    fn test_normal_enter_select() {
        let mut handler = InputHandler::default();
        assert_eq!(handler.handle_key(key(KeyCode::Enter)), InputAction::Select);
    }

    #[test]
    fn test_normal_esc_deselect() {
        let mut handler = InputHandler::default();
        assert_eq!(handler.handle_key(key(KeyCode::Esc)), InputAction::Deselect);
    }

    #[test]
    fn test_colon_enters_command_mode() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Char(':'))),
            InputAction::None
        );
        assert_eq!(handler.mode(), InputMode::Command);
        assert!(handler.buffer().is_empty());
    }

    #[test]
    fn test_slash_enters_search_mode() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('/'))),
            InputAction::None
        );
        assert_eq!(handler.mode(), InputMode::Search);
        assert!(handler.buffer().is_empty());
    }

    #[test]
    fn test_command_mode_typing_and_execute() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char(':')));

        handler.handle_key(key(KeyCode::Char('q')));
        assert_eq!(handler.buffer(), "q");

        let action = handler.handle_key(key(KeyCode::Enter));
        assert_eq!(action, InputAction::ExecuteCommand("q".to_string()));
        assert_eq!(handler.mode(), InputMode::Normal);
        assert!(handler.buffer().is_empty());
    }

    #[test]
    fn test_command_mode_esc_cancels() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char(':')));
        handler.handle_key(key(KeyCode::Char('k')));

        let action = handler.handle_key(key(KeyCode::Esc));
        assert_eq!(action, InputAction::CancelInput);
        assert_eq!(handler.mode(), InputMode::Normal);
        assert!(handler.buffer().is_empty());
    }

    #[test]
    fn test_command_mode_backspace_deletes() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char(':')));
        handler.handle_key(key(KeyCode::Char('a')));
        handler.handle_key(key(KeyCode::Char('b')));
        assert_eq!(handler.buffer(), "ab");

        handler.handle_key(key(KeyCode::Backspace));
        assert_eq!(handler.buffer(), "a");
        assert_eq!(handler.mode(), InputMode::Command);
    }

    #[test]
    fn test_command_mode_backspace_empty_cancels() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char(':')));
        handler.handle_key(key(KeyCode::Char('x')));
        handler.handle_key(key(KeyCode::Backspace));
        // Buffer is now empty → exits to Normal
        assert_eq!(handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_search_mode_enter_submits() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char('/')));
        handler.handle_key(key(KeyCode::Char('f')));
        handler.handle_key(key(KeyCode::Char('o')));
        handler.handle_key(key(KeyCode::Char('o')));

        let action = handler.handle_key(key(KeyCode::Enter));
        assert_eq!(action, InputAction::Search("foo".to_string()));
        assert_eq!(handler.mode(), InputMode::Normal);
    }

    #[test]
    fn test_search_enables_n_and_shift_n() {
        let mut handler = InputHandler::default();
        // Before search, n does nothing special
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('n'))),
            InputAction::None
        );

        // Perform a search
        handler.handle_key(key(KeyCode::Char('/')));
        handler.handle_key(key(KeyCode::Char('x')));
        handler.handle_key(key(KeyCode::Enter));

        // Now n/N work
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('n'))),
            InputAction::NextMatch
        );
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('N'))),
            InputAction::PrevMatch
        );
    }

    #[test]
    fn test_empty_search_does_not_enable_n() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char('/')));
        handler.handle_key(key(KeyCode::Enter)); // empty search
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('n'))),
            InputAction::None
        );
    }

    #[test]
    fn test_status_text_normal() {
        let handler = InputHandler::default();
        assert_eq!(handler.status_text(), "[NORMAL]");
    }

    #[test]
    fn test_status_text_command() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char(':')));
        handler.handle_key(key(KeyCode::Char('k')));
        handler.handle_key(key(KeyCode::Char('i')));
        handler.handle_key(key(KeyCode::Char('l')));
        handler.handle_key(key(KeyCode::Char('l')));
        assert_eq!(handler.status_text(), "[COMMAND] :kill");
    }

    #[test]
    fn test_status_text_search() {
        let mut handler = InputHandler::default();
        handler.handle_key(key(KeyCode::Char('/')));
        handler.handle_key(key(KeyCode::Char('f')));
        assert_eq!(handler.status_text(), "[SEARCH] /f");
    }

    #[test]
    fn test_question_mark_toggles_help() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('?'))),
            InputAction::ToggleHelp
        );
    }

    #[test]
    fn test_sort_action() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('s'))),
            InputAction::EnterSort
        );
    }

    #[test]
    fn test_unknown_key_returns_none() {
        let mut handler = InputHandler::default();
        assert_eq!(
            handler.handle_key(key(KeyCode::Char('z'))),
            InputAction::None
        );
    }
}

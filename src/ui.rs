use std::io;

use crossterm::{terminal::{EnterAlternateScreen, LeaveAlternateScreen}, event::{EnableMouseCapture, DisableMouseCapture}};
use ratatui::{backend::CrosstermBackend, Terminal, widgets::{Block, Borders}};

use crate::command;

pub struct Ui {
}

impl Ui {
    pub fn run() -> Result<(), Box<dyn std::error::Error>> {
        // setup terminal
        crossterm::terminal::enable_raw_mode()?;
        let mut stdout = io::stdout();
        crossterm::execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        loop {
            terminal.draw(|f| {
                let size = f.size();
                let block = Block::default()
                    .title("Block")
                    .borders(Borders::ALL);
                f.render_widget(block, size);
            })?;
        }

        // restore terminal
        crossterm::terminal::disable_raw_mode()?;
        crossterm::execute!(terminal.backend_mut(), LeaveAlternateScreen, DisableMouseCapture)?;
        terminal.show_cursor()?;

        // println!("{}", err);

        Ok(())
    }
}

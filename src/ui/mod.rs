pub mod helper;
pub mod input;
pub mod widgets;

use crate::library::Song;
use crate::player::symphonia_player::SymphoniaPlayer;
use crate::player::Player;
use crate::state::AppState;
use crate::utils::constants::Requests::{PlayerRequests, UIRequests::*};
use crate::{library::Library, utils::constants::Requests::UIRequests};
use std::sync::{Arc, Mutex};
use std::{
    fmt::format,
    io::{self, Stdout},
    sync::mpsc::{Receiver, Sender},
    time::{Duration, Instant},
};
use tui::layout::Alignment;
use tui::widgets::Wrap;
use widgets::stateful_list::StatefulList;

use crossterm::{
    cursor, event,
    event::Event,
    event::KeyCode,
    execute, style,
    terminal::{self, disable_raw_mode, enable_raw_mode},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Spans, Text},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};

pub fn start<'a>(
    state: Arc<Mutex<AppState>>,
    rx: Receiver<UIRequests>,
    songs: Vec<Song>,
    player_tx: Sender<PlayerRequests>,
) {
    info!("Starting up UI...");

    // initialize terminal state
    enable_raw_mode().unwrap();
    let mut stdout = io::stdout();
    execute!(
        stdout,
        cursor::Hide,
        terminal::EnterAlternateScreen,
        event::EnableMouseCapture
    )
    .unwrap();
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend).unwrap();

    debug!("Terminal started successfully");

    let app = App::with_songs(state, songs);
    app.run(&mut terminal, rx, player_tx);

    info!("stopping now");

    // restore terminal
    info!("Starting to cleanup terminal ...");
    disable_raw_mode().unwrap();
    execute!(
        terminal.backend_mut(),
        terminal::LeaveAlternateScreen,
        event::DisableMouseCapture
    )
    .unwrap();
    terminal.show_cursor().unwrap();
    info!("Terminal cleaned successfully");
}

pub struct App {
    state: Arc<Mutex<AppState>>,
    song_list: StatefulList<Song>,
    tmp_show_popup: bool,
}

impl App {
    pub fn new(state: Arc<Mutex<AppState>>) -> App {
        App {
            state,
            song_list: StatefulList::with_items(vec![]),
            tmp_show_popup: false,
        }
    }

    pub fn with_songs(state: Arc<Mutex<AppState>>, songs: Vec<Song>) -> App {
        App {
            state,
            song_list: StatefulList::with_items(songs),
            tmp_show_popup: false,
        }
    }

    #[warn(unreachable_patterns)]
    pub fn run(
        mut self,
        terminal: &mut Terminal<CrosstermBackend<Stdout>>,
        rx: Receiver<UIRequests>,
        player_tx: Sender<PlayerRequests>,
    ) -> () {
        self.song_list.next(); // select first element

        loop {
            terminal.draw(|f| self.get_ui(f, &player_tx)).unwrap();
            match rx.recv() {
                Ok(request) => match request {
                    Up => self.on_up(),
                    Down => self.on_down(),
                    Enter => self.on_enter(),
                    ShowSearch => self.state.lock().unwrap().searching = true,
                    SearchInput(ch) => self.state.lock().unwrap().search_term.push(ch),
                    GoBack => self.go_back(),
                    Quit => return,
                    _ => {
                        error!("This UI event is not implemented yet")
                    }
                },
                Err(err) => {
                    error!(
                        "Could not receive UI event. \n \t Reason: {}",
                        err.to_string()
                    )
                }
            }
        }
    }

    fn on_up(&mut self) {
        self.song_list.previous()
    }

    fn on_down(&mut self) {
        self.song_list.next();
    }

    fn on_enter(&mut self) {
        self.tmp_show_popup = !self.tmp_show_popup;
    }

    fn go_back(&mut self) {
        if self.state.lock().unwrap().searching {
            self.state.lock().unwrap().searching = false;
            self.state.lock().unwrap().search_term.clear();
        }
    }

    fn get_ui<B: Backend>(&mut self, frame: &mut Frame<B>, player_tx: &Sender<PlayerRequests>) {
        let size = frame.size();
        let block = Block::default().title("tarvrs").borders(Borders::ALL);
        frame.render_widget(block, size);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .margin(1)
            .constraints(
                [
                    Constraint::Percentage(10),
                    Constraint::Percentage(70),
                    Constraint::Percentage(20),
                ]
                .as_ref(),
            )
            .split(frame.size());

        let block = Block::default().title("Block").borders(Borders::ALL);
        frame.render_widget(block, chunks[0]);

        let block = Block::default().title("Block 3").borders(Borders::ALL);
        frame.render_widget(block, chunks[2]);

        let list: Vec<ListItem> = self
            .song_list
            .items
            .iter()
            .map(|i| ListItem::new(vec![Spans::from(i.title.clone())]))
            .collect();

        let list = List::new(list)
            .block(Block::default().borders(Borders::ALL).title("Songs"))
            .highlight_style(
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            )
            .highlight_symbol(">> ");

        frame.render_stateful_widget(list, chunks[1], &mut self.song_list.state);
        widgets::curr_playing_bar::render(frame, chunks[2], &(self.state.lock().unwrap()));

        if self.tmp_show_popup {
            let block = Block::default().title("Popup").borders(Borders::ALL);
            let area = helper::centered_rect(60, 60, size);
            let selected_song = self
                .song_list
                .items
                .get(self.song_list.state.selected().unwrap());
            let paragraph = Paragraph::new(format!("{:#?}", selected_song.unwrap()))
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Left);
            frame.render_widget(Clear, area);
            frame.render_widget(paragraph, block.inner(area));
            frame.render_widget(block, area);
            player_tx.send(PlayerRequests::Start(
                selected_song.unwrap().path.to_owned(),
            ));
            self.state.lock().unwrap().curr_song = Some(selected_song.unwrap().to_owned());
        } else {
            player_tx.send(PlayerRequests::Stop);
        }

        if self.state.lock().unwrap().searching {
            // widgets::search_popup::render(frame, self.state.lock().unwrap().search_term.to_owned());
            let search = Paragraph::new(self.state.lock().unwrap().search_term.to_owned())
                .style(Style::default().fg(Color::White))
                .alignment(Alignment::Center)
                .wrap(Wrap { trim: false });
            frame.render_widget(Clear, chunks[0]);
            frame.render_widget(search, chunks[0])
        }

    }
}

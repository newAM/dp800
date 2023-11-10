use anyhow::Context;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use dp800::{Dp800, Measurement};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Span, Text},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph},
    Frame, Terminal,
};
use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_millis(250);

const NUM_CH: usize = 3;

/// Vertical selection
enum Vsel {
    Measure,
    // list 1
    SetVolt,
    SetAmp,
    // list 2
    Ovp,
    Ocp,
    OvpOn,
    OcpOn,
}

impl Vsel {
    #[must_use]
    pub fn next(&self) -> Self {
        match self {
            Vsel::Measure => Vsel::SetVolt,
            Vsel::SetVolt => Vsel::SetAmp,
            Vsel::SetAmp => Vsel::Ovp,
            Vsel::Ovp => Vsel::Ocp,
            Vsel::Ocp => Vsel::OvpOn,
            Vsel::OvpOn => Vsel::OcpOn,
            Vsel::OcpOn => Vsel::Measure,
        }
    }

    #[must_use]
    pub fn prev(&self) -> Self {
        match self {
            Vsel::Measure => Vsel::OcpOn,
            Vsel::SetVolt => Vsel::Measure,
            Vsel::SetAmp => Vsel::SetVolt,
            Vsel::Ovp => Vsel::SetAmp,
            Vsel::Ocp => Vsel::Ovp,
            Vsel::OvpOn => Vsel::Ocp,
            Vsel::OcpOn => Vsel::OvpOn,
        }
    }

    pub fn list_idx(&self) -> Option<usize> {
        match self {
            Vsel::Measure => None,
            Vsel::SetVolt => Some(0),
            Vsel::SetAmp => Some(1),
            Vsel::Ovp => Some(0),
            Vsel::Ocp => Some(1),
            Vsel::OvpOn => Some(2),
            Vsel::OcpOn => Some(3),
        }
    }
}

#[derive(Default)]
struct Data {
    output_state: bool,
    meas_voltage: f32,
    meas_current: f32,
    meas_power: f32,
    sp_voltage: f32,
    sp_current: f32,
    limit_voltage: f32,
    limit_current: f32,
    ovp_on: bool,
    ocp_on: bool,
}

struct App {
    dp832: Dp800,
    data: [Data; NUM_CH],
    ch: u8,
    vsel: Vsel,
    input_title: String,
    input: String,
}

impl App {
    async fn on_tick(&mut self) -> anyhow::Result<()> {
        for (idx, data) in self.data.iter_mut().enumerate() {
            let ch_idx = u8::try_from(idx).unwrap() + 1;
            let meas: Measurement = self.dp832.measure(ch_idx).await?;

            *data = Data {
                output_state: self.dp832.output_state(ch_idx).await?,
                meas_voltage: meas.voltage,
                meas_current: meas.current,
                meas_power: meas.power,
                sp_voltage: self.dp832.voltage(ch_idx).await?,
                sp_current: self.dp832.current(ch_idx).await?,
                limit_voltage: self.dp832.ovp(ch_idx).await?,
                limit_current: self.dp832.ocp(ch_idx).await?,
                ovp_on: self.dp832.ovp_on(ch_idx).await?,
                ocp_on: self.dp832.ocp_on(ch_idx).await?,
            };
        }

        Ok(())
    }

    fn ch_data(&self) -> &Data {
        &self.data[(self.ch - 1) as usize]
    }
}

async fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration,
) -> anyhow::Result<()> {
    app.on_tick().await?;

    let mut last_tick: Instant = Instant::now();
    loop {
        terminal.draw(|f| ui(f, &app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));

        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                if !app.input_title.is_empty() {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Enter => {
                            app.input_title = String::new();
                            // should not panic with character input limitations
                            // would be a good thing to fuzz if this was more than
                            // a simple weekend project
                            let value: f32 = app.input.parse().unwrap();
                            app.input = String::new();
                            match app.vsel {
                                Vsel::SetVolt => app.dp832.set_voltage(app.ch, value).await?,
                                Vsel::SetAmp => app.dp832.set_current(app.ch, value).await?,
                                Vsel::Ovp => app.dp832.set_ovp(app.ch, value).await?,
                                Vsel::Ocp => app.dp832.set_ocp(app.ch, value).await?,
                                Vsel::Measure | Vsel::OvpOn | Vsel::OcpOn => unreachable!(),
                            }
                        }
                        KeyCode::Char(c @ ('0'..='9' | '.')) => {
                            if app.input.len() < 16 {
                                app.input.push(c);
                            }
                        }
                        KeyCode::Backspace => {
                            app.input.pop();
                        }
                        KeyCode::Esc => {
                            app.input_title = String::new();
                            app.input = String::new();
                        }
                        _ => (),
                    }
                } else {
                    match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Right | KeyCode::Char('l') => {
                            app.ch += 1;
                            if usize::from(app.ch) > NUM_CH {
                                app.ch = 1;
                            }
                            app.dp832.set_ch(app.ch).await?;
                            // switching channels too quickly can cause the PSU
                            // to report invalid commands
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                        KeyCode::Left | KeyCode::Char('h') => {
                            app.ch -= 1;
                            if app.ch == 0 {
                                app.ch = NUM_CH as u8;
                            }
                            app.dp832.set_ch(app.ch).await?;
                            // switching channels too quickly can cause the PSU
                            // to report invalid commands
                            tokio::time::sleep(Duration::from_millis(50)).await;
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            app.vsel = app.vsel.prev();
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            app.vsel = app.vsel.next();
                        }
                        KeyCode::Enter => match app.vsel {
                            Vsel::Measure => {
                                app.dp832
                                    .set_output_state(app.ch, !app.ch_data().output_state)
                                    .await?
                            }
                            Vsel::SetVolt => app.input_title = "Voltage Setpoint (V)".to_string(),
                            Vsel::SetAmp => app.input_title = "Current Setpoint (A)".to_string(),
                            Vsel::Ovp => {
                                app.input_title = "Over Voltage Protection (V)".to_string()
                            }
                            Vsel::Ocp => {
                                app.input_title = "Over Current Protection (A)".to_string()
                            }
                            Vsel::OvpOn => {
                                app.dp832.set_ovp_on(app.ch, !app.ch_data().ovp_on).await?
                            }
                            Vsel::OcpOn => {
                                app.dp832.set_ocp_on(app.ch, !app.ch_data().ocp_on).await?
                            }
                        },
                        _ => {}
                    }
                }
            }
        }

        if last_tick.elapsed() >= tick_rate {
            const NUM_RETRY: usize = 3;
            for attempt in 1..=NUM_RETRY {
                match tokio::time::timeout(TIMEOUT, app.on_tick()).await {
                    Err(e) => {
                        if attempt == NUM_RETRY {
                            Err(e).with_context(|| {
                                format!("DP832 sample timeout after {NUM_RETRY} attempts")
                            })?;
                        } else {
                            log::warn!("Sample timeout attempt {attempt}/{NUM_RETRY}");
                            tokio::time::sleep(TIMEOUT).await;
                        }
                    }
                    Ok(result) => result?,
                }
            }

            last_tick = Instant::now();
        }
    }
}

fn ui(f: &mut Frame, app: &App) {
    let size = f.size();

    let mut constraints: Vec<Constraint> = vec![Constraint::Max(15)];
    if !app.input_title.is_empty() {
        constraints.push(Constraint::Max(3));
    }
    constraints.push(Constraint::Max(1));

    let vertical_split = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(size);

    let mut veritical_iterator = vertical_split.iter();

    let channels = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(33),
            Constraint::Percentage(33),
            Constraint::Percentage(33),
        ])
        .split(*veritical_iterator.next().unwrap());

    for (idx, data) in app.data.iter().enumerate() {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(5), Constraint::Min(4), Constraint::Min(6)])
            .split(channels[idx]);

        let ch_idx: u8 = u8::try_from(idx).unwrap() + 1;
        let ch_selected: bool = ch_idx == app.ch;

        let title_style: Style = {
            let title_color: Color = if data.output_state {
                Color::Green
            } else {
                Color::White
            };
            Style::default()
                .add_modifier(Modifier::BOLD)
                .fg(title_color)
        };

        let block_color = |selected| {
            if selected {
                Color::White
            } else {
                Color::DarkGray
            }
        };

        let bool_to_on_off = |val| {
            if val {
                "On"
            } else {
                "Off"
            }
        };

        {
            let selected: bool = ch_selected && matches!(app.vsel, Vsel::Measure);
            let block_color: Color = block_color(selected);
            let power_state: &str = bool_to_on_off(data.output_state);

            let title: String = format!("CH{ch_idx} - {power_state}");

            let block: Block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(block_color))
                .title(Span::styled(title, title_style));

            let mut style: Style = Style::default().add_modifier(Modifier::BOLD);
            if !data.output_state {
                style = style.add_modifier(Modifier::DIM)
            } else {
                style = style.fg(Color::White)
            }

            let paragraph: Paragraph = Paragraph::new(Text::styled(
                format!(
                    "{:>6.3} V\n{:>6.3} A\n{:>6.3} W",
                    data.meas_voltage, data.meas_current, data.meas_power
                ),
                style,
            ))
            .block(block);

            f.render_widget(paragraph, chunks[0]);
        }

        {
            let selected: bool = ch_selected && matches!(app.vsel, Vsel::SetAmp | Vsel::SetVolt);
            let block_color: Color = block_color(selected);

            let block: Block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(block_color))
                .title(Span::styled("Set", title_style));

            let list_items: [ListItem; 2] = [
                ListItem::new(format!("{:>6.3} V", data.sp_voltage)),
                ListItem::new(format!("{:>6.3} A", data.sp_current)),
            ];

            let list: List = List::new(list_items)
                .style(Style::default().bg(Color::Reset).fg(Color::White))
                .block(block)
                .highlight_symbol(">");

            let mut state: ListState = ListState::default();
            if selected {
                state.select(app.vsel.list_idx());
            }

            f.render_stateful_widget(list, chunks[1], &mut state);
        }

        {
            let selected: bool = ch_selected
                && matches!(app.vsel, Vsel::Ocp | Vsel::OcpOn | Vsel::Ovp | Vsel::OvpOn);
            let block_color: Color = block_color(selected);

            let block: Block = Block::default()
                .borders(Borders::ALL)
                .style(Style::default().fg(block_color))
                .title(Span::styled("Limit", title_style));

            let mut ocp_style: Style = Style::default();
            if !data.ocp_on {
                ocp_style = ocp_style.add_modifier(Modifier::DIM)
            }
            let mut ovp_style: Style = Style::default();
            if !data.ovp_on {
                ovp_style = ovp_style.add_modifier(Modifier::DIM)
            }

            let list_items: [ListItem; 4] = [
                ListItem::new(format!("{:>6.3} V", data.limit_voltage)).style(ovp_style),
                ListItem::new(format!("{:>6.3} A", data.limit_current)).style(ocp_style),
                ListItem::new(format!("OVP: {}", bool_to_on_off(data.ovp_on))),
                ListItem::new(format!("OCP: {}", bool_to_on_off(data.ocp_on))),
            ];

            let list: List = List::new(list_items)
                .style(Style::default().bg(Color::Reset).fg(Color::White))
                .block(block)
                .highlight_symbol(">");

            let mut state: ListState = ListState::default();
            if selected {
                state.select(app.vsel.list_idx());
            }

            f.render_stateful_widget(list, chunks[2], &mut state);
        }
    }

    if !app.input_title.is_empty() {
        let block: Block = Block::default()
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::Yellow))
            .title(Span::styled(
                app.input_title.as_str(),
                Style::default()
                    .add_modifier(Modifier::BOLD)
                    .fg(Color::White),
            ));

        let paragraph: Paragraph = Paragraph::new(app.input.as_str()).block(block);
        f.render_widget(paragraph, *veritical_iterator.next().unwrap());
    }

    {
        let paragraph: Paragraph =
            Paragraph::new("Navigate [←↓↑→] Select [⏎] Discard Input [Esc] Quit [q]");
        f.render_widget(paragraph, *veritical_iterator.next().unwrap());
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut conf: PathBuf =
        dirs::config_dir().context("Unable to locate configuration directory")?;
    conf.push("dp832.txt");

    let conf_file_contents: String = std::fs::read_to_string(&conf)
        .with_context(|| format!("Failed to read configuration file {}", conf.display()))?;

    let address: &str = conf_file_contents.trim();

    log::debug!("Connecting to {address}");
    let mut dp832: Dp800 = Dp800::connect(&address)
        .await
        .with_context(|| format!("Failed to connect to power supply at {address}"))?;
    log::debug!("Connected");
    let ch: u8 = dp832.ch().await?;

    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(250);
    let app = App {
        dp832,
        ch,
        vsel: Vsel::Measure,
        input_title: String::new(),
        input: String::new(),
        data: Default::default(),
    };
    let res = run_app(&mut terminal, app, tick_rate).await;

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    res?;

    Ok(())
}

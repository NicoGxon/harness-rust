use agent_core::{AgentEvent, AgentRunner, UserInput};
use console::style;
use crossterm::cursor::MoveToPreviousLine;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};

use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::io::{self, Write};
use tokio::sync::mpsc;

use super::input::InputState;
use super::markdown::MarkdownStreamProcessor;

pub async fn run_tui(
    runner: AgentRunner,
    _model_name: String,
    _provider_name: String,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (input_tx, input_rx) = mpsc::unbounded_channel::<UserInput>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

    // Spawn agent runner in background
    let agent_handle = tokio::spawn(async move { runner.run(input_rx, event_tx).await });

    let mut input_state = InputState::new();

    loop {
        // Habilitar raw mode para interactuar con la caja inline de Ratatui
        enable_raw_mode()?;

        let mut current_viewport_height = 3u16;
        let mut terminal = Terminal::with_options(
            CrosstermBackend::new(io::stdout()),
            TerminalOptions {
                viewport: Viewport::Inline(current_viewport_height),
            },
        )?;

        // Bucle de edición dentro de la caja de input inline
        let user_prompt = loop {
            let term_width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
            let (display_text, target_x, target_y, total_lines) =
                input_state.format_display_lines(term_width);

            let needed_height = (total_lines + 2).clamp(3, 12);
            if needed_height != current_viewport_height {
                let _ = terminal.clear();
                let _ = crossterm::execute!(
                    io::stdout(),
                    MoveToPreviousLine(current_viewport_height.saturating_sub(1)),
                    Clear(ClearType::FromCursorDown)
                );
                if let Ok(new_term) = Terminal::with_options(
                    CrosstermBackend::new(io::stdout()),
                    TerminalOptions {
                        viewport: Viewport::Inline(needed_height),
                    },
                ) {
                    terminal = new_term;
                    current_viewport_height = needed_height;
                }
            }

            terminal.draw(|frame| {
                let area = frame.area();
                let input_widget = Paragraph::new(display_text)
                    .style(Style::default().fg(Color::White))
                    .block(Block::default().borders(Borders::TOP | Borders::BOTTOM));
                frame.render_widget(input_widget, area);

                // Posicionar el cursor en la línea y columna exactas tras el formateo explícito
                let cursor_x = area.x + target_x;
                let cursor_y = area.y + 1 + target_y;
                frame.set_cursor_position((cursor_x, cursor_y));
            })?;

            if event::poll(std::time::Duration::from_millis(30))?
                && let Event::Key(key) = event::read()?
                && key.kind == crossterm::event::KeyEventKind::Press
            {
                match (key.code, key.modifiers) {
                    // Salir
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) | (KeyCode::Esc, _) => {
                        let _ = terminal.clear();
                        let _ = crossterm::execute!(
                            io::stdout(),
                            MoveToPreviousLine(current_viewport_height.saturating_sub(1)),
                            Clear(ClearType::FromCursorDown)
                        );
                        disable_raw_mode()?;
                        println!("\nSaliendo de Typhon...");
                        let _ = input_tx.send(UserInput::Exit);
                        let _ = agent_handle.await;
                        return Ok(());
                    }
                    // Insertar salto de línea con Alt+Enter o Shift+Enter
                    (KeyCode::Enter, KeyModifiers::ALT) | (KeyCode::Enter, KeyModifiers::SHIFT) => {
                        input_state.insert_char('\n');
                    }
                    // Enviar mensaje con Enter
                    (KeyCode::Enter, _) => {
                        let trimmed = input_state.text.trim().to_string();
                        input_state.clear();
                        if !trimmed.is_empty() {
                            break Some(trimmed);
                        }
                    }
                    // Movimiento entre palabras (Alt + Izquierda/Derecha o Ctrl + Izquierda/Derecha)
                    (KeyCode::Left, KeyModifiers::ALT) | (KeyCode::Left, KeyModifiers::CONTROL) => {
                        input_state.move_word_left();
                    }
                    (KeyCode::Right, KeyModifiers::ALT)
                    | (KeyCode::Right, KeyModifiers::CONTROL) => {
                        input_state.move_word_right();
                    }
                    // Movimiento carácter a carácter
                    (KeyCode::Left, _) => {
                        input_state.move_left();
                    }
                    (KeyCode::Right, _) => {
                        input_state.move_right();
                    }
                    // Inicio / Fin de línea
                    (KeyCode::Home, _) | (KeyCode::Char('a'), KeyModifiers::CONTROL) => {
                        input_state.move_home();
                    }
                    (KeyCode::End, _) | (KeyCode::Char('e'), KeyModifiers::CONTROL) => {
                        input_state.move_end();
                    }
                    // Borrado
                    (KeyCode::Backspace, _) => {
                        input_state.delete_prev_char();
                    }
                    (KeyCode::Delete, _) => {
                        input_state.delete_next_char();
                    }
                    // Insertar caracteres
                    (KeyCode::Char(c), KeyModifiers::NONE)
                    | (KeyCode::Char(c), KeyModifiers::SHIFT) => {
                        input_state.insert_char(c);
                    }
                    _ => {}
                }
            }
        };

        // Limpiar pantalla del viewport inline y deshabilitar raw mode antes de imprimir la respuesta
        let _ = terminal.clear();
        let _ = crossterm::execute!(
            io::stdout(),
            MoveToPreviousLine(current_viewport_height.saturating_sub(1)),
            Clear(ClearType::FromCursorDown)
        );
        disable_raw_mode()?;

        if let Some(prompt) = user_prompt {
            println!(
                "\r{}",
                style("────────────────────────────────────────").dim()
            );
            println!("{}\n", style(format!("❯ {}", prompt)).bold().green());
            io::stdout().flush()?;

            let _ = input_tx.send(UserInput::Message(prompt));

            let term_width = crossterm::terminal::size().map(|(w, _)| w).unwrap_or(80);
            let mut stream_processor = MarkdownStreamProcessor::new(term_width);

            // Streaming directo en vivo mientras la respuesta se escribe carácter a carácter
            while let Some(event) = event_rx.recv().await {
                match event {
                    AgentEvent::Text(t) => {
                        let _ = stream_processor.write_chunk(&t);
                    }
                    AgentEvent::Reasoning(t) => {
                        let _ = stream_processor.write_reasoning_chunk(&t);
                    }
                    AgentEvent::Error(err) => {
                        let _ = stream_processor.flush_final();
                        println!("\nError: {}", err);
                        break;
                    }
                    AgentEvent::StreamEnd { .. } => {
                        let _ = stream_processor.flush_final();
                        println!();
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

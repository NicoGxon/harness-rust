use agent_core::{AgentEvent, AgentRunner, UserInput};
use brain::{AgentSettings, Usage};
use console::style;
use crossterm::cursor::MoveToPreviousLine;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{Clear, ClearType, disable_raw_mode, enable_raw_mode};

use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Clear as ClearWidget, Paragraph, Wrap},
};
use std::io::{self, Write};
use tokio::sync::mpsc;

use super::commands::{self, Command, SessionInfo};
use super::input::InputState;
use super::markdown::MarkdownStreamProcessor;
use crate::config;

type TuiResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

/// Muestra información temporal debajo del contenido actual y vuelve a la TUI al pulsar Esc.
fn show_info_view(title: &str, content: &str) -> TuiResult {
    enable_raw_mode()?;

    let viewport_height = (content.lines().count() as u16 + 4).clamp(6, 20);
    let mut terminal = Terminal::with_options(
        CrosstermBackend::new(io::stdout()),
        TerminalOptions {
            viewport: Viewport::Inline(viewport_height),
        },
    )?;

    let result: TuiResult = (|| {
        loop {
            terminal.draw(|frame| {
                let body = format!("{}\n\nPresiona Esc para volver", content);

                frame.render_widget(ClearWidget, frame.area());
                frame.render_widget(
                    Paragraph::new(body)
                        .alignment(Alignment::Left)
                        .wrap(Wrap { trim: false })
                        .block(
                            Block::default()
                                .title(format!(" {} ", title))
                                .borders(Borders::ALL)
                                .style(Style::default().fg(Color::White)),
                        ),
                    frame.area(),
                );
            })?;

            if event::poll(std::time::Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
                && key.kind == crossterm::event::KeyEventKind::Press
                && key.code == KeyCode::Esc
            {
                break;
            }
        }

        Ok(())
    })();

    let _ = terminal.clear();
    let cleanup_result = crossterm::execute!(
        io::stdout(),
        MoveToPreviousLine(viewport_height.saturating_sub(1)),
        Clear(ClearType::FromCursorDown)
    );
    disable_raw_mode()?;
    cleanup_result?;
    result
}

fn aplicar_configuracion(
    result: anyhow::Result<Option<config::TyphonConfig>>,
    config: &mut config::TyphonConfig,
    session_info: &mut SessionInfo,
    input_tx: &mpsc::UnboundedSender<UserInput>,
) -> TuiResult {
    match result {
        Ok(Some(updated)) => {
            let settings = AgentSettings {
                provider: updated.provider,
                model: updated.model.clone(),
                preamble: updated.preamble.clone(),
                credential: updated.credential.clone(),
                temperature: updated.temperature,
                reasoning_effort: updated.reasoning_effort,
            };
            session_info.update(
                format!("{}", updated.provider),
                updated.model.clone(),
                updated.temperature,
                updated.reasoning_effort,
                updated.verbose,
                &updated.config_path,
                &updated.prompt_path,
            );
            *config = updated;
            let _ = input_tx.send(UserInput::Reconfigure(settings));
            show_info_view(
                "Configuración actualizada",
                &commands::config_text(session_info),
            )?;
        }
        Ok(None) => {}
        Err(error) => {
            show_info_view("Error de configuración", &error.to_string())?;
        }
    }

    Ok(())
}

pub async fn run_tui(
    runner: AgentRunner,
    mut config: config::TyphonConfig,
    mut session_info: SessionInfo,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let (input_tx, input_rx) = mpsc::unbounded_channel::<UserInput>();
    let (event_tx, mut event_rx) = mpsc::unbounded_channel::<AgentEvent>();

    // Spawn agent runner in background
    let agent_handle = tokio::spawn(async move { runner.run(input_rx, event_tx).await });

    let mut input_state = InputState::new();
    let mut session_usage = Usage::default();

    loop {
        // Habilitar raw mode para interactuar con la caja inline de Ratatui
        enable_raw_mode()?;

        let mut current_viewport_height = 4u16;
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

            let needed_height = (total_lines + 3).clamp(4, 13);
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
                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([
                        Constraint::Min(needed_height.saturating_sub(1)),
                        Constraint::Length(1),
                    ])
                    .split(area);

                let input_widget = Paragraph::new(display_text)
                    .style(Style::default().fg(Color::White))
                    .block(Block::default().borders(Borders::TOP | Borders::BOTTOM));
                frame.render_widget(input_widget, chunks[0]);

                let metrics_text = format!(
                    "  {}  |  Modelo: {} ({})",
                    commands::session_usage_text(&session_usage),
                    session_info.model,
                    session_info.provider
                );
                let metrics_widget =
                    Paragraph::new(metrics_text).style(Style::default().fg(Color::DarkGray));
                frame.render_widget(metrics_widget, chunks[1]);

                // Posicionar el cursor en la línea y columna exactas tras el formateo explícito
                let cursor_x = chunks[0].x + target_x;
                let cursor_y = chunks[0].y + 1 + target_y;
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
            match commands::parse(&prompt) {
                Command::Message(message) => {
                    println!(
                        "\r{}",
                        style("────────────────────────────────────────").dim()
                    );
                    println!("{}\n", style(format!("❯ {}", prompt)).bold().green());
                    io::stdout().flush()?;
                    let _ = input_tx.send(UserInput::Message(message));
                }
                Command::Help => {
                    show_info_view("Ayuda", commands::help_text())?;
                    continue;
                }
                Command::Status => {
                    show_info_view(
                        "Estado de la sesión",
                        &commands::status_text(&session_info, &session_usage),
                    )?;
                    continue;
                }
                Command::Tools => {
                    show_info_view("Herramientas", commands::tools_text())?;
                    continue;
                }
                Command::Config => {
                    show_info_view("Configuración", &commands::config_text(&session_info))?;
                    continue;
                }
                Command::Provider => {
                    aplicar_configuracion(
                        config::configurar_proveedor(&config).await,
                        &mut config,
                        &mut session_info,
                        &input_tx,
                    )?;
                    continue;
                }
                Command::Model => {
                    aplicar_configuracion(
                        config::configurar_modelo(&config).await,
                        &mut config,
                        &mut session_info,
                        &input_tx,
                    )?;
                    continue;
                }
                Command::Temperature => {
                    aplicar_configuracion(
                        config::configurar_temperatura(&config),
                        &mut config,
                        &mut session_info,
                        &input_tx,
                    )?;
                    continue;
                }
                Command::Reasoning => {
                    aplicar_configuracion(
                        config::configurar_razonamiento(&config),
                        &mut config,
                        &mut session_info,
                        &input_tx,
                    )?;
                    continue;
                }
                Command::Verbose => {
                    aplicar_configuracion(
                        config::configurar_verbose(&config),
                        &mut config,
                        &mut session_info,
                        &input_tx,
                    )?;
                    continue;
                }
                Command::Prompt => {
                    aplicar_configuracion(
                        config::configurar_prompt(&config),
                        &mut config,
                        &mut session_info,
                        &input_tx,
                    )?;
                    continue;
                }
                Command::PromptFile => {
                    aplicar_configuracion(
                        config::configurar_prompt_file(&config),
                        &mut config,
                        &mut session_info,
                        &input_tx,
                    )?;
                    continue;
                }
                Command::Clear => {
                    crossterm::execute!(io::stdout(), Clear(ClearType::All))?;
                    continue;
                }
                Command::New => {
                    let _ = input_tx.send(UserInput::ResetConversation);
                    let message = match event_rx.recv().await {
                        Some(AgentEvent::SystemMessage(message)) => message,
                        Some(other) => format!("No se pudo reiniciar la conversación: {:?}", other),
                        None => {
                            "No se pudo reiniciar la conversación: el agente terminó.".to_string()
                        }
                    };
                    show_info_view("Conversación nueva", &message)?;
                    session_usage = Usage::default();
                    continue;
                }
                Command::Exit => {
                    let _ = input_tx.send(UserInput::Exit);
                    let _ = agent_handle.await;
                    println!("Saliendo de Typhon...");
                    return Ok(());
                }
                Command::Unknown(command) => {
                    show_info_view("Comando no reconocido", &commands::unknown_text(&command))?;
                    continue;
                }
            }

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
                    AgentEvent::SessionUsage(usage) => {
                        session_usage = usage;
                    }
                    AgentEvent::Error(err) => {
                        let _ = stream_processor.flush_final();
                        println!("\nError: {}", err);
                        break;
                    }
                    AgentEvent::StreamEnd { usage, .. } => {
                        let _ = stream_processor.flush_final();
                        println!();
                        session_usage = usage;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

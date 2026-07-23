mod app;
pub mod input;
pub mod markdown;

#[cfg(test)]
mod tests;

pub use app::run_tui;

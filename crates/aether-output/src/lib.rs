pub mod discord;
pub mod error;
pub mod file;
pub mod pipeline;
mod serialize;
pub mod slack;
pub mod stdout;
pub mod telegram;

pub use discord::DiscordSink;
pub use file::FileSink;
pub use pipeline::OutputPipeline;
pub use slack::SlackSink;
pub use stdout::{OutputFormat, StdoutSink};
pub use telegram::TelegramSink;

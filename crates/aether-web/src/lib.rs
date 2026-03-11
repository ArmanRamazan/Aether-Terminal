pub mod api;
pub mod embedded;
pub mod error;
pub mod server;
pub mod state;
pub mod ws;

pub use error::WebError;
pub use server::{router, serve};
pub use state::SharedState;

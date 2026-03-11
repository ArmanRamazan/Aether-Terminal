pub mod api;
pub mod error;
pub mod server;
pub mod state;

pub use error::WebError;
pub use server::{router, serve};
pub use state::SharedState;

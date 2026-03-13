pub(crate) mod api;
pub(crate) mod embedded;
pub mod error;
pub mod server;
pub mod state;
pub(crate) mod ws;

pub use error::WebError;
pub use server::{router, serve};
pub use state::SharedState;

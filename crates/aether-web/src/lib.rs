pub(crate) mod api;
pub(crate) mod embedded;
pub(crate) mod error;
pub(crate) mod server;
pub(crate) mod state;
pub(crate) mod ws;

pub use error::WebError;
pub use server::{router, serve};
pub use state::SharedState;

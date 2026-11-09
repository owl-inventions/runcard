//! runcard turns a GPX track into a poster-style PNG card: one coloured stripe
//! per kilometre, the runner's name, the route outline and the race stats.

pub mod card;
pub mod error;
pub mod format;
pub mod geo;
pub mod gpx;
pub mod layout;
pub mod model;
pub mod palette;
pub mod pipeline;
pub mod render;
pub mod route;
pub mod server;
pub mod splits;
pub mod text;
pub mod variant;

pub use error::Error;
pub use model::*;
pub use variant::Variant;

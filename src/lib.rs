//! runcard turns a GPX track into a typographic PNG card: the event, the
//! finish time and the stats in a shared frame around one of six pictures of
//! the kilometre splits, each kilometre coloured by its pace.

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
pub mod theme;
pub mod variant;

pub use error::Error;
pub use model::*;
pub use theme::Theme;
pub use variant::Variant;

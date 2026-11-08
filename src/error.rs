//! Crate-wide error type and `Result` alias.

/// Everything that can go wrong between reading a GPX file and writing the PNG.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The file could not be parsed as GPX.
    #[error("invalid gpx: {0}")]
    Gpx(String),
    /// Reading the input or writing the output failed.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// The track carries no `<time>` elements, so no split can be timed.
    #[error("track has no timestamps, pace cannot be derived")]
    NoTimestamps,
    /// Some points carry a `<time>`, this one does not; `index` counts the
    /// track's points from 0.
    #[error("point {index} has no timestamp")]
    MissingTimestamp { index: usize },
    /// This point was recorded before the one preceding it; `index` counts the
    /// track's points from 0.
    #[error("timestamp at point {index} is earlier than the previous one")]
    TimeNotMonotonic { index: usize },
    /// A track needs at least two points to have a distance.
    #[error("track has fewer than two points")]
    TooFewPoints,
    /// The points span at most one metre, too little for a split.
    #[error("track is shorter than one metre")]
    TooShort,
    /// The SVG template failed to load or render.
    #[error(transparent)]
    Template(#[from] minijinja::Error),
    /// The rendered SVG could not be parsed or rasterised.
    #[error("svg error: {0}")]
    Svg(String),
    /// The raster could not be encoded as PNG.
    #[error(transparent)]
    Png(#[from] png::EncodingError),
}

/// Result with the crate's [`Error`].
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_match_the_contract() {
        assert_eq!(
            Error::Gpx("bad xml".into()).to_string(),
            "invalid gpx: bad xml"
        );
        assert_eq!(
            Error::NoTimestamps.to_string(),
            "track has no timestamps, pace cannot be derived"
        );
        assert_eq!(
            Error::TooFewPoints.to_string(),
            "track has fewer than two points"
        );
        assert_eq!(
            Error::MissingTimestamp { index: 7 }.to_string(),
            "point 7 has no timestamp"
        );
        assert_eq!(
            Error::TimeNotMonotonic { index: 2 }.to_string(),
            "timestamp at point 2 is earlier than the previous one"
        );
        assert_eq!(
            Error::TooShort.to_string(),
            "track is shorter than one metre"
        );
    }

    #[test]
    fn converts_into_anyhow() {
        fn assert_send_sync<T: Send + Sync + 'static>() {}
        assert_send_sync::<Error>();
        let _: anyhow::Error = Error::TooFewPoints.into();
    }
}

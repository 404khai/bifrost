use std::fmt;

/// A schema mismatch or a rename that would overwrite existing data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BifrostError {
    /// An existing value on the migration path must be an object.
    /// The empty path denotes the document root.
    ExpectedObject { path: &'static str },
    /// Both the source and destination keys exist in the terminal object.
    Conflict {
        path: &'static str,
        source: &'static str,
        destination: &'static str,
    },
}

impl fmt::Display for BifrostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExpectedObject { path } => {
                let location = if path.is_empty() {
                    "document root"
                } else {
                    path
                };
                write!(f, "expected an object at {location}")
            }
            Self::Conflict {
                path,
                source,
                destination,
            } => write!(
                f,
                "cannot rename {source:?} to {destination:?} at {path}: both keys exist"
            ),
        }
    }
}

impl std::error::Error for BifrostError {}

#[cfg(test)]
mod tests {
    use super::BifrostError;

    #[test]
    fn errors_describe_the_location_and_conflict() {
        assert_eq!(
            BifrostError::ExpectedObject { path: "" }.to_string(),
            "expected an object at document root"
        );
        assert_eq!(
            BifrostError::ExpectedObject { path: "/session" }.to_string(),
            "expected an object at /session"
        );
        assert_eq!(
            BifrostError::Conflict {
                path: "/session/terminal",
                source: "name",
                destination: "title",
            }
            .to_string(),
            "cannot rename \"name\" to \"title\" at /session/terminal: both keys exist"
        );
    }
}

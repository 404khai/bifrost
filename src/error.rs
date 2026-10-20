use std::fmt;

/// A schema mismatch or a rename that would overwrite existing data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BifrostError {
    /// Invalid JSON Pointer rule configuration.
    InvalidPointer(crate::PointerError),
    /// An existing value on the migration path must be an object.
    /// The empty path denotes the document root.
    ExpectedObject { path: String },
    /// Both the source and destination keys exist in the target object.
    Conflict {
        path: String,
        source: String,
        destination: String,
    },
}

impl fmt::Display for BifrostError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPointer(error) => write!(f, "invalid rule pointer: {error}"),
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

impl std::error::Error for BifrostError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::InvalidPointer(error) => Some(error),
            _ => None,
        }
    }
}

impl From<crate::PointerError> for BifrostError {
    fn from(error: crate::PointerError) -> Self {
        Self::InvalidPointer(error)
    }
}

#[cfg(test)]
mod tests {
    use super::BifrostError;

    #[test]
    fn errors_describe_the_location_and_conflict() {
        assert_eq!(
            BifrostError::ExpectedObject { path: "".into() }.to_string(),
            "expected an object at document root"
        );
        assert_eq!(
            BifrostError::ExpectedObject {
                path: "/session".into()
            }
            .to_string(),
            "expected an object at /session"
        );
        assert_eq!(
            BifrostError::Conflict {
                path: "/session/terminal".into(),
                source: "name".into(),
                destination: "title".into(),
            }
            .to_string(),
            "cannot rename \"name\" to \"title\" at /session/terminal: both keys exist"
        );
    }
}

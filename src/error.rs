#[derive(Debug)]
pub enum OsmGraphError {
    Network(reqwest::Error),
    XmlParse(quick_xml::DeError),
    EmptyGraph,
    NodeNotFound,
    LockPoisoned,
    GeocodingFailed(String),
    InvalidInput(String),
    Io(std::io::Error),
    PbfError(String),
}

impl std::fmt::Display for OsmGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsmGraphError::Network(e)         => write!(f, "Network error: {}", e),
            OsmGraphError::XmlParse(e)        => write!(f, "XML parse error: {}", e),
            OsmGraphError::EmptyGraph         => write!(f, "Graph is empty"),
            OsmGraphError::NodeNotFound       => write!(f, "No node found near the given coordinates"),
            OsmGraphError::LockPoisoned       => write!(f, "Internal cache lock was poisoned"),
            OsmGraphError::GeocodingFailed(p) => write!(f, "Could not geocode '{}'", p),
            OsmGraphError::InvalidInput(msg)  => write!(f, "Invalid input: {}", msg),
            OsmGraphError::Io(e)              => write!(f, "IO error: {}", e),
            OsmGraphError::PbfError(msg)      => write!(f, "PBF error: {}", msg),
        }
    }
}

impl std::error::Error for OsmGraphError {}

impl From<reqwest::Error> for OsmGraphError {
    fn from(e: reqwest::Error) -> Self { OsmGraphError::Network(e) }
}

impl From<quick_xml::DeError> for OsmGraphError {
    fn from(e: quick_xml::DeError) -> Self { OsmGraphError::XmlParse(e) }
}

impl From<std::io::Error> for OsmGraphError {
    fn from(e: std::io::Error) -> Self { OsmGraphError::Io(e) }
}

// Only compile the pyo3 conversion when building the Python extension.
#[cfg(feature = "extension-module")]
impl From<OsmGraphError> for pyo3::PyErr {
    fn from(e: OsmGraphError) -> Self {
        pyo3::exceptions::PyRuntimeError::new_err(e.to_string())
    }
}

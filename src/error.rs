use pyo3::exceptions::PyRuntimeError;
use pyo3::PyErr;

#[derive(Debug)]
pub enum OsmGraphError {
    Network(reqwest::Error),
    XmlParse(quick_xml::DeError),
    EmptyGraph,
    NodeNotFound,
    LockPoisoned,
}

impl std::fmt::Display for OsmGraphError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OsmGraphError::Network(e) => write!(f, "Network error: {}", e),
            OsmGraphError::XmlParse(e) => write!(f, "XML parse error: {}", e),
            OsmGraphError::EmptyGraph => write!(f, "Graph is empty"),
            OsmGraphError::NodeNotFound => write!(f, "No node found near the given coordinates"),
            OsmGraphError::LockPoisoned => write!(f, "Internal cache lock was poisoned"),
        }
    }
}

impl std::error::Error for OsmGraphError {}

impl From<reqwest::Error> for OsmGraphError {
    fn from(e: reqwest::Error) -> Self {
        OsmGraphError::Network(e)
    }
}

impl From<quick_xml::DeError> for OsmGraphError {
    fn from(e: quick_xml::DeError) -> Self {
        OsmGraphError::XmlParse(e)
    }
}

impl From<OsmGraphError> for PyErr {
    fn from(e: OsmGraphError) -> Self {
        PyRuntimeError::new_err(e.to_string())
    }
}

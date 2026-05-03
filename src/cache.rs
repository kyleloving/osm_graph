use lru::LruCache;
use std::sync::Mutex;
use std::path::PathBuf;
use lazy_static::lazy_static;
use crate::graph::SpatialGraph;
use crate::error::OsmGraphError;

type GraphCache = Mutex<LruCache<String, SpatialGraph>>;
type XmlCache = Mutex<LruCache<String, String>>;

lazy_static! {
    static ref GRAPH_CACHE: GraphCache =
        Mutex::new(LruCache::new(std::num::NonZeroUsize::new(100).unwrap()));
    static ref XML_CACHE: XmlCache =
        Mutex::new(LruCache::new(std::num::NonZeroUsize::new(20).unwrap()));
}

pub fn check_cache(key: &str) -> Result<Option<SpatialGraph>, OsmGraphError> {
    Ok(GRAPH_CACHE.lock().map_err(|_| OsmGraphError::LockPoisoned)?.get(key).cloned())
}

pub fn insert_into_cache(key: String, sg: SpatialGraph) -> Result<(), OsmGraphError> {
    GRAPH_CACHE.lock().map_err(|_| OsmGraphError::LockPoisoned)?.put(key, sg);
    Ok(())
}

pub fn check_xml_cache(query: &str) -> Result<Option<String>, OsmGraphError> {
    Ok(XML_CACHE.lock().map_err(|_| OsmGraphError::LockPoisoned)?.get(query).cloned())
}

pub fn insert_into_xml_cache(query: String, xml: String) -> Result<(), OsmGraphError> {
    XML_CACHE.lock().map_err(|_| OsmGraphError::LockPoisoned)?.put(query, xml);
    Ok(())
}

pub fn clear_cache() -> Result<(), OsmGraphError> {
    GRAPH_CACHE.lock().map_err(|_| OsmGraphError::LockPoisoned)?.clear();
    XML_CACHE.lock().map_err(|_| OsmGraphError::LockPoisoned)?.clear();
    Ok(())
}

// --- Disk-backed XML cache ---

/// FNV-1a 64-bit hash — stable across Rust versions, no dependencies.
fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 14695981039346656037;
    for byte in s.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(1099511628211);
    }
    hash
}

/// Returns the disk cache directory, overridable via `OSM_GRAPH_CACHE_DIR`.
/// Defaults to a `cache/` folder in the current working directory — the same
/// convention used by OSMnx, so researchers get persistent, visible caching
/// next to their notebooks and scripts.
pub fn disk_cache_dir() -> PathBuf {
    std::env::var_os("OSM_GRAPH_CACHE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("cache"))
}

fn disk_xml_path(query: &str) -> PathBuf {
    disk_cache_dir().join(format!("{:016x}.xml", fnv1a(query)))
}

/// Check the disk cache for a previously fetched Overpass XML response.
/// Returns None on any error — a cache miss is always safe.
pub fn check_disk_xml_cache(query: &str) -> Option<String> {
    std::fs::read_to_string(disk_xml_path(query)).ok()
}

/// Persist an Overpass XML response to disk. Best-effort — silently ignores errors.
pub fn write_disk_xml_cache(query: &str, xml: &str) {
    let dir = disk_cache_dir();
    if std::fs::create_dir_all(&dir).is_ok() {
        let _ = std::fs::write(disk_xml_path(query), xml);
    }
}

/// Delete all files in the disk cache directory.
pub fn clear_disk_cache() -> Result<(), OsmGraphError> {
    let dir = disk_cache_dir();
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disk_cache_round_trip() {
        // Use a nanosecond-suffixed temp dir to avoid collisions with parallel test runs
        let dir = std::env::temp_dir().join(format!(
            "osm_graph_test_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
        ));
        std::env::set_var("OSM_GRAPH_CACHE_DIR", &dir);

        write_disk_xml_cache("test_query", "<xml>hello</xml>");
        let result = check_disk_xml_cache("test_query");
        assert_eq!(result, Some("<xml>hello</xml>".to_string()));

        assert!(check_disk_xml_cache("other_query").is_none());

        let _ = std::fs::remove_dir_all(&dir);
        std::env::remove_var("OSM_GRAPH_CACHE_DIR");
    }
}

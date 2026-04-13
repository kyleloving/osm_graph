use lru::LruCache;
use std::sync::Mutex;
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

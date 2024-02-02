use lru::LruCache;
use std::sync::Mutex;
use lazy_static::lazy_static;

lazy_static! {
    static ref GRAPH_CACHE: Mutex<LruCache<String, petgraph::graph::DiGraph<crate::graph::XmlNode, crate::graph::XmlWay>>> = {
        let cache_size = std::num::NonZeroUsize::new(100).unwrap(); // Adjust the cache size as needed
        Mutex::new(LruCache::new(cache_size))
    };
}

pub fn check_cache(query: &str) -> Option<petgraph::graph::DiGraph<crate::graph::XmlNode, crate::graph::XmlWay>> {
    let mut cache = GRAPH_CACHE.lock().unwrap();
    cache.get(query).cloned()
}

pub fn insert_into_cache(query: String, graph: petgraph::graph::DiGraph<crate::graph::XmlNode, crate::graph::XmlWay>) {
    let mut cache = GRAPH_CACHE.lock().unwrap();
    cache.put(query, graph);
}

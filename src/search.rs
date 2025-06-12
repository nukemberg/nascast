use serde::Serialize;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

#[derive(Serialize, Debug)]
pub struct SearchIndexEntry {
    pub id: String,           // unique identifier (hash-based)
    pub title: String,        // media title
    pub year: Option<u16>,    // release year
    pub media_type: String,   // "movie", "series", or "episode"
    pub url: String,          // page URL for navigation
    pub poster_url: String,   // thumbnail for search results
    pub meta: String,         // searchable metadata: genre, actors, director, writer (space-separated)
}

#[derive(Serialize, Debug)]
pub struct SearchIndex {
    pub entries: Vec<SearchIndexEntry>,
}

impl SearchIndex {
    pub fn new() -> Self {
        SearchIndex {
            entries: Vec::new(),
        }
    }

    pub fn add_entry(&mut self, entry: SearchIndexEntry) {
        self.entries.push(entry);
    }
}

/// Helper function to create a unique ID from a path
pub fn generate_id(path: &std::path::Path, media_type: &str) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_str().hash(&mut hasher);
    media_type.hash(&mut hasher);
    format!("{}_{}", media_type, hasher.finish())
}

/// Helper function to build meta string from individual fields
pub fn build_meta_string(genre: Option<&str>, actors: Option<&str>, director: Option<&str>, writer: Option<&str>) -> String {
    let mut meta_parts = Vec::new();
    
    if let Some(g) = genre {
        meta_parts.push(g.to_string());
    }
    if let Some(a) = actors {
        meta_parts.push(a.to_string());
    }
    if let Some(d) = director {
        meta_parts.push(d.to_string());
    }
    if let Some(w) = writer {
        meta_parts.push(w.to_string());
    }
    
    meta_parts.join(" ")
}

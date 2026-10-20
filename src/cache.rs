use rusqlite::{Connection, OptionalExtension, Params, Result, params};
use serde::de::DeserializeOwned;
use std::path::Path;
use std::fs;

use crate::movie::MovieInfo;
use crate::tv::{TvSeriesInfo, EpisodeTemplateData};

pub struct MediaCache {
    conn: Connection,
}

impl MediaCache {
    /// Initialize a new cache connection
    pub fn new(cache_path: &Path) -> Result<Self> {
        // Create parent directory if it doesn't exist
        if let Some(parent) = cache_path.parent() {
            fs::create_dir_all(parent).ok();
        }

        let conn = Connection::open(cache_path)?;
        
        // Initialize the database tables
        conn.execute(
            "CREATE TABLE IF NOT EXISTS movies (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                year INTEGER NOT NULL,
                path_hash TEXT NOT NULL UNIQUE,
                json_data TEXT NOT NULL
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS tv_series (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                year INTEGER,
                json_data TEXT NOT NULL,
                UNIQUE(name)
            )",
            [],
        )?;

        conn.execute(
            "CREATE TABLE IF NOT EXISTS tv_episodes (
                id INTEGER PRIMARY KEY,
                series_name TEXT NOT NULL,
                season INTEGER NOT NULL,
                episode INTEGER NOT NULL,
                json_data TEXT NOT NULL,
                UNIQUE(series_name, season, episode)
            )",
            [],
        )?;

        Ok(MediaCache { conn })
    }

    /// Runs a single-row `SELECT json_data ...` query and deserializes the result.
    /// Rows that fail to deserialize are treated as cache misses.
    fn get_json<T: DeserializeOwned>(&self, sql: &str, params: impl Params) -> Result<Option<T>> {
        let json_data: Option<String> = self.conn.query_row(sql, params, |row| row.get(0)).optional()?;
        Ok(json_data.and_then(|json| serde_json::from_str(&json).ok()))
    }

    /// Store a movie in the cache
    pub fn store_movie(&self, movie: &MovieInfo, path_hash: &str) -> Result<()> {
        let json_data = serde_json::to_string(movie).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO movies (name, year, path_hash, json_data) VALUES (?1, ?2, ?3, ?4)",
            params![movie.name, movie.year, path_hash, json_data],
        )?;
        Ok(())
    }

    /// Retrieve a movie from the cache by path hash
    pub fn get_movie_by_path_hash(&self, path_hash: &str) -> Result<Option<MovieInfo>> {
        self.get_json("SELECT json_data FROM movies WHERE path_hash = ?1", [path_hash])
    }

    /// Store TV series info in the cache
    pub fn store_tv_series(&self, series: &TvSeriesInfo) -> Result<()> {
        let json_data = serde_json::to_string(series).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO tv_series (name, year, json_data) VALUES (?1, ?2, ?3)",
            params![series.name, series.year, json_data],
        )?;
        Ok(())
    }

    /// Retrieve TV series info from cache by name
    pub fn get_tv_series_by_name(&self, series_name: &str) -> Result<Option<TvSeriesInfo>> {
        self.get_json("SELECT json_data FROM tv_series WHERE name = ?1", [series_name])
    }

    /// Store TV episode info in the cache
    pub fn store_tv_episode(&self, series_name: &str, season: u8, episode: u8, data: &EpisodeTemplateData) -> Result<()> {
        let json_data = serde_json::to_string(data).unwrap_or_default();
        self.conn.execute(
            "INSERT OR REPLACE INTO tv_episodes (series_name, season, episode, json_data) VALUES (?1, ?2, ?3, ?4)",
            params![series_name, season, episode, json_data],
        )?;
        Ok(())
    }

    /// Retrieve TV episode info from cache
    pub fn get_tv_episode(&self, series_name: &str, season: u8, episode: u8) -> Result<Option<EpisodeTemplateData>> {
        self.get_json(
            "SELECT json_data FROM tv_episodes WHERE series_name = ?1 AND season = ?2 AND episode = ?3",
            params![series_name, season, episode],
        )
    }
}

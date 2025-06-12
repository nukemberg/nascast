use rusqlite::{Connection, Result, params};
use std::path::Path;
use serde_json;
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

    /// Store a movie in the cache
    pub fn store_movie(&self, movie: &MovieInfo, path_hash: &str) -> Result<()> {
        let json_data = serde_json::to_string(movie).unwrap_or_default();
        
        self.conn.execute(
            "INSERT OR REPLACE INTO movies (name, year, path_hash, json_data) 
             VALUES (?1, ?2, ?3, ?4)",
            params![
                movie.name,
                movie.year,
                path_hash,
                json_data
            ],
        )?;
        
        Ok(())
    }

    /// Retrieve a movie from the cache by path hash
    pub fn get_movie_by_path_hash(&self, path_hash: &str) -> Result<Option<MovieInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT json_data FROM movies WHERE path_hash = ?1"
        )?;
        
        let movie_iter = stmt.query_map([path_hash], |row| {
            let json_data: String = row.get(0)?;
            Ok(json_data)
        })?;
        
        for movie_result in movie_iter {
            if let Ok(json_data) = movie_result {
                if let Ok(movie) = serde_json::from_str::<MovieInfo>(&json_data) {
                    return Ok(Some(movie));
                }
            }
        }
        
        Ok(None)
    }

    /// Store TV series info in the cache
    pub fn store_tv_series(&self, series: &TvSeriesInfo) -> Result<()> {
        let json_data = serde_json::to_string(series).unwrap_or_default();
        
        self.conn.execute(
            "INSERT OR REPLACE INTO tv_series (name, year, json_data) 
             VALUES (?1, ?2, ?3)",
            params![
                series.name,
                series.year,
                json_data
            ],
        )?;
        
        Ok(())
    }

    /// Retrieve TV series info from cache by name
    pub fn get_tv_series_by_name(&self, series_name: &str) -> Result<Option<TvSeriesInfo>> {
        let mut stmt = self.conn.prepare(
            "SELECT json_data FROM tv_series WHERE name = ?1"
        )?;
        
        let series_iter = stmt.query_map([series_name], |row| {
            let json_data: String = row.get(0)?;
            Ok(json_data)
        })?;
        
        for series_result in series_iter {
            if let Ok(json_data) = series_result {
                if let Ok(series) = serde_json::from_str::<TvSeriesInfo>(&json_data) {
                    return Ok(Some(series));
                }
            }
        }
        
        Ok(None)
    }

    /// Store TV episode info in the cache
    pub fn store_tv_episode(&self, series_name: &str, season: u8, episode: u8, data: &EpisodeTemplateData) -> Result<()> {
        let json_data = serde_json::to_string(data).unwrap_or_default();
        
        self.conn.execute(
            "INSERT OR REPLACE INTO tv_episodes (series_name, season, episode, json_data) 
             VALUES (?1, ?2, ?3, ?4)",
            params![
                series_name,
                season,
                episode,
                json_data
            ],
        )?;
        
        Ok(())
    }

    /// Retrieve TV episode info from cache
    pub fn get_tv_episode(&self, series_name: &str, season: u8, episode: u8) -> Result<Option<EpisodeTemplateData>> {
        let mut stmt = self.conn.prepare(
            "SELECT json_data FROM tv_episodes WHERE series_name = ?1 AND season = ?2 AND episode = ?3"
        )?;
        
        let episode_iter = stmt.query_map(params![series_name, season, episode], |row| {
            let json_data: String = row.get(0)?;
            Ok(json_data)
        })?;
        
        for episode_result in episode_iter {
            if let Ok(json_data) = episode_result {
                if let Ok(episode_data) = serde_json::from_str::<EpisodeTemplateData>(&json_data) {
                    return Ok(Some(episode_data));
                }
            }
        }
        
        Ok(None)
    }
}

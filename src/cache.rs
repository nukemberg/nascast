use rusqlite::{Connection, OptionalExtension, Params, Result, params};
use serde::de::DeserializeOwned;
use std::path::Path;
use std::fs;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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

        // One row per (video file, language) looked up on OpenSubtitles. `content` is NULL when
        // nothing was found, so misses aren't searched again until they're stale.
        conn.execute(
            "CREATE TABLE IF NOT EXISTS subtitles (
                id INTEGER PRIMARY KEY,
                media_path TEXT NOT NULL,
                language TEXT NOT NULL,
                content TEXT,
                checked_at INTEGER NOT NULL,
                UNIQUE(media_path, language)
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

    /// Store the result of a subtitle lookup; `content` is `None` when nothing was found.
    pub fn store_subtitle(&self, media_path: &Path, language: &str, content: Option<&str>) -> Result<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO subtitles (media_path, language, content, checked_at) VALUES (?1, ?2, ?3, ?4)",
            params![media_path.to_string_lossy(), language, content, unix_now()],
        )?;
        Ok(())
    }

    /// Retrieve a subtitle lookup result: `Some(Some(content))` when found, `Some(None)` for a
    /// miss checked less than `retry_misses_after` ago, `None` when it needs (re)checking.
    pub fn get_subtitle(&self, media_path: &Path, language: &str, retry_misses_after: Duration) -> Result<Option<Option<String>>> {
        let row: Option<(Option<String>, i64)> = self.conn.query_row(
            "SELECT content, checked_at FROM subtitles WHERE media_path = ?1 AND language = ?2",
            params![media_path.to_string_lossy(), language],
            |row| Ok((row.get(0)?, row.get(1)?)),
        ).optional()?;
        Ok(match row {
            Some((Some(content), _)) => Some(Some(content)),
            Some((None, checked_at)) if unix_now() - checked_at < retry_misses_after.as_secs() as i64 => Some(None),
            _ => None,
        })
    }

    /// Retrieve TV episode info from cache
    pub fn get_tv_episode(&self, series_name: &str, season: u8, episode: u8) -> Result<Option<EpisodeTemplateData>> {
        self.get_json(
            "SELECT json_data FROM tv_episodes WHERE series_name = ?1 AND season = ?2 AND episode = ?3",
            params![series_name, season, episode],
        )
    }
}

fn unix_now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subtitle_cache() {
        let dir = tempfile::tempdir().unwrap();
        let cache = MediaCache::new(&dir.path().join("cache.sqlite")).unwrap();
        let video = Path::new("/tv/Show/S01E01.mkv");
        let week = Duration::from_secs(7 * 24 * 3600);

        assert_eq!(cache.get_subtitle(video, "en", week).unwrap(), None);
        cache.store_subtitle(video, "en", Some("1\n...")).unwrap();
        cache.store_subtitle(video, "he", None).unwrap();
        assert_eq!(cache.get_subtitle(video, "en", week).unwrap(), Some(Some("1\n...".to_string())));
        assert_eq!(cache.get_subtitle(video, "he", week).unwrap(), Some(None));
        // A stale miss is due for another lookup
        assert_eq!(cache.get_subtitle(video, "he", Duration::ZERO).unwrap(), None);
    }
}

//! Client for the OpenSubtitles REST API (https://opensubtitles.stoplight.io/docs/opensubtitles-api).
//!
//! Every request needs an API key (a "consumer" key from https://www.opensubtitles.com/consumers).
//! Downloads count against a daily per-user quota, which logging in with a username and
//! password raises.

use std::error::Error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::time::Duration;
use serde::Deserialize;
use serde_json::json;
use url::Url;

pub const DEFAULT_API_URL: &str = "https://api.opensubtitles.com/api/v1";
const USER_AGENT: &str = concat!("nascast v", env!("CARGO_PKG_VERSION"));
/// The API allows a handful of requests per second; stay well below that.
const REQUEST_INTERVAL: Duration = Duration::from_millis(300);

#[derive(Clone)]
pub struct Credentials {
    pub api_key: String,
    pub username: Option<String>,
    pub password: Option<String>,
    /// API root, [`DEFAULT_API_URL`] except in tests.
    pub api_url: String,
}

/// What to search subtitles for.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Query {
    Movie { imdb_id: u64 },
    Episode { series_imdb_id: u64, season: u8, episode: u8 },
}

/// Extracts the numeric IMDb id from an IMDb title URL such as `https://www.imdb.com/title/tt0942385`.
pub fn imdb_id(info_url: &Url) -> Option<u64> {
    info_url.path_segments()?
        .rfind(|s| !s.is_empty())?
        .strip_prefix("tt")?
        .parse()
        .ok()
}

/// OpenSubtitles' "moviehash": file size plus the 64-bit little-endian word sums of the first
/// and last 64 KiB. `None` for files too small to hash (or unreadable).
pub fn movie_hash(path: &Path) -> Option<String> {
    const CHUNK: u64 = 64 * 1024;
    let mut file = File::open(path).ok()?;
    let size = file.metadata().ok()?.len();
    if size < CHUNK * 2 {
        return None;
    }
    let mut sum_chunk = |offset: u64| -> Option<u64> {
        let mut buf = vec![0u8; CHUNK as usize];
        file.seek(SeekFrom::Start(offset)).ok()?;
        file.read_exact(&mut buf).ok()?;
        Some(buf.as_chunks::<8>().0.iter().fold(0u64, |acc, w| acc.wrapping_add(u64::from_le_bytes(*w))))
    };
    let hash = size.wrapping_add(sum_chunk(0)?).wrapping_add(sum_chunk(size - CHUNK)?);
    Some(format!("{:016x}", hash))
}

#[derive(Deserialize, Debug)]
struct LoginResponse {
    token: String,
    base_url: Option<String>,
    user: Option<LoginUser>,
}

#[derive(Deserialize, Debug)]
struct LoginUser {
    remaining_downloads: Option<i64>,
}

#[derive(Deserialize, Debug)]
struct SearchResponse {
    data: Vec<SearchResult>,
}

#[derive(Deserialize, Debug)]
struct SearchResult {
    attributes: SubtitleAttributes,
}

#[derive(Deserialize, Debug)]
struct SubtitleAttributes {
    #[serde(default)]
    download_count: u64,
    #[serde(default)]
    hearing_impaired: bool,
    #[serde(default)]
    ai_translated: bool,
    #[serde(default)]
    machine_translated: bool,
    #[serde(default)]
    moviehash_match: bool,
    #[serde(default)]
    files: Vec<SubtitleFile>,
}

#[derive(Deserialize, Debug)]
struct SubtitleFile {
    file_id: u64,
}

#[derive(Deserialize, Debug)]
struct DownloadResponse {
    link: String,
    remaining: Option<i64>,
}

/// Outcome of looking for a subtitle.
#[derive(Debug, PartialEq)]
pub enum Lookup {
    /// SRT content of the best match.
    Found(String),
    /// OpenSubtitles has nothing for this media in this language.
    NotFound,
}

pub struct OpenSubtitles {
    client: reqwest::blocking::Client,
    api_url: String,
    credentials: Credentials,
    token: Option<String>,
    logged_in: bool,
    remaining_downloads: Option<i64>,
    last_request: Option<std::time::Instant>,
}

impl OpenSubtitles {
    pub fn new(credentials: Credentials) -> Self {
        OpenSubtitles {
            client: reqwest::blocking::Client::builder().user_agent(USER_AGENT).build().unwrap(),
            api_url: credentials.api_url.trim_end_matches('/').to_string(),
            credentials,
            token: None,
            logged_in: false,
            remaining_downloads: None,
            last_request: None,
        }
    }

    /// Whether the daily download quota is known to be used up.
    pub fn quota_exhausted(&self) -> bool {
        self.remaining_downloads.is_some_and(|r| r <= 0)
    }

    fn throttle(&mut self) {
        if let Some(last) = self.last_request {
            if let Some(wait) = REQUEST_INTERVAL.checked_sub(last.elapsed()) {
                std::thread::sleep(wait);
            }
        }
        self.last_request = Some(std::time::Instant::now());
    }

    fn request(&mut self, method: reqwest::Method, path: &str) -> reqwest::blocking::RequestBuilder {
        self.throttle();
        let mut request = self.client.request(method, format!("{}{}", self.api_url, path))
            .header("Api-Key", &self.credentials.api_key)
            .header("Accept", "application/json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        request
    }

    /// Logs in once, if credentials were given. Failures are logged and downloads fall back
    /// to the anonymous quota.
    fn ensure_logged_in(&mut self) {
        if self.logged_in {
            return;
        }
        self.logged_in = true;
        let (Some(username), Some(password)) = (self.credentials.username.clone(), self.credentials.password.clone()) else {
            return;
        };
        let response = self.request(reqwest::Method::POST, "/login")
            .json(&json!({ "username": username, "password": password }))
            .send()
            .and_then(|r| r.error_for_status())
            .and_then(|r| r.json::<LoginResponse>());
        match response {
            Ok(login) => {
                // Logged-in users may be pointed at a different API host (e.g. for VIP accounts)
                if let Some(host) = login.base_url.filter(|h| !h.is_empty() && self.api_url == DEFAULT_API_URL) {
                    self.api_url = format!("https://{}/api/v1", host);
                }
                self.remaining_downloads = login.user.and_then(|u| u.remaining_downloads);
                self.token = Some(login.token);
                log::info!(target: "cli", "Logged in to OpenSubtitles as {} ({:?} downloads left today)", username, self.remaining_downloads);
            }
            Err(e) => log::warn!(target: "cli", "OpenSubtitles login failed, using anonymous downloads: {}", e),
        }
    }

    fn search(&mut self, query: Query, language: &str, movie_hash: Option<&str>) -> Result<Vec<SearchResult>, Box<dyn Error>> {
        // The API redirects unless parameters are lowercase and sorted alphabetically
        let mut params: Vec<(&str, String)> = vec![("languages", language.to_lowercase())];
        match query {
            Query::Movie { imdb_id } => params.push(("imdb_id", imdb_id.to_string())),
            Query::Episode { series_imdb_id, season, episode } => params.extend([
                ("episode_number", episode.to_string()),
                ("parent_imdb_id", series_imdb_id.to_string()),
                ("season_number", season.to_string()),
            ]),
        }
        if let Some(hash) = movie_hash {
            params.push(("moviehash", hash.to_string()));
        }
        params.sort();
        let response: SearchResponse = self.request(reqwest::Method::GET, "/subtitles")
            .query(&params)
            .send()?
            .error_for_status()?
            .json()?;
        Ok(response.data)
    }

    fn download(&mut self, file_id: u64) -> Result<String, Box<dyn Error>> {
        let response = self.request(reqwest::Method::POST, "/download")
            .json(&json!({ "file_id": file_id }))
            .send()?;
        if response.status() == reqwest::StatusCode::NOT_ACCEPTABLE {
            // 406: daily download quota exceeded
            self.remaining_downloads = Some(0);
            return Err("OpenSubtitles daily download quota exceeded".into());
        }
        let download: DownloadResponse = response.error_for_status()?.json()?;
        self.remaining_downloads = download.remaining;
        self.throttle();
        Ok(self.client.get(&download.link).send()?.error_for_status()?.text()?)
    }

    /// Finds and downloads the best subtitle for `query` in `language`. `video` is used to
    /// prefer subtitles synced to that exact file (via its moviehash).
    pub fn find_subtitle(&mut self, query: Query, language: &str, video: &Path) -> Result<Lookup, Box<dyn Error>> {
        if self.quota_exhausted() {
            return Err("OpenSubtitles daily download quota exceeded".into());
        }
        self.ensure_logged_in();
        let hash = movie_hash(video);
        let Some(file_id) = best_file(self.search(query, language, hash.as_deref())?) else {
            return Ok(Lookup::NotFound);
        };
        Ok(Lookup::Found(self.download(file_id)?))
    }
}

/// Picks the most trustworthy result: synced to the exact file, human-translated, not SDH,
/// then most downloaded.
fn best_file(mut results: Vec<SearchResult>) -> Option<u64> {
    results.retain(|r| !r.attributes.files.is_empty());
    results.sort_by_key(|r| {
        let a = &r.attributes;
        (!a.moviehash_match, a.machine_translated || a.ai_translated, a.hearing_impaired, std::cmp::Reverse(a.download_count))
    });
    results.first().map(|r| r.attributes.files[0].file_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_imdb_id() {
        assert_eq!(imdb_id(&Url::parse("https://www.imdb.com/title/tt0942385").unwrap()), Some(942385));
        assert_eq!(imdb_id(&Url::parse("https://www.imdb.com/title/tt14688458/").unwrap()), Some(14688458));
        assert_eq!(imdb_id(&Url::parse("https://www.imdb.com/title/").unwrap()), None);
    }

    #[test]
    fn test_movie_hash() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        let data: Vec<u8> = (0..200_000u32).map(|i| (i % 251) as u8).collect();
        file.write_all(&data).unwrap();
        // Expected value computed independently with the reference algorithm
        assert_eq!(movie_hash(file.path()).as_deref(), Some(EXPECTED_HASH));

        let small = tempfile::NamedTempFile::new().unwrap();
        assert_eq!(movie_hash(small.path()), None);
    }
    const EXPECTED_HASH: &str = "e19d5212c9812cd6";

    fn result(file_id: u64, downloads: u64, hash_match: bool, machine: bool, hi: bool) -> SearchResult {
        SearchResult { attributes: SubtitleAttributes {
            download_count: downloads, hearing_impaired: hi, ai_translated: false,
            machine_translated: machine, moviehash_match: hash_match, files: vec![SubtitleFile { file_id }],
        } }
    }

    #[test]
    fn test_best_file() {
        assert_eq!(best_file(vec![]), None);
        assert_eq!(best_file(vec![result(1, 10, false, false, false), result(2, 500, false, false, false)]), Some(2));
        assert_eq!(best_file(vec![result(1, 900, false, false, false), result(2, 5, true, false, false)]), Some(2));
        assert_eq!(best_file(vec![result(1, 900, false, true, false), result(2, 5, false, false, true), result(3, 1, false, false, false)]), Some(3));
    }

    /// Runs a fake OpenSubtitles API and records the requests it gets.
    fn fake_api(quota_left: i64) -> (String, Arc<Mutex<Vec<String>>>) {
        let log = Arc::new(Mutex::new(Vec::new()));
        let server_log = log.clone();
        let server = rouille::Server::new("127.0.0.1:0", move |request| {
            let auth = request.header("Authorization").unwrap_or("").to_string();
            let api_key = request.header("Api-Key").unwrap_or("").to_string();
            server_log.lock().unwrap().push(format!("{} {} key={} auth={}", request.method(), request.raw_url(), api_key, auth));
            match (request.method(), request.url().as_str()) {
                ("POST", "/api/v1/login") => rouille::Response::json(&json!({
                    "token": "tok", "base_url": "elsewhere.example", "user": { "remaining_downloads": quota_left }
                })),
                ("GET", "/api/v1/subtitles") if request.get_param("imdb_id").as_deref() == Some("404") => {
                    rouille::Response::json(&json!({ "data": [] }))
                }
                ("GET", "/api/v1/subtitles") => rouille::Response::json(&json!({ "data": [
                    { "attributes": { "language": "en", "download_count": 3, "files": [{ "file_id": 11 }] } },
                    { "attributes": { "language": "en", "download_count": 99, "files": [{ "file_id": 22 }] } },
                ] })),
                ("POST", "/api/v1/download") => {
                    let host = request.header("Host").unwrap_or("").to_string();
                    rouille::Response::json(&json!({ "link": format!("http://{}/file/22.srt", host), "remaining": quota_left - 1 }))
                }
                ("GET", "/file/22.srt") => rouille::Response::text("1\n00:00:01,000 --> 00:00:02,000\nHello\n"),
                _ => rouille::Response::empty_404(),
            }
        }).unwrap();
        let url = format!("http://{}/api/v1", server.server_addr());
        std::thread::spawn(move || server.run());
        (url, log)
    }

    fn credentials(api_url: &str) -> Credentials {
        Credentials { api_key: "key".into(), username: Some("user".into()), password: Some("pass".into()), api_url: api_url.into() }
    }

    #[test]
    fn test_find_subtitle_flow() {
        let (url, log) = fake_api(5);
        let mut client = OpenSubtitles::new(credentials(&url));
        let video = tempfile::NamedTempFile::new().unwrap();

        let found = client.find_subtitle(Query::Episode { series_imdb_id: 14688458, season: 1, episode: 2 }, "EN", video.path()).unwrap();
        assert_eq!(found, Lookup::Found("1\n00:00:01,000 --> 00:00:02,000\nHello\n".into()));
        assert_eq!(client.find_subtitle(Query::Movie { imdb_id: 404 }, "en", video.path()).unwrap(), Lookup::NotFound);
        assert_eq!(client.remaining_downloads, Some(4));

        let log = log.lock().unwrap();
        assert_eq!(log[0], "POST /api/v1/login key=key auth=");
        // Sorted, lowercase params; logged-in requests carry the token; base_url is only
        // followed for the real API
        assert_eq!(log[1], "GET /api/v1/subtitles?episode_number=2&languages=en&parent_imdb_id=14688458&season_number=1 key=key auth=Bearer tok");
        assert_eq!(log[2], "POST /api/v1/download key=key auth=Bearer tok");
        assert_eq!(log[4], "GET /api/v1/subtitles?imdb_id=404&languages=en key=key auth=Bearer tok");
        assert_eq!(log.len(), 5);
    }

    #[test]
    fn test_quota_exhausted() {
        let (url, log) = fake_api(0);
        let mut client = OpenSubtitles::new(credentials(&url));
        let video = tempfile::NamedTempFile::new().unwrap();
        client.ensure_logged_in();
        assert!(client.quota_exhausted());
        assert!(client.find_subtitle(Query::Movie { imdb_id: 1 }, "en", video.path()).is_err());
        assert_eq!(log.lock().unwrap().len(), 1, "no requests after the quota is used up");
    }
}

use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use regex::Regex;
use serde::{Serialize, Deserialize};
use url::Url;
use crate::cache::MediaCache;
use crate::subtitles::{self, SubtitleLanguage, SubtitleTrack};
use crate::media::{omdb_find, omdb_get_episode_metadata, playability_rank, poster_url, rotten_tomatoes_rating, MediaVersion, OmdbResponse, OmdbType, SeriesRef};

/// Holds information parsed directly from a TV Series folder path.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct TvSeriesMediaInfo {
    /// Series name derived from the folder name.
    pub name: String,
    /// Optional year derived from the folder name.
    pub year: Option<u16>,
    /// Path to the series folder.
    pub path: PathBuf,
    /// Episodes belonging to this series.
    pub episodes: Vec<TvEpisodeMediaInfo>,
    /// OMDB Data
    pub released: Option<String>,
    pub genre: Option<String>,
    pub plot: Option<String>,
    pub actors: Option<String>,
    pub language: Option<String>,
    pub country: Option<String>,
    pub poster_url: Option<String>,
    pub imdb_rating: Option<String>,
    pub total_seasons: Option<String>,
}

impl TvSeriesMediaInfo {
    fn new(name: String, year: Option<u16>, path: PathBuf, episodes: Vec<TvEpisodeMediaInfo>) -> Self {
        TvSeriesMediaInfo {
            name,
            year,
            path,
            episodes,
            released: None,
            genre: None,
            plot: None,
            actors: None,
            language: None,
            country: None,
            poster_url: None,
            imdb_rating: None,
            total_seasons: None,
        }
    }

    /// Copies OMDB series metadata onto the folder-derived series info.
    pub fn apply_series_info(&mut self, info: &TvSeriesInfo) {
        self.poster_url = Some(info.poster_url.to_string());
        self.released = Some(info.released.clone());
        self.genre = Some(info.genre.clone());
        self.plot = Some(info.plot.clone());
        self.actors = Some(info.actors.clone());
        self.language = Some(info.language.clone());
        self.country = Some(info.country.clone());
        self.imdb_rating = Some(info.imdb_rating.clone());
        self.total_seasons = Some(info.total_seasons.clone());
        self.year = info.year;
    }

    /// Absorbs the episodes of another folder holding the same series.
    pub fn merge(&mut self, other: TvSeriesMediaInfo) {
        self.episodes.extend(other.episodes);
    }

    /// Collapses multiple files of the same episode into one entry. The most playable file
    /// becomes the primary one and the rest are kept as alternates. Leaves episodes sorted.
    pub fn dedupe_episodes(&mut self) {
        self.episodes.sort_by(|a, b| {
            (a.season, a.episode, playability_rank(&a.path), &a.path)
                .cmp(&(b.season, b.episode, playability_rank(&b.path), &b.path))
        });
        let mut deduped: Vec<TvEpisodeMediaInfo> = Vec::with_capacity(self.episodes.len());
        for episode in self.episodes.drain(..) {
            match deduped.last_mut() {
                Some(primary) if (primary.season, primary.episode) == (episode.season, episode.episode) => {
                    let media_ref = episode.media_ref.clone().unwrap_or_default();
                    primary.alternates.push(MediaVersion::new(&episode.path, media_ref));
                    primary.alternates.extend(episode.alternates);
                }
                _ => deduped.push(episode),
            }
        }
        self.episodes = deduped;
    }
}

/// Holds information parsed directly from a TV Episode video file path.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct TvEpisodeMediaInfo {
    /// Name of the series this episode belongs to.
    pub series_name: String,
    /// Season number.
    pub season: u8,
    /// Episode number.
    pub episode: u8,
    /// Path to the episode file.
    pub path: PathBuf,
    /// Episode title from OMDB.
    pub title: Option<String>,
    /// Episode plot from OMDB.
    pub plot: Option<String>,
    /// Episode IMDb rating from OMDB.
    pub imdb_rating: Option<String>,
    /// Episode air date from OMDB.
    pub air_date: Option<String>,
    /// Episode director from OMDB.
    pub director: Option<String>,
    /// Play link to the episode file (relative or absolute URI)
    pub media_ref: Option<String>,
    /// Other files of the same episode.
    pub alternates: Vec<MediaVersion>,
    /// Subtitles for the primary file.
    pub subtitles: Vec<SubtitleTrack>,
}

impl TvEpisodeMediaInfo {
    /// Copies OMDB episode metadata onto the file-derived episode info.
    pub fn apply_episode_info(&mut self, info: EpisodeTemplateData) {
        self.title = Some(info.title);
        self.plot = info.plot;
        self.imdb_rating = info.imdb_rating;
        self.air_date = info.aired_date;
        self.director = info.director;
    }
}

/// OMDB-enriched TV series details.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct TvSeriesInfo {
    pub name: String,
    pub year: Option<u16>,
    pub director: String,
    pub info_url: Url,
    pub poster_url: Url,
    pub language: String,
    pub country: String,
    pub plot: String,
    pub genre: String,
    pub runtime: String,
    pub released: String,
    pub rated: String,
    pub actors: String,
    pub imdb_rating: String,
    pub total_seasons: String,
    pub rotten_tomatoes_rating: Option<String>,
}

// ---- Structs for HTML Template Data ----

/// Data structure for rendering a full TV series page.
#[derive(Serialize, Debug)]
pub struct SeriesPageTemplateData {
    /// OMDB-enriched details of the TV series, or None if unavailable.
    pub series_info: Option<TvSeriesInfo>,
    /// List of seasons, each containing its episodes.
    pub seasons: Vec<SeasonTemplateData>,
    /// Series name as detected from folder structure
    pub name: String,
    /// Languages any episode has subtitles in.
    pub subtitle_languages: Vec<SubtitleLanguage>,
}

impl SeriesPageTemplateData {
    /// Builds the series page data, grouping episodes by season in ascending order.
    pub fn new(series: &TvSeriesMediaInfo, series_info: Option<TvSeriesInfo>) -> Self {
        let mut seasons_map: BTreeMap<u8, Vec<EpisodeTemplateData>> = BTreeMap::new();
        for ep in &series.episodes {
            seasons_map.entry(ep.season).or_default().push(EpisodeTemplateData {
                title: ep.title.clone().unwrap_or_default(),
                episode_number: ep.episode,
                plot: ep.plot.clone(),
                imdb_rating: ep.imdb_rating.clone(),
                aired_date: ep.air_date.clone(),
                director: ep.director.clone(),
                media_ref: ep.media_ref.clone().unwrap_or_default(),
                alternates: ep.alternates.clone(),
                subtitles: ep.subtitles.clone(),
            });
        }
        let seasons = seasons_map.into_iter()
            .map(|(season_number, episodes)| SeasonTemplateData { season_number, episodes })
            .collect();
        let subtitle_languages = subtitles::languages_of(series.episodes.iter().flat_map(|ep| &ep.subtitles));
        SeriesPageTemplateData { series_info, seasons, name: series.name.clone(), subtitle_languages }
    }
}

/// Data structure for a single season within a series page template.
#[derive(Serialize, Deserialize, Debug)]
pub struct SeasonTemplateData {
    pub season_number: u8,
    pub episodes: Vec<EpisodeTemplateData>,
}

/// Data structure for a single episode within a season.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct EpisodeTemplateData {
    /// Episode-specific title (e.g., from OMDB).
    pub title: String,
    pub episode_number: u8,
    // season_number is available via the parent `SeasonTemplateData`.
    /// Episode-specific plot summary.
    pub plot: Option<String>,
    /// Episode-specific IMDb rating.
    pub imdb_rating: Option<String>,
    /// Episode air date from OMDB.
    pub aired_date: Option<String>,
    /// Episode director from OMDB.
    pub director: Option<String>,
    /// Generated URL/path to the media file for playback.
    pub media_ref: String,
    /// Other files of the same episode. Defaulted so episodes cached before this field existed still load.
    #[serde(default)]
    pub alternates: Vec<MediaVersion>,
    /// Subtitles for `media_ref`. Defaulted for the same reason as `alternates`.
    #[serde(default)]
    pub subtitles: Vec<SubtitleTrack>,
}

/// HTML id of an episode on its series page.
pub fn episode_anchor(season: u8, episode: u8) -> String {
    format!("s{}e{}", season, episode)
}

fn compile_all(patterns: &[&str]) -> Vec<Regex> {
    patterns.iter().map(|p| Regex::new(p).unwrap()).collect()
}

static TV_PATTERNS_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| compile_all(&[
    // Priority 1: Fullest match - Name, SxxExx
    r"(?i)(?P<name>.+?)[._ ]S(?P<season>\d{1,2})E(?P<episode>\d{1,2})",
    // Priority 1: Fullest match - Name, xxXxx
    r"(?i)(?P<name>.+?)[._ ](?P<season>\d{1,2})x(?P<episode>\d{1,2})",

    // Priority 2: SxxExx at start of filename (or after non-word char), no series name in file
    // e.g., "S01E01.mkv", "ignored-S01E01.mkv"
    r"(?i)(?:^|[^a-zA-Z0-9\p{L}])S(?P<season>\d{1,2})E(?P<episode>\d{1,2})",
    // Priority 2: xxXxx at start of filename (or after non-word char), no series name in file
    // e.g., "1x01.mkv"
    r"(?i)(?:^|[^a-zA-Z0-9\p{L}])(?P<season>\d{1,2})x(?P<episode>\d{1,2})",

    // Priority 3: Name then Eyy (season from folder context)
    r"(?i)(?P<name>.+?)[._ ]E(?P<episode>\d{1,2})",
    // Priority 3: Name then Episode yy (season from folder context)
    r"(?i)(?P<name>.+?)[._ ]Episode[._ ](?P<episode>\d{1,2})",

    // Priority 4: Eyy at start (season from folder context)
    r"(?i)(?:^|[^a-zA-Z0-9\p{L}])E(?P<episode>\d{1,2})",
    // Priority 4: Episode yy at start (season from folder context)
    r"(?i)(?:^|[^a-zA-Z0-9\p{L}])Episode[._ ](?P<episode>\d{1,2})",

    // Priority 5: Special compact format (e.g. tloop0108)
    // Assumes name part does not end with digits that could be season/episode
    r"(?i)(?P<name>[a-zA-Z ._-]+?)(?P<season>\d{2})(?P<episode>\d{2})[^\d\p{L}/]*$",
]));

static SEASON_FOLDER_PATTERNS_RE: LazyLock<Vec<Regex>> = LazyLock::new(|| compile_all(&[
    // Folder name "Season XX" or "SXX" (captures only season)
    r"(?i)^S(?:eason)?[._ ]?(?P<season>\d{1,2})$",
    // Series name followed by SXX or Season XX
    r"(?i)(?P<name>.+?)[._ ]S(?:eason)?[._ ]?(?P<season>\d{1,2})",
]));

/// Finds a year pattern (YYYY) within a string.
static YEAR_FINDER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?:\(|\[|\b)(\d{4})(?:\)|\]|\b)").unwrap());
/// Captures a name without its trailing year pattern.
static NAME_CLEANER_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^(.*?)(?:[._ ]*(?:\(|\[)?\d{4}(?:\)|\])?)?[._ ]*$").unwrap());
static PAREN_YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\(\d{4}\)").unwrap());
static YEAR_BEFORE_SXX_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)(\d{4})[._ ]*S\d{1,2}$").unwrap());

/// Returns the first plausible year (1900..2050) found in `s`.
fn find_year(s: &str) -> Option<u16> {
    let y = YEAR_FINDER_RE.captures(s)?.get(1)?.as_str().parse::<u16>().ok()?;
    (1900..2050).contains(&y).then_some(y)
}

/// Strips a trailing year pattern from `name`.
fn strip_trailing_year(name: &str) -> String {
    NAME_CLEANER_RE.captures(name)
        .and_then(|caps| caps.get(1))
        .map_or_else(|| name.to_string(), |m| m.as_str().trim().to_string())
}

fn dots_to_spaces(s: &str) -> String {
    s.replace('.', " ").trim().to_string()
}

pub fn parse_tv_episode_path(
    file_path: &Path,
    series_name_from_folder: Option<String>,
    season_from_folder: Option<u8>,
) -> Option<TvEpisodeMediaInfo> {
    let file_name = file_path.file_stem()?.to_str()?;
    // The first matching pattern decides; we don't fall through to later patterns.
    let caps = TV_PATTERNS_RE.iter().find_map(|re| re.captures(file_name))?;

    let series_name = caps.name("name").map(|m| dots_to_spaces(m.as_str())).or(series_name_from_folder)?;
    let season = caps.name("season").and_then(|m| m.as_str().parse::<u8>().ok()).or(season_from_folder)?;
    let episode = caps.name("episode").and_then(|m| m.as_str().parse::<u8>().ok())?;

    Some(TvEpisodeMediaInfo {
        series_name,
        season,
        episode,
        path: file_path.to_path_buf(),
        title: None,
        plot: None,
        imdb_rating: None,
        air_date: None,
        director: None,
        media_ref: None,
        alternates: Vec::new(),
        subtitles: Vec::new(),
    })
}

/// Parses a series or season folder name into `(series name, season, year)`.
pub fn parse_series_folder_name(folder_path: &Path) -> (Option<String>, Option<u8>, Option<u16>) {
    let folder_name = folder_path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    let mut parsed_year = find_year(folder_name);

    for re in SEASON_FOLDER_PATTERNS_RE.iter() {
        let Some(caps) = re.captures(folder_name) else { continue };
        let Some(season) = caps.name("season").and_then(|m| m.as_str().parse::<u8>().ok()) else { continue };
        let full_match = caps.get(0).unwrap();

        let mut series_name = match caps.name("name") {
            Some(name) => dots_to_spaces(name.as_str()),
            None => dots_to_spaces(&folder_name[..full_match.start()]),
        };

        if parsed_year.is_none() {
            parsed_year = find_year(&series_name);
        }
        // Keep the year as part of the name when it's in parens or directly before Sxx
        // (e.g. "Series.Name.2023.S01"); otherwise strip it.
        if parsed_year.is_some()
            && !PAREN_YEAR_RE.is_match(&series_name)
            && !YEAR_BEFORE_SXX_RE.is_match(folder_name)
        {
            series_name = strip_trailing_year(&series_name);
        }
        if series_name.is_empty() && folder_name.starts_with(full_match.as_str()) {
            return (None, Some(season), parsed_year);
        }
        return (Some(series_name).filter(|s| !s.is_empty()), Some(season), parsed_year);
    }

    // No season pattern matched: the whole folder name is the series name, minus any year.
    let name = dots_to_spaces(folder_name);
    let name = if parsed_year.is_some() { strip_trailing_year(&name) } else { name };
    (Some(name).filter(|s| !s.is_empty()), None, parsed_year)
}

/// Folders holding bonus material rather than episodes.
const NON_EPISODE_FOLDERS: &[&str] = &[
    "extras", "featurettes", "behind the scenes", "deleted scenes",
    "interviews", "trailers", "sample", "samples", "bonus",
];

fn is_non_episode_folder(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|name| NON_EPISODE_FOLDERS.contains(&name.to_lowercase().as_str()))
}

fn is_video_file(path: &Path) -> bool {
    path.is_file()
        && path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "mkv" | "mp4" | "avi" | "mov" | "wmv" | "flv" | "webm"))
}

/// Collects episodes from the video files directly inside `folder_path` (non-recursive).
fn collect_episodes_from_folder(
    folder_path: &Path,
    series_name: &str,
    season_from_folder: Option<u8>,
    episodes_list: &mut Vec<TvEpisodeMediaInfo>,
) -> Result<(), io::Error> {
    for file_entry in fs::read_dir(folder_path)? {
        let file_path = file_entry?.path();
        if is_video_file(&file_path) {
            episodes_list.extend(parse_tv_episode_path(&file_path, Some(series_name.to_string()), season_from_folder));
        }
    }
    Ok(())
}

/// Scans the main TV library directory to find all TV series and their episodes.
///
/// Supported layouts:
/// - `<Series>/<Season folder>/<episode files>` (season taken from the folder name)
/// - `<Series>/<other folder>/<episode files>` (season taken from file names; bonus-material
///   folders such as `Extras` are skipped)
/// - `<Series>/<episode files>`
/// - `<Series Season N>/<episode files>`
pub fn scan_tv_directory(tv_base_path: &Path) -> Result<Vec<TvSeriesMediaInfo>, io::Error> {
    let mut all_series_info: Vec<TvSeriesMediaInfo> = Vec::new();

    for series_entry in fs::read_dir(tv_base_path)? {
        let series_folder_path = series_entry?.path();
        if !series_folder_path.is_dir() {
            continue;
        }

        let (series_name, season_from_series_folder, year) = parse_series_folder_name(&series_folder_path);
        // Top-level folders not recognized as a series are skipped.
        let Some(series_name) = series_name else { continue };

        let mut episodes: Vec<TvEpisodeMediaInfo> = Vec::new();
        if season_from_series_folder.is_some() {
            collect_episodes_from_folder(&series_folder_path, &series_name, season_from_series_folder, &mut episodes)?;
        } else {
            for item_entry in fs::read_dir(&series_folder_path)? {
                let item_path = item_entry?.path();
                if item_path.is_dir() && is_non_episode_folder(&item_path) {
                    continue;
                } else if item_path.is_dir() {
                    // Season subfolder ("Season 1", "S02") or other folder ("Extras"), in which
                    // case the season must come from file names.
                    let (_, season_from_sub_dir, _) = parse_series_folder_name(&item_path);
                    collect_episodes_from_folder(&item_path, &series_name, season_from_sub_dir, &mut episodes)?;
                } else if is_video_file(&item_path) {
                    episodes.extend(parse_tv_episode_path(&item_path, Some(series_name.clone()), None));
                }
            }
        }

        if !episodes.is_empty() {
            episodes.sort_by_key(|ep| (ep.season, ep.episode));
            all_series_info.push(TvSeriesMediaInfo::new(series_name, year, series_folder_path, episodes));
        }
    }
    all_series_info.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(all_series_info)
}

pub fn get_series_info(
    omdb_api_key: &str,
    series_name: &str,
    cache: Option<&MediaCache>,
) -> Result<TvSeriesInfo, Box<dyn std::error::Error>> {
    if let Some(media_cache) = cache {
        if let Some(cached_series_info) = media_cache.get_tv_series_by_name(series_name)? {
            log::info!(target: "cli", "Cache hit for TV series: {}", series_name);
            return Ok(cached_series_info);
        }
    }

    log::info!(target: "cli", "Cache miss for TV series: {}. Fetching from OMDB.", series_name);
    let r = omdb_find(omdb_api_key, OmdbType::Series, series_name, None)?;
    let info_url = r.imdb_url();
    let OmdbResponse::Series {
        title, year, director, poster, language, country, plot, genre,
        runtime, released, rated, actors, imdb_rating, total_seasons, ratings, ..
    } = r else {
        return Err(format!("Unexpected OMDB response type for series: {}", series_name).into());
    };

    let series_info = TvSeriesInfo {
        name: title,
        // `year` is a range such as "2008-2013" or "2008-"; keep the start year.
        year: year.split(|c: char| !c.is_ascii_digit()).next().and_then(|s| s.parse().ok()),
        director,
        info_url,
        poster_url: poster_url(&poster),
        language,
        country,
        plot,
        genre,
        runtime,
        released,
        rated,
        actors,
        imdb_rating,
        total_seasons,
        rotten_tomatoes_rating: rotten_tomatoes_rating(&ratings),
    };

    if let Some(media_cache) = cache {
        match media_cache.store_tv_series(&series_info) {
            Ok(()) => log::info!(target: "cli", "Stored TV series '{}' in cache", series_info.name),
            Err(e) => log::error!("Failed to store TV series '{}' in cache: {}", series_info.name, e),
        }
    }
    Ok(series_info)
}

/// Episode metadata, cached under `series_name`. Looked up by the series' IMDb id when known,
/// since the folder name may not be the series title OMDB knows.
pub fn get_episode_info(
    omdb_api_key: &str,
    series_name: &str,
    series_imdb_id: Option<&str>,
    season: u8,
    episode: u8,
    cache: Option<&MediaCache>,
) -> Result<EpisodeTemplateData, Box<dyn std::error::Error>> {
    if let Some(media_cache) = cache {
        if let Some(cached_episode_info) = media_cache.get_tv_episode(series_name, season, episode)? {
            log::info!(target: "cli", "Cache hit for TV episode: {} S{:02}E{:02}", series_name, season, episode);
            return Ok(cached_episode_info);
        }
    }

    log::info!(target: "cli", "Cache miss for TV episode: {} S{:02}E{:02}. Fetching from OMDB.", series_name, season, episode);
    let series = series_imdb_id.map_or(SeriesRef::Title(series_name), SeriesRef::ImdbId);
    let r = omdb_get_episode_metadata(omdb_api_key, series, season, episode)?;
    let OmdbResponse::Episode { title, plot, imdb_rating, released, director, .. } = r else {
        return Err(format!("OMDB response was not an episode for: {} S{:02}E{:02}", series_name, season, episode).into());
    };

    let episode_data = EpisodeTemplateData {
        title,
        episode_number: episode,
        plot: Some(plot),
        imdb_rating: Some(imdb_rating),
        // OMDB 'Released' is the air date for episodes
        aired_date: Some(released),
        director: Some(director),
        // Populated later by the caller
        media_ref: String::new(),
        alternates: Vec::new(),
        subtitles: Vec::new(),
    };

    if let Some(media_cache) = cache {
        match media_cache.store_tv_episode(series_name, season, episode, &episode_data) {
            Ok(()) => log::info!(target: "cli", "Stored TV episode '{}' S{:02}E{:02} in cache", series_name, season, episode),
            Err(e) => log::error!("Failed to store TV episode '{}' S{:02}E{:02} in cache: {}", series_name, season, episode, e),
        }
    }
    Ok(episode_data)
}

// ---- Tests ----

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::fs; // Ensure fs is imported for test setup
     // Import tempfile crate

    fn assert_parsed_episode(
        path_str: &str,
        expected_series: &str,
        expected_season: u8,
        expected_episode: u8,
        series_from_folder: Option<&str>,
        season_from_folder: Option<u8>,
    ) {
        let path = Path::new(path_str);
        let result = parse_tv_episode_path(
            path,
            series_from_folder.map(String::from),
            season_from_folder,
        );
        assert!(
            result.is_some(),
            "Failed to parse: {}. Series from folder: {:?}, Season from folder: {:?}",
            path_str, series_from_folder, season_from_folder
        );
        let info = result.unwrap();
        assert_eq!(info.series_name, expected_series, "Mismatch in series name for {}", path_str);
        assert_eq!(info.season, expected_season, "Mismatch in season number for {}", path_str);
        assert_eq!(info.episode, expected_episode, "Mismatch in episode number for {}", path_str);
    }

    #[test]
    fn test_parse_series_folder_name() {
        assert_eq!(parse_series_folder_name(Path::new("Series Name Season 1")), (Some("Series Name".to_string()), Some(1), None));
        assert_eq!(parse_series_folder_name(Path::new("Series Name S01")), (Some("Series Name".to_string()), Some(1), None));
        assert_eq!(parse_series_folder_name(Path::new("Series Name (2023) Season 1")), (Some("Series Name (2023)".to_string()), Some(1), Some(2023))); // This needs refinement if year is part of name
        assert_eq!(parse_series_folder_name(Path::new("Series.Name.2023.S01")), (Some("Series Name 2023".to_string()), Some(1), Some(2023)));
        assert_eq!(parse_series_folder_name(Path::new("Series Name")), (Some("Series Name".to_string()), None, None));
        assert_eq!(parse_series_folder_name(Path::new("Series Name (2020)")), (Some("Series Name".to_string()), None, Some(2020)));
         assert_eq!(parse_series_folder_name(Path::new("tales.from.the.loop.2020.season.01")), (Some("tales from the loop".to_string()), Some(1), Some(2020)));
    }

    #[test]
    fn test_tv_filename_parsing() {
        // Case 1: Silo
        assert_parsed_episode(
            "/Users/avishai/Movies/TV/Silo.S01.COMPLETE.720p.ATVP.WEBRip.x264-GalaxyTV[TGx]/Silo.S01E06.720p.ATVP.WEBRip.x264-GalaxyTV.mkv",
            "Silo", 1, 6, Some("Silo"), Some(1)
        );

        // Case 2: Mythic Quest
        assert_parsed_episode(
            "/Users/avishai/Movies/TV/Mythic.Quest.Ravens.Banquet.S01.COMPLETE.XviD-AFG[TGx]/Mythic.Quest.Ravens.Banquet.S01E01.XviD-AFG.avi",
            "Mythic Quest Ravens Banquet", 1, 1, Some("Mythic Quest Ravens Banquet"), Some(1)
        );

        // Case 3: Danger 5
        assert_parsed_episode(
            "/Users/avishai/Movies/TV/Danger.5.S01/Danger.5.S01E01.HDTV.XviD-tellymad.I.Danced.for.Hitler.avi",
            "Danger 5", 1, 1, Some("Danger 5"), Some(1)
        );

        // Case 4: Scavengers Reign (series name from folder, also in filename)
        assert_parsed_episode(
            "/Users/avishai/Movies/TV/Scavengers Reign/Scavengers.Reign.S01E02.1080p.HEVC.x265-MeGusta[eztv.re].mkv",
            "Scavengers Reign", 1, 2, Some("Scavengers Reign"), None // Season not in this specific folder name
        );

        // Case 5: Tales from the Loop (tloop0108 format)
        assert_parsed_episode(
            "/Users/avishai/Movies/TV/tales.from.the.loop.2020.season.01/tloop0108.mp4",
            "tloop", 1, 8, Some("tales from the loop 2020"), Some(1)
        );
        
        // Case 6: Episode only in filename, season from folder
        assert_parsed_episode(
            "/Users/avishai/Movies/TV/Some.Show.Season.2/Some.Show.E03.mkv",
            "Some Show", 2, 3, Some("Some Show"), Some(2)
        );
         assert_parsed_episode(
            "/Users/avishai/Movies/TV/Another.Show.S04/Another.Show.Episode.05.mkv",
            "Another Show", 4, 5, Some("Another Show"), Some(4)
        );
    }

    fn episode(season: u8, episode: u8, path: &str) -> TvEpisodeMediaInfo {
        TvEpisodeMediaInfo {
            series_name: "Show".to_string(),
            season,
            episode,
            path: PathBuf::from(path),
            title: None,
            plot: None,
            imdb_rating: None,
            air_date: None,
            director: None,
            media_ref: Some(format!("tv/{}", path)),
            alternates: vec![],
            subtitles: vec![],
        }
    }

    #[test]
    fn test_dedupe_episodes() {
        let mut series = TvSeriesMediaInfo::new("Show".to_string(), None, PathBuf::from("Show"), vec![
            episode(1, 2, "Show.S01E02.mkv"),
            episode(1, 1, "Show.S01E01.mkv"),
            episode(1, 1, "Show.S01E01.mp4"),
            episode(1, 1, "Show.S01E01.avi"),
        ]);
        series.dedupe_episodes();

        let numbers: Vec<_> = series.episodes.iter().map(|e| (e.season, e.episode)).collect();
        assert_eq!(numbers, vec![(1, 1), (1, 2)]);
        // mp4 is preferred as the primary file, the rest become alternates in playability order
        assert_eq!(series.episodes[0].path, PathBuf::from("Show.S01E01.mp4"));
        assert_eq!(series.episodes[0].alternates, vec![
            MediaVersion { name: "Show.S01E01.mkv".to_string(), media_ref: "tv/Show.S01E01.mkv".to_string() },
            MediaVersion { name: "Show.S01E01.avi".to_string(), media_ref: "tv/Show.S01E01.avi".to_string() },
        ]);
        assert!(series.episodes[1].alternates.is_empty());
    }

    #[test]
    fn test_scan_tv_directory() {
        // Create a temporary directory structure for testing
        let base_dir = tempfile::Builder::new().prefix("test_scan_tv").tempdir().unwrap();
        let base_path = base_dir.path();

        let default_media_info = TvEpisodeMediaInfo {
            series_name: String::new(),
            season: 0,
            episode: 0,
            path: PathBuf::new(),
            title: None,
            plot: None,
            imdb_rating: None,
            air_date: None,
            director: None,
            media_ref: None,
            alternates: vec![],
            subtitles: vec![],
        };

        // Series 1: Standard structure
        let series1_folder_name = "Series One (2020)";
        let series1_path = base_path.join(series1_folder_name);
        fs::create_dir_all(&series1_path).unwrap();
        let s1_s01_path = series1_path.join("Season 01");
        fs::create_dir_all(&s1_s01_path).unwrap();
        let s1e1_path = s1_s01_path.join("S01E01.The.First.Episode.mkv");
        fs::File::create(&s1e1_path).unwrap();
        let s1e2_path = s1_s01_path.join("S01E02.The.Second.Episode.mp4");
        fs::File::create(&s1e2_path).unwrap();
        let s1_s02_path = series1_path.join("S02"); // Different season folder naming
        fs::create_dir_all(&s1_s02_path).unwrap();
        let s1_s2e1_path = s1_s02_path.join("Series.One.S02E01.avi");
        fs::File::create(&s1_s2e1_path).unwrap();
        // Bonus material must not show up as episodes
        let s1_extras_path = series1_path.join("Extras");
        fs::create_dir_all(&s1_extras_path).unwrap();
        fs::File::create(s1_extras_path.join("Series.One.S01E01.Featurette.mkv")).unwrap();

        // Series 2: Episodes directly in series folder
        let series2_folder_name = "Series Two";
        let series2_path = base_path.join(series2_folder_name);
        fs::create_dir_all(&series2_path).unwrap();
        let s2e1_path = series2_path.join("Series.Two.S01E01.mp4");
        fs::File::create(&s2e1_path).unwrap();

        // Series 3: Folder name includes season
        let series3_folder_name = "Series Three Season 1";
        let series3_path = base_path.join(series3_folder_name);
        fs::create_dir_all(&series3_path).unwrap();
        let s3e1_path = series3_path.join("S3E01.OnlyEp.mkv");
        fs::File::create(&s3e1_path).unwrap();
        let s3e2_path = series3_path.join("Series.Three.E02.mp4");
        fs::File::create(&s3e2_path).unwrap();

        // Series 4: Year in series name, and season folder
        let series4_folder_name = "Series Four (2021)";
        let series4_path = base_path.join(series4_folder_name);
        fs::create_dir_all(&series4_path).unwrap();
        let s4_s01_path = series4_path.join("Season 1");
        fs::create_dir_all(&s4_s01_path).unwrap();
        let s4e1_path = s4_s01_path.join("s01e01.episode.one.mkv");
        fs::File::create(&s4e1_path).unwrap();

        let mut s1e1 = default_media_info.clone();
        s1e1.series_name = "Series One".to_string();
        s1e1.season = 1;
        s1e1.episode = 1;
        s1e1.path = s1e1_path;

        let mut s1e2 = default_media_info.clone();
        s1e2.series_name = "Series One".to_string();
        s1e2.season = 1;
        s1e2.episode = 2;
        s1e2.path = s1e2_path;

        let mut s1s2e1 = default_media_info.clone();
        s1s2e1.series_name = "Series One".to_string();
        s1s2e1.season = 2;
        s1s2e1.episode = 1;
        s1s2e1.path = s1_s2e1_path;

        let mut s2e1 = default_media_info.clone();
        s2e1.series_name = "Series Two".to_string();
        s2e1.season = 1;
        s2e1.episode = 1;
        s2e1.path = s2e1_path;

        let mut s3e1 = default_media_info.clone();
        s3e1.series_name = "Series Three".to_string();
        s3e1.season = 3;
        s3e1.episode = 1;
        s3e1.path = s3e1_path;

        let mut s3e2 = default_media_info.clone();
        s3e2.series_name = "Series Three".to_string();
        s3e2.season = 1;
        s3e2.episode = 2;
        s3e2.path = s3e2_path;

        let mut s4e1 = default_media_info.clone();
        s4e1.series_name = "Series Four".to_string();
        s4e1.season = 1;
        s4e1.episode = 1;
        s4e1.path = s4e1_path;

        // Expected results: A Vec<TvSeriesMediaInfo>
        let expected_series_info = vec![
            TvSeriesMediaInfo {
                name: "Series Four".to_string(),
                year: Some(2021),
                path: series4_path.clone(),
                episodes: vec![s4e1],
                released: None,
                genre: None,
                plot: None,
                actors: None,
                language: None,
                country: None,
                poster_url: None,
                imdb_rating: None,
                total_seasons: None,
            },
            TvSeriesMediaInfo {
                name: "Series One".to_string(),
                year: Some(2020),
                path: series1_path.clone(),
                episodes: vec![s1e1, s1e2, s1s2e1],
                released: None,
                genre: None,
                plot: None,
                actors: None,
                language: None,
                country: None,
                poster_url: None,
                imdb_rating: None,
                total_seasons: None,
            },
            TvSeriesMediaInfo {
                name: "Series Three".to_string(),
                year: None,
                path: series3_path.clone(),
                episodes: vec![s3e2, s3e1],
                released: None,
                genre: None,
                plot: None,
                actors: None,
                language: None,
                country: None,
                poster_url: None,
                imdb_rating: None,
                total_seasons: None,
            },
            TvSeriesMediaInfo {
                name: "Series Two".to_string(),
                year: None,
                path: series2_path.clone(),
                episodes: vec![s2e1],
                released: None,
                genre: None,
                plot: None,
                actors: None,
                language: None,
                country: None,
                poster_url: None,
                imdb_rating: None,
                total_seasons: None,
            },
        ];

        let result = scan_tv_directory(base_path).unwrap();
        
        let mut expected_sorted_series_info = expected_series_info;
        for series in expected_sorted_series_info.iter_mut() {
            series.episodes.sort_by_key(|ep| (ep.season, ep.episode));
        }
        expected_sorted_series_info.sort_by(|a, b| a.name.cmp(&b.name));

        assert_eq!(result.len(), expected_sorted_series_info.len());

        for (res_series, exp_series) in result.iter().zip(expected_sorted_series_info.iter()) {
            assert_eq!(res_series.name, exp_series.name);
            assert_eq!(res_series.year, exp_series.year);
            assert_eq!(res_series.path, exp_series.path);
            assert_eq!(res_series.episodes.len(), exp_series.episodes.len());

            for (res_ep, exp_ep) in res_series.episodes.iter().zip(exp_series.episodes.iter()) {
                assert_eq!(res_ep.series_name, exp_ep.series_name);
                assert_eq!(res_ep.season, exp_ep.season);
                assert_eq!(res_ep.episode, exp_ep.episode);
                assert_eq!(res_ep.path, exp_ep.path);
            }
        }
    }
}
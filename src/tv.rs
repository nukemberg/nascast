use crate::media::MediaInfoEquiv; // Keep this for the trait
use lazy_static::lazy_static;
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::fs; // Added for directory reading
use std::io; // Added for io::Error
use url::Url;

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
}

impl MediaInfoEquiv for TvEpisodeMediaInfo {
    fn path(&self) -> &Path {
        &self.path
    }
}

// ---- OMDB-enriched Struct Definitions ----
// These structs hold data after enrichment from OMDB

#[derive(Serialize, Debug, PartialEq, Clone)]
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

#[derive(Serialize, Debug, PartialEq)] // Not Cloned as per original, add if necessary
pub struct TvEpisodeInfo {
    pub series_name: String,
    pub season: u8,
    pub episode: u8,
    pub episode_title: String,
    pub year: Option<u16>, // Year of the episode/series
    pub path: PathBuf,     // Path to the media file
    pub series_info: Option<TvSeriesInfo>, // Link to the overall series info
    pub imdb_rating: Option<String>,
    pub plot: Option<String>,
}

impl MediaInfoEquiv for TvEpisodeInfo {
    fn path(&self) -> &Path {
        &self.path
    }
}

// ---- Structs for HTML Template Data (using OMDB-enriched info) ----

// The `TvSeriesInfo` and `TvEpisodeInfo` structs defined above
// will be primary sources for these template structs.

/// Data structure for rendering a full TV series page.
#[derive(Serialize, Debug)]
pub struct SeriesPageTemplateData {
    /// OMDB-enriched details of the TV series.
    pub series_info: TvSeriesInfo,
    /// List of seasons, each containing its episodes.
    pub seasons: Vec<SeasonTemplateData>,
    /// Title for the HTML page.
    pub page_title: String,
}

/// Data structure for a single season within a series page template.
#[derive(Serialize, Debug)]
pub struct SeasonTemplateData {
    pub season_number: u8,
    pub episodes: Vec<EpisodeTemplateData>,
}

/// Data structure for a single episode within a series page template.
/// This will be populated from an instance of `TvEpisodeInfo` and
/// will include a generated media reference.
#[derive(Serialize, Debug, Clone)]
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
}

lazy_static! {
    pub static ref TV_PATTERNS_RE: Vec<Regex> = {
        let patterns = vec![
            // Priority 1: Fullest match - Name, SxxExx
            r"(?i)(?P<name>.+?)[._ ]S(?P<season>\\d{1,2})E(?P<episode>\\d{1,2})",
            // Priority 1: Fullest match - Name, xxXxx
            r"(?i)(?P<name>.+?)[._ ](?P<season>\\d{1,2})x(?P<episode>\\d{1,2})",
            
            // Priority 2: SxxExx at start of filename (or after non-word char), no series name in file
            // e.g., "S01E01.mkv", "ignored-S01E01.mkv"
            r"(?i)(?:^|[^a-zA-Z0-9\\p{L}])S(?P<season>\\d{1,2})E(?P<episode>\\d{1,2})",
            // Priority 2: xxXxx at start of filename (or after non-word char), no series name in file
            // e.g., "1x01.mkv"
            r"(?i)(?:^|[^a-zA-Z0-9\\p{L}])(?P<season>\\d{1,2})x(?P<episode>\\d{1,2})",

            // Priority 3: Name then Eyy (season from folder context)
            r"(?i)(?P<name>.+?)[._ ]E(?P<episode>\\d{1,2})",
            // Priority 3: Name then Episode yy (season from folder context)
            r"(?i)(?P<name>.+?)[._ ]Episode[._ ](?P<episode>\\d{1,2})",
            
            // Priority 4: Eyy at start (season from folder context)
            r"(?i)(?:^|[^a-zA-Z0-9\\p{L}])E(?P<episode>\\d{1,2})",
            // Priority 4: Episode yy at start (season from folder context)
            r"(?i)(?:^|[^a-zA-Z0-9\\p{L}])Episode[._ ](?P<episode>\\d{1,2})",
            
            // Priority 5: Special compact format (e.g. tloop0108)
            // Assumes name part does not end with digits that could be season/episode
            r"(?i)(?P<name>[a-zA-Z ._-]+?)(?P<season>\\d{2})(?P<episode>\\d{2})[^\\d\\p{L}/]*$",
        ];
        patterns.into_iter().map(|p| Regex::new(&p.replace("\\\\d", "\\d").replace("\\\\p{L}", "\\p{L}")).unwrap()).collect()
    };

    pub static ref SEASON_FOLDER_PATTERNS_RE: Vec<Regex> = {
        let patterns = vec![
            // Folder name "Season XX" or "SXX" (captures only season)
            r"(?i)^S(?:eason)?[._ ]?(?P<season>\\d{1,2})$",
            // Series name followed by SXX or Season XX
            r"(?i)(?P<name>.+?)[._ ]S(?:eason)?[._ ]?(?P<season>\\d{1,2})",
        ];
        patterns.into_iter().map(|p| Regex::new(&p.replace("\\\\d", "\\d")).unwrap()).collect() // Correct \\d to \d
    };
}

pub fn parse_tv_episode_path(
    file_path: &std::path::Path,
    series_name_from_folder: Option<String>,
    season_from_folder: Option<u8>,
) -> Option<TvEpisodeMediaInfo> {
    let file_name = file_path.file_stem()?.to_str()?;

    for re in TV_PATTERNS_RE.iter() {
        if let Some(caps) = re.captures(file_name) {
            let name_match = caps.name("name").map(|m| m.as_str().replace(".", " ").trim().to_string());
            let season_match = caps.name("season").and_then(|m| m.as_str().parse::<u8>().ok());
            let episode_match = caps.name("episode").and_then(|m| m.as_str().parse::<u8>().ok());

            let series_name = name_match.or_else(|| series_name_from_folder.clone())?;
            let season = season_match.or(season_from_folder)?;
            let episode = episode_match?;

            return Some(TvEpisodeMediaInfo {
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
            });
        }
    }
    None
}

pub fn parse_series_folder_name(folder_path: &std::path::Path) -> (Option<String>, Option<u8>, Option<u16>) {
    let folder_name = folder_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    // Regex to find a year pattern (YYYY) within a string.
    let year_finder_re = Regex::new(r"(?:\(|\[|\b)(\d{4})(?:\)|\]|\b)").unwrap();
    // Regex to help clean a name by removing a trailing year pattern.
    let name_cleaner_re = Regex::new(r"^(.*?)(?:[._ ]*(?:\(|\[)?\d{4}(?:\)|\])?)?[._ ]*$").unwrap();

    let mut parsed_year: Option<u16> = None;
    // First, try to find a year anywhere in the folder name.
    if let Some(caps) = year_finder_re.captures(folder_name) {
        if let Ok(y) = caps.get(1).unwrap().as_str().parse::<u16>() {
            // Basic sanity check for a plausible year.
            if y >= 1900 && y < 2050 {
                parsed_year = Some(y);
            }
        }
    }

    let name_candidate = folder_name.replace(".", " ").trim().to_string();

    // Try to parse season using existing SEASON_FOLDER_PATTERNS_RE
    for re in SEASON_FOLDER_PATTERNS_RE.iter() {
        if let Some(caps) = re.captures(folder_name) { // Apply to original folder_name
            let season_from_pattern = caps.name("season").and_then(|m| m.as_str().parse::<u8>().ok());

            if let Some(season_val) = season_from_pattern {
                let series_name_str;
                if let Some(name_match_from_season_re) = caps.name("name") {
                    series_name_str = name_match_from_season_re.as_str().replace(".", " ").trim().to_string();
                } else {
                    // If season pattern matched but had no "name" group (e.g. "Season 01" in "My Show Season 01")
                    // then the name is the part of folder_name before the season match.
                    if let Some(full_season_match) = caps.get(0) {
                         series_name_str = folder_name[..full_season_match.start()].replace(".", " ").trim().to_string();
                    } else {
                        // Fallback, should ideally not be reached if caps matched and gave a season.
                        series_name_str = name_candidate.clone();
                    }
                }

                // If a global year wasn't found, try to find one in this derived series_name_str
                if parsed_year.is_none() {
                    if let Some(ycaps) = year_finder_re.captures(&series_name_str) {
                         if let Ok(y) = ycaps.get(1).unwrap().as_str().parse::<u16>() {
                            if y >= 1900 && y < 2050 {
                                parsed_year = Some(y);
                            }
                        }
                    }
                }
                
                // Per tests like `Series Name (2020) Season 1` -> name `Series Name (2020)`,
                // the series_name_str should retain its form if it includes the year.
                // The `parsed_year` is separate.

                if series_name_str.is_empty() && folder_name.starts_with(caps.get(0).unwrap().as_str()){
                    // Handles cases like folder being just "Season 01", where series name isn't in the folder name.
                    return (None, Some(season_val), parsed_year);
                }
                return (Some(series_name_str).filter(|s| !s.is_empty()), Some(season_val), parsed_year);
            }
        }
    }

    // No season pattern matched. The whole folder_name is effectively the series name.
    // `parsed_year` holds any year found initially.
    // Now, we need to decide if the returned name string should have the year stripped if `parsed_year` is Some.
    // Test `("Series Name (2020)") -> (Some("Series Name"), None, Some(2020))` implies stripping.
    let final_name_str = if parsed_year.is_some() {
        if let Some(clean_caps) = name_cleaner_re.captures(&name_candidate) {
            clean_caps.get(1).map_or_else(|| name_candidate.clone(), |m| m.as_str().trim().to_string())
        } else {
            name_candidate
        }
    } else {
        name_candidate
    };
    
    (Some(final_name_str).filter(|s| !s.is_empty()), None, parsed_year)
}


// Helper function to check for common video file extensions
fn is_video_file(path: &Path) -> bool {
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        matches!(ext.to_lowercase().as_str(), "mkv" | "mp4" | "avi" | "mov" | "wmv" | "flv" | "webm")
    } else {
        false
    }
}

// Helper function to scan files within a given folder and collect episode information
fn collect_episodes_from_folder(
    folder_path: &Path,
    series_name: &str, // Ensure this is the clean series name, not including season/year if parsed separately
    season_from_folder: Option<u8>, // Contextual season number from folder structure
    episodes_list: &mut Vec<TvEpisodeMediaInfo>,
) -> Result<(), io::Error> {
    for file_entry in fs::read_dir(folder_path)? {
        let file_entry = file_entry?;
        let file_path = file_entry.path();

        if file_path.is_file() && is_video_file(&file_path) {
            if let Some(episode_info) = parse_tv_episode_path(
                &file_path,
                Some(series_name.to_string()),
                season_from_folder, // Pass the determined season context
            ) {
                episodes_list.push(episode_info);
            }
        }
        // Not recursing into subdirectories from here; scan_tv_directory handles structure.
    }
    Ok(())
}

/// Scans the main TV library directory to find all TV series and their episodes.
///
/// It navigates through series folders and season subfolders, using the
/// parsing functions to extract metadata for each episode and associate
/// it with its parent series.
pub fn scan_tv_directory(tv_base_path: &Path) -> Result<Vec<TvSeriesMediaInfo>, io::Error> { // Changed return type
    let mut all_series_info: Vec<TvSeriesMediaInfo> = Vec::new();

    for series_entry in fs::read_dir(tv_base_path)? {
        let series_entry = series_entry?;
        let series_folder_path = series_entry.path(); // Path to the potential series folder

        if !series_folder_path.is_dir() {
            continue; // Skip non-directories at the top level of the TV library
        }

        // Try to parse the main folder as a series.
        // It might also be a series folder that *includes* a season, e.g., "My Show Season 1"
        let (parsed_series_name, parsed_season_from_series_folder, parsed_year_from_series_folder) =
            parse_series_folder_name(&series_folder_path);

        if let Some(current_series_name_str) = parsed_series_name {
            let mut current_series_episodes: Vec<TvEpisodeMediaInfo> = Vec::new();

            // Case 1: The main series directory itself specifies a season (e.g., "My Show Season 1")
            // In this case, parsed_series_name_str is "My Show", and parsed_season_from_series_folder is Some(1)
            if let Some(main_dir_season_num) = parsed_season_from_series_folder {
                collect_episodes_from_folder(
                    &series_folder_path, // Scan files directly within this "Series Season X" directory
                    &current_series_name_str,
                    Some(main_dir_season_num), // Season context from this folder
                    &mut current_series_episodes,
                )?;
            } else {
                // Case 2: The main series directory is just "My Show".
                // Look for season subfolders OR episode files directly within it.
                for item_entry in fs::read_dir(&series_folder_path)? {
                    let item_entry = item_entry?;
                    let item_path = item_entry.path();

                    if item_path.is_dir() {
                        // Check if this subdirectory is a season folder (e.g., "Season 1", "S02")
                        // or another type of subfolder (e.g. "Extras")
                        let (_name_from_season_sub_dir, season_from_sub_dir, _year_from_sub_dir) =
                            parse_series_folder_name(&item_path);

                        if let Some(season_num) = season_from_sub_dir {
                            // It's a season subfolder
                            collect_episodes_from_folder(
                                &item_path, // Scan within this season subfolder
                                &current_series_name_str, // Inherit series name
                                Some(season_num),         // Season context from this subfolder
                                &mut current_series_episodes,
                            )?;
                        } else {
                            // It's a subdirectory but not recognized as a season folder (e.g., "Extras", "Specials").
                            // Scan it for episodes, relying on filenames for season/episode info.
                            // The series name is still current_series_name_str.
                            // No season context is provided by this sub-folder itself.
                            collect_episodes_from_folder(
                                &item_path,
                                &current_series_name_str,
                                None, 
                                &mut current_series_episodes,
                            )?;
                        }
                    } else if item_path.is_file() && is_video_file(&item_path) {
                        // It's an episode file directly in the series folder (e.g., "My Show/My.Show.S01E01.mkv")
                        // No season context from the immediate parent folder (series_folder_path).
                        if let Some(episode_info) = parse_tv_episode_path(
                            &item_path,
                            Some(current_series_name_str.clone()),
                            None, // No season context from this level of folder
                        ) {
                            current_series_episodes.push(episode_info);
                        }
                    }
                }
            }
            
            // After collecting all episodes for current_series_name_str,
            // create and add the TvSeriesMediaInfo if episodes were found.
            if !current_series_episodes.is_empty() {
                // Sort episodes by season and then episode number
                current_series_episodes.sort_by_key(|ep| (ep.season, ep.episode));
                
                all_series_info.push(TvSeriesMediaInfo {
                    name: current_series_name_str.clone(),
                    year: parsed_year_from_series_folder,
                    path: series_folder_path.to_path_buf(),
                    episodes: current_series_episodes,
                    poster_url: None, // Do not use placeholder, will be set from OMDB elsewhere if available
                    released: None,
                    genre: None,
                    plot: None,
                    actors: None,
                    language: None,
                    country: None,
                    imdb_rating: None,
                    total_seasons: None,
                });
            }
        }
        // If parsed_series_name is None, this folder at the TV root is not recognized as a series folder and is skipped.
    }
    // Sort series by name for consistent output
    all_series_info.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(all_series_info)
}

pub fn get_series_info(api_key: &str, series_name: &str) -> Result<TvSeriesInfo, Box<dyn std::error::Error>> {
    use crate::media::{OmdbType, omdb_get_metadata};
    
    let response = omdb_get_metadata(api_key, OmdbType::Series, series_name, None)?;
    
    // Convert OMDB response to TvSeriesInfo
    match response {
        crate::media::OmdbResponse::Series { 
            actors, 
            country, 
            director, 
            genre, 
            language, 
            plot, 
            poster, 
            rated, 
            ratings, 
            released, 
            runtime, 
            title, 
            year, 
            imdb_id, 
            imdb_rating, 
            total_seasons, 
            .. 
        } => {
            let info_url = format!("https://www.imdb.com/title/{}", imdb_id);
            let rotten_tomatoes_rating = ratings.iter()
                .find(|r| r.source == "Rotten Tomatoes")
                .map(|rt| rt.value.clone());
                
            Ok(TvSeriesInfo {
                name: title,
                year: year.split('–').next().and_then(|y| y.parse().ok()),
                director,
                info_url: Url::parse(&info_url)?,
                poster_url: Url::parse(&poster)?,
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
                rotten_tomatoes_rating,
            })
        },
        _ => Err("Expected Series response from OMDB but got Movie".into())
    }
}

pub fn get_episode_info(api_key: &str, series_name: &str, season: u8, episode: u8) -> Result<EpisodeTemplateData, Box<dyn std::error::Error>> {
    // Use omdb_get_metadata_with_season_episode for episode info
    let resp = omdb_get_metadata_with_season_episode(api_key, series_name, season, episode)?;
    
    Ok(EpisodeTemplateData {
        title: resp["Title"].as_str().unwrap_or_default().to_string(),
        episode_number: episode,
        plot: resp["Plot"].as_str()
            .filter(|p| *p != "N/A")
            .map(String::from),
        imdb_rating: resp["imdbRating"].as_str()
            .filter(|r| *r != "N/A")
            .map(String::from),
        aired_date: resp["Released"].as_str()
            .filter(|d| *d != "N/A")
            .map(String::from),
        director: resp["Director"].as_str()
            .filter(|d| *d != "N/A")
            .map(String::from),
        media_ref: String::new(), // Will be filled later
    })
}

// Helper function to call OMDB for episode info using blocking reqwest, matching omdb_get_metadata style
fn omdb_get_metadata_with_season_episode(api_key: &str, series_name: &str, season: u8, episode: u8) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let url = url::Url::parse_with_params(
        "https://www.omdbapi.com/",
        &[
            ("apiKey", api_key),
            ("t", series_name),
            ("Season", &season.to_string()),
            ("Episode", &episode.to_string()),
            ("type", "episode"),
        ],
    )?;
    let resp = reqwest::blocking::get(url)?.json::<serde_json::Value>()?;
    Ok(resp)
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::fs; // Ensure fs is imported for test setup
    use tempfile; // Import tempfile crate

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
         assert_eq!(parse_series_folder_name(Path::new("tales.from.the.loop.2020.season.01")), (Some("tales from the loop 2020".to_string()), Some(1), Some(2020)));
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
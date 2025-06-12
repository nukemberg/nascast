use std::path::PathBuf;
use regex::Regex;
use url::Url;
use serde_derive::{Serialize, Deserialize}; // Add Deserialize
use crate::media::{omdb_get_metadata, MediaInfo, MediaInfoEquiv, OmdbResponse, OmdbType};
use crate::cache::MediaCache; // Import MediaCache
use std::hash::{Hash, Hasher}; // For hashing
use std::collections::hash_map::DefaultHasher; // For hashing

#[derive(Serialize, Deserialize, Debug, PartialEq)] // Add Deserialize
pub struct MovieInfo {
    pub name: String,
    pub year: u16,
    pub director: String,
    pub path: PathBuf,
    pub info_url: Url,
    pub poster_url: Url,
    pub language: String,
    pub plot: String,
    pub genre: String,
    pub runtime: String,
    pub released: String,
    pub rated: String,
    pub actors: String,
    pub imdb_rating: String,
    pub rotten_tomatoes_rating: Option<String>
}

impl MediaInfoEquiv for MovieInfo {
    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

lazy_static! {
    pub static ref MOVIE_PATTERNS_RE: Vec<Regex> = [
        "(?P<name>[a-zA-Z0-9'.]+)\\.(?P<year>(?:19|20)\\d{2})\\..*",
        "(?P<name>[a-zA-Z0-9' ]+) [\\[\\()](?P<year>(?:19|20)\\d{2})[\\]\\)].*",
        "\\[[\\w\\s]]\\s+-\\s+(?:\\(\\w+\\)\\.)?(?P<name>[a-zA-Z0-9.']+)\\.(?P<year>(?:19|20)\\d{2})\\..*",
        "\\[[\\w\\s]]\\s+-\\s+(?:\\(\\w+\\) )?(?P<name>[a-zA-Z0-9' ]+) [\\[\\(](?P<year>(?:19|20)\\d{2})[\\]\\)].*",
        "(?P<name>[a-zA-Z0-9' ]+) (?P<year>(?:19|20)\\d{2}).*"
    ].iter().map(|pattern| Regex::new(pattern).unwrap()).collect();
}


pub fn parse_movie_filename(regexs: &Vec<Regex>, path: &PathBuf) -> Option<MediaInfo> {
    let filename = path.file_stem()?.to_str()?;
    let filename_match = regexs.iter().find_map(|re| re.captures(filename))?;
    let name = {
        let n = filename_match.name("name")?.as_str();
        if !n.contains(" ") && n.contains(".") {
            n.replace(".", " ")
        } else {
            n.to_owned()
        }
    };
    let year = filename_match.name("year").and_then(|s| s.as_str().parse::<u16>().ok());

    log::info!(target: "cli", "Media file discovered: {name:?} ({path:?})");

    Some(MediaInfo{name: name, year: year, path: path.to_owned()})
}

pub fn get_movie_info_logged(
    omdb_api_key: &str,
    movie_file_info: MediaInfo,
    cache: &Option<MediaCache>,
) -> Result<MovieInfo, Box<dyn std::error::Error>> {
    let name = movie_file_info.name.clone();
    let path_hash = {
        let mut hasher = DefaultHasher::new();
        movie_file_info.path.hash(&mut hasher);
        hasher.finish().to_string()
    };

    // Try to get from cache first
    if let Some(media_cache) = cache {
        if let Some(cached_movie_info) = media_cache.get_movie_by_path_hash(&path_hash).map_err(|e| e.to_string())? {
            log::info!(target: "cli", "Cache hit for movie (by path_hash {}): {}", path_hash, name);
            return Ok(cached_movie_info);
        }
    }

    log::info!(target: "cli", "Cache miss for movie (by path_hash {}): {}. Fetching from OMDB.", path_hash, name);
    let movie_info_result = get_movie_info(omdb_api_key, movie_file_info, cache, &path_hash); // Pass cache and path_hash

    match movie_info_result {
        Ok(info) => Ok(info),
        Err(err) => {
            log::warn!("Failed to get movie info for {}, error: {}", name, err);
            Err(err)
        }
    }
}

pub fn get_movie_info(
    omdb_api_key: &str,
    movie_file_info: MediaInfo,
    cache: &Option<MediaCache>,
    path_hash: &str, // Added path_hash parameter
) -> Result<MovieInfo, Box<dyn std::error::Error>> {
    let r = omdb_get_metadata(omdb_api_key, OmdbType::Movie, &movie_file_info.name, movie_file_info.year)?;
    match r {
        OmdbResponse::Movie { .. } => {
            let info_url = r.imdb_url();
            if let OmdbResponse::Movie {
                title,
                year,
                director,
                poster,
                language,
                plot,
                genre,
                runtime,
                released,
                rated,
                actors,
                imdb_rating,
                ratings,
                ..
            } = r
            {
                let rotten_tomatoes_rating = ratings.iter()
                    .find(|r_item| r_item.source == "Rotten Tomatoes")
                    .map(|r_item| r_item.value.to_string());

                let fetched_movie_info = MovieInfo {
                    name: title,
                    year: year.parse()?,
                    director,
                    poster_url: Url::parse(&poster)?,
                    language,
                    plot,
                    info_url,
                    path: movie_file_info.path,
                    genre,
                    runtime,
                    released,
                    rated,
                    actors,
                    imdb_rating,
                    rotten_tomatoes_rating,
                };

                // Store in cache
                if let Some(media_cache) = cache {
                    if let Err(e) = media_cache.store_movie(&fetched_movie_info, path_hash) {
                        log::error!("Failed to store movie '{}' in cache: {}", fetched_movie_info.name, e);
                    } else {
                        log::info!(target: "cli", "Stored movie '{}' in cache (path_hash: {})", fetched_movie_info.name, path_hash);
                    }
                }
                Ok(fetched_movie_info)
            } else {
                unreachable!()
            }
        }
        _ => Err("Wrong OMDB response".into()),
    }
}

#[cfg(test)]
mod tests {
    use crate::movie::{parse_movie_filename, MOVIE_PATTERNS_RE};
    use crate::media::MediaInfo;
    use std::path::Path;

    #[test]
    fn test_movie_name_parsing() {
        fn assert_movie_file_info(path: &str, name: &str, year: Option<u16>) {
            let _path = Path::new(path).to_path_buf();
            assert_eq!(parse_movie_filename(&MOVIE_PATTERNS_RE, &_path), Some(MediaInfo{path: _path, name: name.into(), year: year}));
        }

        assert_movie_file_info("movies/Journey.To.The.West.Conquering.The.Demons.2013.720p.WEBRip.x264.AC3-JYK.mp4", "Journey To The West Conquering The Demons", Some(2013));
        assert_movie_file_info("movies/Man On The Moon (1999) [1080p].mp4", "Man On The Moon", Some(1999));
        assert_movie_file_info("Movies/Movie 43 (2013) [1080p]/Movie.43.2013.1080p.BRrip.x264.GAZ.mp4", "Movie 43", Some(2013));
        assert_movie_file_info("Movies/The Kick [2011].x264.DVDrip(MartialArts).mp4", "The Kick", Some(2011));
        assert_movie_file_info("Movies/Tropic Thunder 2008 Unrated DC 1080p BluRay HEVC H265 5.1 BONE.mp4", "Tropic Thunder", Some(2008));
        assert_movie_file_info("Lesbian Vampire Killers 2009 720p BluRay x264 AAC-Mkvking.mkv", "Lesbian Vampire Killers", Some(2009));
    }
}
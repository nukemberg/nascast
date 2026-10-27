use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use regex::Regex;
use serde::{Serialize, Deserialize};
use url::Url;
use crate::media::{omdb_find, poster_url, rotten_tomatoes_rating, MediaInfo, OmdbResponse, OmdbType};
use crate::cache::MediaCache;

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
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

/// A year in a name: 19xx/20xx as a whole word, optionally in brackets.
static YEAR_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[\[(]?\b((?:19|20)\d{2})\b[\])]?").unwrap());
/// Junk before the title: `[Group] - `, `(Tag).`, `www.site.com - `.
static PREFIX_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:\s*\[[^\]]*\]\s*(?:-\s*)?|\s*\([^)]*\)\.|\s*www\.\S+\s+-\s+)+").unwrap()
});
/// A parenthesized aside at the end of a title, e.g. an original title.
static TRAILING_ASIDE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\([^)]*\)$").unwrap());

/// Parses a title and year from a file or folder name such as `Movie.Name.2013.720p.x264`,
/// `Movie Name (2013) [1080p]` or `[Group] - Movie Name 2013 BluRay`. The year is the first
/// bracketed one, else the first one after the start (so "2001 A Space Odyssey 1968" works).
pub fn parse_title(name: &str) -> Option<(String, Option<u16>)> {
    let name = PREFIX_RE.replace(name, "");
    let name = if name.contains(' ') { name.into_owned() } else { name.replace(['.', '_'], " ") };
    let years: Vec<_> = YEAR_RE.captures_iter(&name).filter(|c| c.get(0).unwrap().start() > 0).collect();
    let year_match = years.iter().find(|c| c.get(0).unwrap().as_str().len() > 4).or(years.first());
    let (title, year) = match year_match {
        Some(c) => (&name[..c.get(0).unwrap().start()], c[1].parse().ok()),
        None => (name.as_str(), None),
    };
    let title = title.trim_matches(|c: char| c.is_whitespace() || "-.([_".contains(c));
    let title = TRAILING_ASIDE_RE.replace(title, "").trim().to_string();
    (!title.is_empty()).then_some((title, year))
}

/// Works out a movie's title and year from its path: from the file name, or from the folder
/// it's in (below `root`) when the file name has no year.
pub fn parse_movie_path(path: &Path, root: &Path) -> Option<MediaInfo> {
    let from_file = path.file_stem().and_then(|s| s.to_str()).and_then(parse_title);
    let from_folder = || {
        let parent = path.parent().filter(|p| *p != root)?;
        parse_title(parent.file_name()?.to_str()?)
    };
    let (name, year) = match from_file {
        Some((name, Some(year))) => (name, Some(year)),
        from_file => from_folder().filter(|(_, year)| year.is_some()).or(from_folder()).or(from_file)?,
    };
    log::info!(target: "cli", "Media file discovered: {name:?} ({path:?})");
    Some(MediaInfo { name, year, path: path.to_owned() })
}

/// Whether a file is a sample/trailer clip rather than the movie itself.
pub fn is_sample(path: &Path) -> bool {
    path.components().rev().take(2).any(|c| {
        c.as_os_str().to_str().is_some_and(|s| {
            s.split(|c: char| !c.is_alphanumeric()).any(|w| w.eq_ignore_ascii_case("sample"))
        })
    })
}

/// Bumped when movie matching changes, so movies cached with an older (possibly wrong)
/// match are looked up again.
const MATCHING_VERSION: u32 = 2;

/// Cache key for a movie file. Note: this hashes the `PathBuf` itself, which differs from
/// the page-name hash.
fn movie_cache_key(path: &PathBuf) -> String {
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    format!("{}.v{}", hasher.finish(), MATCHING_VERSION)
}

/// Looks up movie metadata, preferring the cache and falling back to OMDB. Failures are logged.
pub fn get_movie_info_logged(
    omdb_api_key: &str,
    movie_file_info: MediaInfo,
    cache: Option<&MediaCache>,
) -> Result<MovieInfo, Box<dyn std::error::Error>> {
    let name = movie_file_info.name.clone();
    let path_hash = movie_cache_key(&movie_file_info.path);

    if let Some(media_cache) = cache {
        if let Some(cached_movie_info) = media_cache.get_movie_by_path_hash(&path_hash)? {
            log::info!(target: "cli", "Cache hit for movie (by path_hash {}): {}", path_hash, name);
            return Ok(cached_movie_info);
        }
    }

    log::info!(target: "cli", "Cache miss for movie (by path_hash {}): {}. Fetching from OMDB.", path_hash, name);
    get_movie_info(omdb_api_key, movie_file_info, cache, &path_hash)
        .inspect_err(|err| log::warn!("Failed to get movie info for {}, error: {}", name, err))
}

fn get_movie_info(
    omdb_api_key: &str,
    movie_file_info: MediaInfo,
    cache: Option<&MediaCache>,
    path_hash: &str,
) -> Result<MovieInfo, Box<dyn std::error::Error>> {
    let r = omdb_find(omdb_api_key, OmdbType::Movie, &movie_file_info.name, movie_file_info.year)?;
    let info_url = r.imdb_url();
    let OmdbResponse::Movie {
        title, year, director, poster, language, plot, genre,
        runtime, released, rated, actors, imdb_rating, ratings, ..
    } = r else {
        return Err("Wrong OMDB response".into());
    };

    let movie_info = MovieInfo {
        name: title,
        year: year.parse()?,
        director,
        poster_url: poster_url(&poster),
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
        rotten_tomatoes_rating: rotten_tomatoes_rating(&ratings),
    };

    if let Some(media_cache) = cache {
        match media_cache.store_movie(&movie_info, path_hash) {
            Ok(()) => log::info!(target: "cli", "Stored movie '{}' in cache (path_hash: {})", movie_info.name, path_hash),
            Err(e) => log::error!("Failed to store movie '{}' in cache: {}", movie_info.name, e),
        }
    }
    Ok(movie_info)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_title() {
        let cases = [
            ("Journey.To.The.West.Conquering.The.Demons.2013.720p.WEBRip.x264.AC3-JYK", "Journey To The West Conquering The Demons", Some(2013)),
            ("Man On The Moon (1999) [1080p]", "Man On The Moon", Some(1999)),
            ("Movie.43.2013.1080p.BRrip.x264.GAZ", "Movie 43", Some(2013)),
            ("The Kick [2011].x264.DVDrip(MartialArts)", "The Kick", Some(2011)),
            ("Tropic Thunder 2008 Unrated DC 1080p BluRay HEVC H265 5.1 BONE", "Tropic Thunder", Some(2008)),
            ("Lesbian Vampire Killers 2009 720p BluRay x264 AAC-Mkvking", "Lesbian Vampire Killers", Some(2009)),
            ("[Group] - (Tag).Some.Movie.2013.720p", "Some Movie", Some(2013)),
            ("Liquid Sky - Sc-Fi 1982 Eng Rus Multi Subs 720p [H264-mp4]", "Liquid Sky - Sc-Fi", Some(1982)),
            ("The Brotherhood Of War (Tae Guk Gi) 2004 DC 720p BRRip", "The Brotherhood Of War", Some(2004)),
            ("Popstar Never Stop Never Stopping (2016) (1080p BluRay x265 t3nzin)", "Popstar Never Stop Never Stopping", Some(2016)),
            ("A.Real.Pain.2024.1080p.WEBRip.10Bit.DDP5.1.x265-Asiimov", "A Real Pain", Some(2024)),
            ("Orgazmo.1997.720p.WEB-DL.AAC2.0.H.264", "Orgazmo", Some(1997)),
            ("www.Torrenting.com - Tristram Shandy A Cock and Bull Story 2005 1080p AMZN WEB-DL DDP2 0 H 264-NTb", "Tristram Shandy A Cock and Bull Story", Some(2005)),
            ("Blade Runner 2049 (2017) 1080p", "Blade Runner 2049", Some(2017)),
            ("2001 A Space Odyssey 1968", "2001 A Space Odyssey", Some(1968)),
            ("Tromeo and Juliet", "Tromeo and Juliet", None),
        ];
        for (name, title, year) in cases {
            assert_eq!(parse_title(name), Some((title.to_string(), year)), "{}", name);
        }
    }

    #[test]
    fn test_parse_movie_path() {
        let root = Path::new("/Movies");
        let parse = |p: &str| parse_movie_path(Path::new(p), root).map(|m| (m.name, m.year));
        assert_eq!(parse("/Movies/Tromeo and Juliet/tromeo.and.juliet.dual.audio-rtl.avi"), Some(("Tromeo and Juliet".into(), None)));
        assert_eq!(parse("/Movies/Calvary (2014) [1080p]/Calvary.2014.1080p.BluRay.x264.YIFY.mp4"), Some(("Calvary".into(), Some(2014))));
        assert_eq!(parse("/Movies/Some Movie (2010)/movie.mkv"), Some(("Some Movie".into(), Some(2010))));
        assert_eq!(parse("/Movies/The Haunted World of El Superbeasto (2009).avi"), Some(("The Haunted World of El Superbeasto".into(), Some(2009))));
    }

    #[test]
    fn test_is_sample() {
        assert!(is_sample(Path::new("/Movies/Warrior/Warrior.2011.1080p.BluRay.x264.AC3-ETRG.Sample.mp4")));
        assert!(is_sample(Path::new("/Movies/Warrior/Sample/warrior.mkv")));
        assert!(!is_sample(Path::new("/Movies/Warrior/Warrior.2011.1080p.BluRay.x264.AC3-ETRG.mp4")));
        assert!(!is_sample(Path::new("/Movies/Samples of Life (2001)/Samples.of.Life.mp4")));
    }
}

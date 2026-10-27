use serde::{Deserialize, Serialize};
use std::error;
use std::path::Path;
use std::sync::LazyLock;
use url::Url;

const OMDB_API_URL: &str = "https://www.omdbapi.com";

#[derive(Serialize, Debug, PartialEq)]
pub struct MediaInfo {
    pub name: String,
    pub year: Option<u16>,
    pub path: std::path::PathBuf
}

/// An additional file for the same movie or episode (e.g. another resolution or container).
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct MediaVersion {
    /// File name, shown as the link label.
    pub name: String,
    /// Play link to the file (relative or absolute URI)
    pub media_ref: String,
}

impl MediaVersion {
    pub fn new(path: &Path, media_ref: String) -> Self {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        MediaVersion { name, media_ref }
    }
}

/// Ranks a video file by how well browsers and Chromecast play its container; lower is better.
/// Used to pick the primary file when there are several versions of the same media.
pub fn playability_rank(path: &Path) -> u8 {
    match path.extension().and_then(|e| e.to_str()).map(str::to_lowercase).as_deref() {
        Some("mp4") => 0,
        Some("webm") => 1,
        Some("mov") => 2,
        Some("mkv") => 3,
        _ => 4,
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all(deserialize="snake_case"))]
pub enum OmdbType {
    Movie,
    Series,
    Episode
}

// {"Title":"Tropic Thunder","Year":"2008","Rated":"R","Released":"13 Aug 2008","Runtime":"107 min","Genre":"Action, Comedy, War","Director":"Ben Stiller","Writer":"Justin Theroux, Ben Stiller, Etan Cohen","Actors":"Ben Stiller, Jack Black, Robert Downey Jr.","Plot":"Through a series of freak occurrences, a group of actors shooting a big-budget war movie are forced to become the soldiers they are portraying.","Language":"English, Mandarin","Country":"United States, United Kingdom, Germany","Awards":"Nominated for 1 Oscar. 10 wins & 47 nominations total","Poster":"https://m.media-amazon.com/images/M/MV5BNDE5NjQzMDkzOF5BMl5BanBnXkFtZTcwODI3ODI3MQ@@._V1_SX300.jpg","Ratings":[{"Source":"Internet Movie Database","Value":"7.1/10"},{"Source":"Rotten Tomatoes","Value":"82%"},{"Source":"Metacritic","Value":"71/100"}],"Metascore":"71","imdbRating":"7.1","imdbVotes":"424,101","imdbID":"tt0942385","Type":"movie","DVD":"18 Nov 2008","BoxOffice":"$110,515,313","Production":"N/A","Website":"N/A","Response":"True"}

#[derive(Deserialize, Debug)]
#[serde(rename_all(deserialize="PascalCase"))]
pub struct OmdbRatings {
    pub source: String,
    pub value: String
} 

#[derive(Deserialize, Debug)]
#[serde(rename_all(deserialize="PascalCase"), tag="Type")]
#[allow(dead_code)]
pub enum OmdbResponse {
    #[serde(rename="movie")]
    Movie {
        #[serde(rename="Actors")]
        actors: String,
        #[serde(rename="Awards")]
        awards: String,
        #[serde(rename="Country")]
        country: String,
        #[serde(rename="Director")]
        director: String,
        #[serde(rename="Genre")]
        genre: String,
        #[serde(rename="Language")]
        language: String,
        #[serde(rename="Plot")]
        plot: String,
        #[serde(rename="Poster")]
        poster: String,
        #[serde(rename="Rated")]
        rated: String,
        #[serde(rename="Ratings")]
        ratings: Vec<OmdbRatings>,
        #[serde(rename="Released")]
        released: String,
        #[serde(rename="Runtime")]
        runtime: String,
        #[serde(rename="Title")]
        title: String,
        #[serde(rename="Writer")]
        writer: String,
        #[serde(rename="Year")]
        year: String,
        #[serde(rename="imdbID")]
        imdb_id: String,
        #[serde(rename="imdbRating")]
        imdb_rating: String,
        #[serde(rename="BoxOffice")]
        box_office: Option<String>,
        #[serde(rename="DVD")]
        dvd: Option<String>,
        #[serde(rename="Metascore")]
        metascore: Option<String>,
        #[serde(rename="Production")]
        production: Option<String>,
        #[serde(rename="Website")]
        website: Option<String>,
    },
    #[serde(rename="series")]
    Series {
        #[serde(rename="Actors")]
        actors: String,
        #[serde(rename="Awards")]
        awards: String,
        #[serde(rename="Country")]
        country: String,
        #[serde(rename="Director")]
        director: String,
        #[serde(rename="Genre")]
        genre: String,
        #[serde(rename="Language")]
        language: String,
        #[serde(rename="Plot")]
        plot: String,
        #[serde(rename="Poster")]
        poster: String,
        #[serde(rename="Rated")]
        rated: String,
        #[serde(rename="Ratings")]
        ratings: Vec<OmdbRatings>,
        #[serde(rename="Released")]
        released: String,
        #[serde(rename="Runtime")]
        runtime: String,
        #[serde(rename="Title")]
        title: String,
        #[serde(rename="Writer")]
        writer: String,
        #[serde(rename="Year")]
        year: String,
        #[serde(rename="imdbID")]
        imdb_id: String,
        #[serde(rename="imdbRating")]
        imdb_rating: String,
        #[serde(rename="totalSeasons")]
        total_seasons: String,
        #[serde(rename="Metascore")]
        metascore: Option<String>,
    },
    #[serde(rename="episode")]
    Episode {
        #[serde(rename="Title")]
        title: String,
        #[serde(rename="Year")]
        year: String,
        #[serde(rename="Rated")]
        rated: String,
        #[serde(rename="Released")]
        released: String,
        #[serde(rename="Season")]
        season: String,
        #[serde(rename="Episode")]
        episode: String,
        #[serde(rename="Runtime")]
        runtime: String,
        #[serde(rename="Genre")]
        genre: String,
        #[serde(rename="Director")]
        director: String,
        #[serde(rename="Writer")]
        writer: String,
        #[serde(rename="Actors")]
        actors: String,
        #[serde(rename="Plot")]
        plot: String,
        #[serde(rename="Language")]
        language: String,
        #[serde(rename="Country")]
        country: String,
        #[serde(rename="Awards")]
        awards: String,
        #[serde(rename="Poster")]
        poster: String,
        #[serde(rename="Ratings")]
        ratings: Vec<OmdbRatings>,
        #[serde(rename="Metascore")]
        metascore: String,
        #[serde(rename="imdbRating")]
        imdb_rating: String,
        #[serde(rename="imdbVotes")]
        imdb_votes: String,
        #[serde(rename="imdbID")]
        imdb_id: String,
        #[serde(rename="seriesID")]
        series_id: String,
    }
}


impl OmdbResponse {
    /// The IMDb id, e.g. "tt0942385".
    pub fn imdb_id(&self) -> &str {
        match self {
            OmdbResponse::Movie { imdb_id, .. } => imdb_id,
            OmdbResponse::Series { imdb_id, .. } => imdb_id,
            OmdbResponse::Episode { imdb_id, .. } => imdb_id,
        }
    }

    pub fn imdb_url(&self) -> Url {
        Url::parse("https://www.imdb.com/title/").unwrap().join(self.imdb_id()).unwrap()
    }
}

/// Extracts the Rotten Tomatoes score from an OMDB ratings list, if present.
pub fn rotten_tomatoes_rating(ratings: &[OmdbRatings]) -> Option<String> {
    ratings.iter()
        .find(|r| r.source == "Rotten Tomatoes")
        .map(|r| r.value.clone())
}

impl std::fmt::Display for OmdbType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OmdbType::Episode => write!(f, "episode"),
            OmdbType::Series => write!(f, "series"),
            OmdbType::Movie => write!(f, "movie")
        }
    }
}

static HTTP_CLIENT: LazyLock<reqwest::blocking::Client> = LazyLock::new(reqwest::blocking::Client::new);

/// Shown for media OMDB has no poster for.
pub const PLACEHOLDER_POSTER_URL: &str = "https://via.placeholder.com/300x450.png?text=No+Poster";

/// Parses an OMDB poster field, which is "N/A" when there is none.
pub fn poster_url(poster: &str) -> Url {
    Url::parse(poster).unwrap_or_else(|_| Url::parse(PLACEHOLDER_POSTER_URL).unwrap())
}

fn omdb_get<T: serde::de::DeserializeOwned>(omdb_api_key: &str, params: &[(&str, String)]) -> Result<T, Box<dyn error::Error>> {
    let mut query = vec![("apiKey", omdb_api_key.to_string())];
    query.extend_from_slice(params);
    Ok(HTTP_CLIENT.get(OMDB_API_URL).query(&query).send()?.json::<T>()?)
}

fn omdb_request(omdb_api_key: &str, entity_type: OmdbType, title: &str, extra: &[(&str, String)]) -> Result<OmdbResponse, Box<dyn error::Error>> {
    let mut params = vec![("t", title.to_string()), ("type", entity_type.to_string())];
    params.extend_from_slice(extra);
    omdb_get(omdb_api_key, &params)
}

#[derive(Deserialize, Debug)]
struct OmdbSearchResponse {
    #[serde(rename = "Search", default)]
    search: Vec<OmdbSearchHit>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "PascalCase")]
struct OmdbSearchHit {
    title: String,
    /// "2008", or "2020–2025" for series
    year: String,
    #[serde(rename = "imdbID")]
    imdb_id: String,
}

/// Lowercase words of a title, without punctuation, for comparing titles.
fn title_words(title: &str) -> Vec<String> {
    title.replace('&', " and ")
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Names to search OMDB for, best first: the name itself, the part before " - "
/// ("Liquid Sky - Sc-Fi"), then ever shorter prefixes of at least two words
/// ("Mythic Quest Ravens Banquet" -> "Mythic Quest"). Only the name itself may match a
/// longer title ("Brotherhood of War" -> "Tae Guk Gi: The Brotherhood of War").
fn title_candidates(name: &str) -> Vec<(String, bool)> {
    let mut candidates = vec![(name.to_string(), true)];
    let base = name.split(" - ").next().unwrap_or(name).trim();
    let words: Vec<&str> = base.split_whitespace().collect();
    for n in (2..=words.len()).rev() {
        let candidate = words[..n].join(" ");
        if !candidates.iter().any(|(c, _)| *c == candidate) {
            candidates.push((candidate, false));
        }
    }
    candidates
}

/// Picks the search hit for `query`: an exact title match, or (if `allow_longer`) a title
/// containing the query's words. OMDB years often differ by one from those in file names
/// (festival vs. release year), so a year within one counts as a match.
fn pick_hit<'a>(hits: &'a [OmdbSearchHit], query: &str, year: Option<u16>, allow_longer: bool) -> Option<&'a OmdbSearchHit> {
    let query_words = title_words(query);
    let year_diff = |hit: &OmdbSearchHit| -> Option<u16> {
        let Some(year) = year else { return Some(0) };
        let hit_year: u16 = hit.year.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok()?;
        Some(hit_year.abs_diff(year)).filter(|d| *d <= 1)
    };
    let exact = |hit: &&OmdbSearchHit| title_words(&hit.title) == query_words;
    let longer = |hit: &&OmdbSearchHit| title_words(&hit.title).windows(query_words.len()).any(|w| w == query_words.as_slice());
    let best = |matches: &dyn Fn(&&OmdbSearchHit) -> bool| {
        hits.iter().filter(matches).filter_map(|h| year_diff(h).map(|d| (d, h))).min_by_key(|(d, _)| *d).map(|(_, h)| h)
    };
    best(&exact).or_else(|| if allow_longer && !query_words.is_empty() { best(&longer) } else { None })
}

/// Finds a movie or series on OMDB by the (possibly noisy) name parsed from a file name.
/// Uses OMDB search, which finds the right entry far more often than a title lookup, then
/// falls back to the title lookup.
pub fn omdb_find(omdb_api_key: &str, entity_type: OmdbType, name: &str, year: Option<u16>) -> Result<OmdbResponse, Box<dyn error::Error>> {
    for (candidate, allow_longer) in title_candidates(name) {
        let hits = omdb_get::<OmdbSearchResponse>(omdb_api_key, &[("s", candidate.clone()), ("type", entity_type.to_string())])?.search;
        if let Some(hit) = pick_hit(&hits, &candidate, year, allow_longer) {
            if candidate != name {
                log::info!(target: "cli", "Matched '{}' on OMDB as '{}' ({})", name, hit.title, hit.year);
            }
            return omdb_get(omdb_api_key, &[("i", hit.imdb_id.clone())]);
        }
    }
    omdb_get_metadata(omdb_api_key, entity_type, name, year)
}

pub fn omdb_get_metadata(omdb_api_key: &str, entity_type: OmdbType, title: &str, year: Option<u16>) -> Result<OmdbResponse, Box<dyn error::Error>> {
    let extra: Vec<_> = year.map(|y| ("y", y.to_string())).into_iter().collect();
    omdb_request(omdb_api_key, entity_type, title, &extra)
}

/// How to identify a series in an episode lookup.
pub enum SeriesRef<'a> {
    Title(&'a str),
    /// IMDb id such as "tt8879940"; reliable even when the folder name isn't the series title.
    ImdbId(&'a str),
}

pub fn omdb_get_episode_metadata(
    omdb_api_key: &str,
    series: SeriesRef,
    season: u8,
    episode: u8,
) -> Result<OmdbResponse, Box<dyn error::Error>> {
    let (series_param, series_title) = match series {
        SeriesRef::Title(title) => (("t", title.to_string()), title),
        SeriesRef::ImdbId(id) => (("i", id.to_string()), id),
    };
    let resp: OmdbResponse = omdb_get(omdb_api_key, &[
        series_param,
        ("type", OmdbType::Episode.to_string()),
        ("Season", season.to_string()),
        ("Episode", episode.to_string()),
    ])?;

    if let OmdbResponse::Episode { .. } = resp {
        Ok(resp)
    } else {
        log::warn!(target: "cli", "OMDB did not return episode data for {} S{}E{}. Response: {:?}", series_title, season, episode, resp);
        Err(Box::from(format!("OMDB did not return valid episode data for {} S{}E{}. Check series title, season, and episode numbers.", series_title, season, episode)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hit(title: &str, year: &str) -> OmdbSearchHit {
        OmdbSearchHit { title: title.into(), year: year.into(), imdb_id: format!("tt-{}", title) }
    }

    #[test]
    fn test_title_candidates() {
        let names = |name| title_candidates(name).into_iter().map(|(c, _)| c).collect::<Vec<_>>();
        assert_eq!(names("Liquid Sky - Sc-Fi"), vec!["Liquid Sky - Sc-Fi", "Liquid Sky"]);
        assert_eq!(names("Mythic Quest Ravens Banquet"), vec!["Mythic Quest Ravens Banquet", "Mythic Quest Ravens", "Mythic Quest"]);
        assert_eq!(names("Silo"), vec!["Silo"]);
    }

    #[test]
    fn test_pick_hit() {
        let hits = [hit("Waking Life", "2002"), hit("Waking Life", "2019"), hit("Waking Life (2001)", "2001")];
        assert_eq!(pick_hit(&hits, "Waking Life", Some(2001), true).unwrap().year, "2002");

        let hits = [hit("Tropic Thunder: Rain of Madness", "2008"), hit("Tropic Thunder", "2008")];
        assert_eq!(pick_hit(&hits, "Tropic Thunder", Some(2008), true).unwrap().title, "Tropic Thunder");

        let hits = [hit("Tae Guk Gi: The Brotherhood of War", "2004")];
        assert!(pick_hit(&hits, "The Brotherhood Of War", Some(2004), true).is_some());
        assert!(pick_hit(&hits, "The Brotherhood Of War", Some(2004), false).is_none());
        assert!(pick_hit(&hits, "The Brotherhood Of War", Some(1990), true).is_none());

        let hits = [hit("Mythic Quest", "2020–2025")];
        assert!(pick_hit(&hits, "Mythic Quest", None, false).is_some());
    }
}

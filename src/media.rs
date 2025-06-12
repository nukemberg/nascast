use serde::Deserialize;
use serde_derive::Serialize;
use reqwest;
use url::Url;
use std::error;

const OMDB_API_URL: &str = "https://www.omdbapi.com";

#[derive(Serialize, Debug, PartialEq)]

pub struct MediaInfo {
    pub name: String,
    pub year: Option<u16>,
    pub path: std::path::PathBuf
}

pub trait MediaInfoEquiv {
    fn path(&self) -> &std::path::Path;
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
    pub fn imdb_url(&self) -> Url {
        let imdb_id = match self {
            OmdbResponse::Movie { imdb_id, .. } => imdb_id,
            OmdbResponse::Series { imdb_id, .. } => imdb_id,
            OmdbResponse::Episode { imdb_id, .. } => imdb_id,
        };
        Url::parse("https://www.imdb.com/title/").unwrap().join(imdb_id).unwrap()
    }
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

pub fn omdb_get_metadata(omdb_api_key: &str, entity_type: OmdbType, title: &str, year: Option<u16>) -> Result<OmdbResponse, Box<dyn error::Error>> {
    let mut params = vec![
        ("apiKey", omdb_api_key.to_string()),
        ("t", title.to_string()),
        ("type", entity_type.to_string()),
    ];
    if let Some(y) = year {
        params.push(("y", y.to_string()));
    }

    let client = reqwest::blocking::Client::new();
    let resp = client.get(OMDB_API_URL)
        .query(&params)
        .send()?
        .json::<OmdbResponse>()?;
    Ok(resp)
}

pub fn omdb_get_episode_metadata(
    omdb_api_key: &str,
    series_title: &str,
    season: u8,
    episode: u8,
) -> Result<OmdbResponse, Box<dyn error::Error>> {
    let params = vec![
        ("apiKey", omdb_api_key.to_string()),
        ("t", series_title.to_string()),
        ("Season", season.to_string()),
        ("Episode", episode.to_string()),
        ("type", OmdbType::Episode.to_string()),
    ];

    let client = reqwest::blocking::Client::new();
    let resp = client.get(OMDB_API_URL)
        .query(&params)
        .send()?
        .json::<OmdbResponse>()?;

    if let OmdbResponse::Episode { .. } = resp {
        Ok(resp)
    } else {
        log::warn!(target: "cli", "OMDB did not return episode data for {} S{}E{}. Response: {:?}", series_title, season, episode, resp);
        Err(Box::from(format!("OMDB did not return valid episode data for {} S{}E{}. Check series title, season, and episode numbers.", series_title, season, episode)))
    }
}
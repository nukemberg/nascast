
use serde::Deserialize;
use serde_derive::Serialize;
use reqwest;
use url::Url;
use std::error;

const OMDB_API_URL: &str = "http://www.omdbapi.com";

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
pub enum OmdbType {
    Movie,
    Series,
    Episode
}

// {"Title":"Tropic Thunder","Year":"2008","Rated":"R","Released":"13 Aug 2008","Runtime":"107 min","Genre":"Action, Comedy, War","Director":"Ben Stiller","Writer":"Justin Theroux, Ben Stiller, Etan Cohen","Actors":"Ben Stiller, Jack Black, Robert Downey Jr.","Plot":"Through a series of freak occurrences, a group of actors shooting a big-budget war movie are forced to become the soldiers they are portraying.","Language":"English, Mandarin","Country":"United States, United Kingdom, Germany","Awards":"Nominated for 1 Oscar. 10 wins & 47 nominations total","Poster":"https://m.media-amazon.com/images/M/MV5BNDE5NjQzMDkzOF5BMl5BanBnXkFtZTcwODI3ODI3MQ@@._V1_SX300.jpg","Ratings":[{"Source":"Internet Movie Database","Value":"7.1/10"},{"Source":"Rotten Tomatoes","Value":"82%"},{"Source":"Metacritic","Value":"71/100"}],"Metascore":"71","imdbRating":"7.1","imdbVotes":"424,101","imdbID":"tt0942385","Type":"movie","DVD":"18 Nov 2008","BoxOffice":"$110,515,313","Production":"N/A","Website":"N/A","Response":"True"}

#[derive(Deserialize)]
pub struct OmdbResponse {
    pub title: String,
    pub year: u16,
    pub runtime: String,
    pub genre: String,
    pub director: String,
    pub writer: String,
    pub actors: Vec<String>,
    pub released: String,
    pub plot: String,
    pub language: String,
    #[serde(rename(deserialize = "totalSeasons"))]
    pub total_seasons: u8,
    #[serde(rename(deserialize = "type"))]
    pub omdb_type: OmdbType,
    pub poster: String,
    #[serde(rename(deserialize = "imdbID"))]
    imdb_id: String
}

impl OmdbResponse {
    pub fn imdb_url(&self) -> Url {
        Url::parse("https://www.imdb.com/title/").unwrap().join(&self.imdb_id).unwrap()
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

pub async fn omdb_get_metadata(omdb_api_key: &str, entity_type: OmdbType, title: &str, year: Option<u16>) -> Result<OmdbResponse, Box<dyn error::Error>> {
    let url = Url::parse_with_params(OMDB_API_URL, &[("apiKey", omdb_api_key), ("t", title), ("y", year.map(|y| y.to_string()).unwrap_or_default().as_str()), ("type", entity_type.to_string().as_str())])?;
    let resp = reqwest::get(url).await?.json::<OmdbResponse>().await?;
    Ok(resp)
}
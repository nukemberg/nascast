use std::path::PathBuf;
use regex::Regex;
use url::Url;
use serde_derive::Serialize;
use crate::media::{omdb_get_metadata, MediaInfo, MediaInfoEquiv, OmdbResponse, OmdbType};

#[derive(Serialize, Debug, PartialEq)]
pub struct MovieInfo {
    name: String,
    year: u16,
    director: String,
    path: PathBuf,
    info_url: Url,
    poster_url: Url,
    language: String,
    plot: String
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

pub fn get_movie_info_logged(omdb_api_key: &str, movie_file_info: MediaInfo) -> Result<MovieInfo, Box<dyn std::error::Error>> {
    let name = movie_file_info.name.clone();
    let movie_info = get_movie_info(omdb_api_key, movie_file_info);
    match movie_info {
        Ok(info) => Ok(info),
        Err(err) => {
            log::warn!("Failed to get movie info for {}, error: {}", name, err);
            Err(err)
        }
    }
}

pub fn get_movie_info(omdb_api_key: &str, movie_file_info: MediaInfo) -> Result<MovieInfo, Box<dyn std::error::Error>> {
    let r = omdb_get_metadata(omdb_api_key, OmdbType::Movie, &movie_file_info.name, movie_file_info.year)?;
    match r {
        OmdbResponse::Movie { title, year, director, poster, language, plot, .. } => {
            let info_url = Url::parse("https://www.imdb.com/title/").unwrap().join(&title).unwrap();
            Ok(MovieInfo{
                name: title,
                year: year.parse()?,
                director,
                poster_url: Url::parse(&poster)?,
                language,
                plot,
                info_url,
                path: movie_file_info.path
            })
        },
        _ => Err("Wrong OMDB response".into())
    }

}

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
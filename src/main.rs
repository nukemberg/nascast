use std::{path::{Path, PathBuf}, hash::{Hash, Hasher}};
use clap;
use serde_derive::{Serialize};
use tera;
use regex::Regex;
use walkdir;
use url::Url;
#[macro_use]
extern crate lazy_static;

#[derive(Serialize, Debug, PartialEq)]
struct MediaInfo {
    name: String,
    year: Option<u32>,
    path: std::path::PathBuf
}

const DEFAULT_CSS_FILE: &str = include_str!("media.css");
const DEFAULT_MEDIA_HTML_TEMPLATE: &str = include_str!("media.html");
const DEFAULT_INDEX_HTML_TEMPLATE: &str = include_str!("media-index.html");
const DEFAULT_JS_FILE: &str = include_str!("./../static/media.js");

fn file_info(regexs: &Vec<Regex>, path: PathBuf) -> Option<MediaInfo> {    
    let filename = path.file_name()?.to_str()?;
    let filename_match = regexs.iter().find_map(|re| re.captures(filename))?;
    let name = {
        let n = filename_match.name("name")?.as_str();
        if !n.contains(" ") && n.contains(".") {
            n.replace(".", " ")
        } else {
            n.to_owned()
        }
    };
    let year = filename_match.name("year").and_then(|s| s.as_str().parse::<u32>().ok());

    Some(MediaInfo{name: name, year: year, path: path})
}

fn scan_folders(basepath: &str) -> Vec<std::path::PathBuf> {
    walkdir::WalkDir::new(basepath)
    .max_depth(2).into_iter()
    .filter_map(|entry| entry.ok().map(|e| e.path().to_path_buf()))
    .filter(|path| path.is_file())
    .filter(|path| path.extension().filter(|ext| *ext == "mp4").is_some())
    .collect()
}

fn split_2_or(s: &str, default_second: Option<&str>) -> (String, String) {
    let mut split = s.split(":");
    let first = split.next().unwrap();
    let second = split.next().or(default_second).unwrap_or(s);
    (first.into(), second.into())
}

fn gen_media_ref(base_url: &Option<Url>, folder_path: &Path, folder_mount: &String, media: &MediaInfo) -> String {
    let relative_path = media.path.strip_prefix(folder_path).unwrap();
    let path_components = relative_path.components().into_iter().map(|c| c.as_os_str().to_str().unwrap());
    
    match base_url {
        Some(url) => {
            let p = path_components.fold(folder_mount.to_owned(), |prev, s| [prev, "/".to_string(), s.to_string()].concat());
            url.join(&p).unwrap().to_string()
        },
        None => path_components.fold(folder_mount.to_string(), |s, comp| [s, "/".to_string(), urlencoding::encode(comp).to_string()].concat())
    }
}

fn process_folder(template: &tera::Tera, base_url: &Option<Url>, output_path: &Path, regexs: &Vec<Regex>, folder_spec: &str) {
    let (folder, mount) = split_2_or(&folder_spec, None);
    scan_folders(&folder).iter().filter_map(|f| file_info(regexs, f.to_owned())).for_each(|media_info| {
        let media_ref = gen_media_ref(&base_url, Path::new(&folder), &mount, &media_info);
        let mut ctx = tera::Context::new();
        ctx.insert("media_ref", &media_ref);
        ctx.insert("media_info", &media_info);

        let t = template.render("movie.html", &ctx).unwrap();
        println!("File: {:?}", media_info);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        media_info.path.to_str().hash(& mut hasher);
        std::fs::write(output_path.join(std::path::Path::new(&(hasher.finish().to_string() + ".html"))), t).unwrap();
    })
}

lazy_static! {
    static ref MOVIE_PATTERNS_RE: Vec<Regex> = [
        "(?P<name>[a-zA-Z0-9.]+)\\.(?P<year>(?:19|20)\\d{2})\\..*",
        "(?P<name>[a-zA-Z0-9 ]+) \\((?P<year>(?:19|20)\\d{2})\\).*",
        "(?P<name>[a-zA-Z0-9 ]+) \\[(?P<year>(?:19|20)\\d{2})\\].*",
        "(?P<name>[a-zA-Z0-9 ]+) (?P<year>(?:19|20)\\d{2}).*"
    ].iter().map(|pattern| Regex::new(pattern).unwrap()).collect();
    static ref TV_PATTERNS_RE: Vec<Regex> = [
        "(?P<name>.*)\\.mp4"
    ].iter().map(|pattern| Regex::new(pattern).unwrap()).collect();

}

fn main() {
    let app = clap::Command::new("nascast")
    .arg(clap::Arg::new("movies-folder").long("movies-folder").action(clap::ArgAction::Append))
    .arg(clap::Arg::new("tv-folder").long("tv-folder").action(clap::ArgAction::Append))
    .arg(clap::Arg::new("output-folder").long("output-folder").default_value("./pub"))
    .arg(clap::Arg::new("base-url").long("base-url"))
    .get_matches();

    let mut template = tera::Tera::default();
    template.add_raw_template("movie.html", DEFAULT_MEDIA_HTML_TEMPLATE).unwrap();
    let output_dir = app.get_one::<String>("output-folder").unwrap();
    let base_url = app.get_one::<String>("base-url").and_then(|s| url::Url::parse(s).ok());
    let output_path = Path::new(&output_dir);
    std::fs::create_dir_all(output_path).unwrap();
    
    app.get_many::<String>("movies-folder").unwrap_or_default().for_each(|folder_spec| process_folder(&template, &base_url, output_path, &MOVIE_PATTERNS_RE, folder_spec));
    app.get_many::<String>("tv-folder").unwrap_or_default().for_each(|folder_spec| process_folder(&template, &base_url, output_path, &TV_PATTERNS_RE, folder_spec));
    println!("Writing media.js");
    std::fs::write(output_path.join(Path::new("media.js")), DEFAULT_JS_FILE).unwrap();
}


#[cfg(test)]
mod tests {
    use std::path::Path;
    
    use url::Url;
    
    use crate::{MediaInfo, gen_media_ref, file_info, MOVIE_PATTERNS_RE};
    
    // nascast --movies-folder /media/storage/Movies:movies --base-url https://pi.nukembase
    // href - https://pi.nukembase/movies/somemovie.mp4
    #[test]
    fn test_media_ref() {
        let media = MediaInfo{name: "some movie".to_string(), year: Some(1993), path: Path::new("./Movies/Some movie 1993/some movie 1993.mp4").to_path_buf()};
        assert_eq!(gen_media_ref(&Url::parse("https://someserver:8080/media/").ok(), Path::new("./Movies"), &"movies".to_string(), &media), "https://someserver:8080/media/movies/Some%20movie%201993/some%20movie%201993.mp4");
        // technically speaking, the base url should end with / or the last component isn't "the base". Might be confusing but there we are 
        assert_eq!(gen_media_ref(&Url::parse("https://someserver:8080/media").ok(), Path::new("./Movies"), &"movies".to_string(), &media), "https://someserver:8080/movies/Some%20movie%201993/some%20movie%201993.mp4");
        assert_eq!(gen_media_ref(&None, Path::new("./Movies"), &"movies".to_string(), &media), "movies/Some%20movie%201993/some%20movie%201993.mp4");
    }

    #[test]
    fn test_movie_name_parsing() {
        fn assert_movie_file_info(path: &str, name: &str, year: Option<u32>) {
            let _path = Path::new(path).to_path_buf();
            assert_eq!(file_info(&MOVIE_PATTERNS_RE ,_path.clone()), Some(MediaInfo{path: _path, name: name.into(), year: year}));
        }

        assert_movie_file_info("movies/Journey.To.The.West.Conquering.The.Demons.2013.720p.WEBRip.x264.AC3-JYK.mp4", "Journey To The West Conquering The Demons", Some(2013));
        assert_movie_file_info("movies/Man On The Moon (1999) [1080p].mp4", "Man On The Moon", Some(1999));
        assert_movie_file_info("Movies/Movie 43 (2013) [1080p]/Movie.43.2013.1080p.BRrip.x264.GAZ.mp4", "Movie 43", Some(2013));
        assert_movie_file_info("Movies/The Kick [2011].x264.DVDrip(MartialArts).mp4", "The Kick", Some(2011));
        assert_movie_file_info("Movies/Tropic Thunder 2008 Unrated DC 1080p BluRay HEVC H265 5.1 BONE.mp4", "Tropic Thunder", Some(2008));
        assert_movie_file_info("Lesbian Vampire Killers 2009 720p BluRay x264 AAC-Mkvking.mkv", "Lesbian Vampire Killers", Some(2009));
    }
}
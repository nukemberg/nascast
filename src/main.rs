use std::{path::Path, hash::{Hash, Hasher}};
use clap;
use futures::{stream::TryStreamExt, StreamExt};
use serde::Serialize;
use tera;
use walkdir;
use url::Url;
#[macro_use]
extern crate lazy_static;
extern crate tokio;

mod movie;
mod media;

use media::MediaInfoEquiv;

use crate::movie::MovieInfo;

const DEFAULT_CSS_FILE: &str = include_str!("media.css");
const DEFAULT_MEDIA_HTML_TEMPLATE: &str = include_str!("media.html");
const DEFAULT_INDEX_HTML_TEMPLATE: &str = include_str!("media-index.html");
const DEFAULT_JS_FILE: &str = include_str!("./../static/media.js");

fn scan_folders(basepath: &Path) -> Vec<std::path::PathBuf> {
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

fn gen_media_ref(base_url: &Option<Url>, folder_path: &Path, folder_mount: &str, media_path: &Path) -> String {
    let relative_path = media_path.strip_prefix(folder_path).unwrap();
    let path_components = relative_path.components().into_iter().map(|c| c.as_os_str().to_str().unwrap());
    
    match base_url {
        Some(url) => {
            let p = path_components.fold(folder_mount.to_owned(), |prev, s| [prev, "/".to_string(), s.to_string()].concat());
            url.join(&p).unwrap().to_string()
        },
        None => path_components.fold(folder_mount.to_string(), |s, comp| [s, "/".to_string(), urlencoding::encode(comp).to_string()].concat())
    }
}


fn render_factory<'a, T>(template: &'a tera::Tera, output_path: &'a Path, base_url: &'a Option<Url>, folder: &'a Path, mount: &'a str) -> Box<dyn Fn(T) -> () + 'a>
    where T: MediaInfoEquiv + Serialize + std::fmt::Debug {
    Box::new(move |media_info: T| {
        let media_path = media_info.path();
        let media_ref = gen_media_ref(&base_url, folder, &mount, media_path);
        let mut ctx = tera::Context::new();
        ctx.insert("media_ref", &media_ref);
        ctx.insert("media_info", &media_info);

        let t = template.render("movie.html", &ctx).unwrap();
        println!("File: {:?}", media_info);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        media_path.to_str().hash(& mut hasher);
        std::fs::write(output_path.join(std::path::Path::new(&(hasher.finish().to_string() + ".html"))), t).unwrap();
    })
}

fn logger<T>(movie_info: T)
    where T: MediaInfoEquiv + Serialize + std::fmt::Debug
 {
    println!("Media: {:?}", movie_info);
 }

#[tokio::main]
async fn main() {
    let app = clap::Command::new("nascast")
    .arg(clap::Arg::new("movies-folder").long("movies-folder").action(clap::ArgAction::Append))
    .arg(clap::Arg::new("tv-folder").long("tv-folder").action(clap::ArgAction::Append))
    .arg(clap::Arg::new("output-folder").long("output-folder").default_value("./pub"))
    .arg(clap::Arg::new("base-url").long("base-url"))
    .arg(clap::Arg::new("omdb-api-key").long("omdb-api-key"))
    .arg(clap::Arg::new("noop").long("noop").help("NoOp mode: only show metadata, does not write anything to disk").action(clap::ArgAction::SetTrue))
    .get_matches();

    let mut template = tera::Tera::default();
    template.add_raw_template("movie.html", DEFAULT_MEDIA_HTML_TEMPLATE).unwrap();
    let output_dir = app.get_one::<String>("output-folder").expect("Output filter required");
    let base_url = app.get_one::<String>("base-url").and_then(|s| url::Url::parse(s).ok());
    let output_path = Path::new(&output_dir);
    let omdb_api_key = app.get_one::<String>("omdb-api-key").expect("OMDB API Key required");
    let noop = app.get_flag("noop");
    std::fs::create_dir_all(output_path).unwrap();
    
    for folder_spec in app.get_many::<String>("movies-folder").unwrap_or_default() {
        let (s_folder, mount) = split_2_or(&folder_spec, None);
        let folder = Path::new(&s_folder);
        let render = if noop {
            render_factory(&template, output_path, &base_url, folder, &mount)
        } else {
            Box::new(logger)
        };
        let info_futures = scan_folders(folder).iter()
            .filter_map(|file| movie::parse_movie_filename(&movie::MOVIE_PATTERNS_RE, file))
            .map(|movie_file_info| movie::get_movie_info(omdb_api_key, movie_file_info)).collect::<Vec<_>>();
        let movies_infos = futures::stream::iter(info_futures).buffer_unordered(20).try_collect::<Vec<MovieInfo>>().await.unwrap();
        for movie_info in movies_infos {
            render(movie_info);
        }
    }
    // app.get_many::<String>("tv-folder").unwrap_or_default().for_each(|folder_spec| process_folder(&template, &base_url, output_path, &TV_PATTERNS_RE, folder_spec));
    println!("Writing media.js");
    std::fs::write(output_path.join(Path::new("media.js")), DEFAULT_JS_FILE).unwrap();
}


#[cfg(test)]
mod tests {
    use std::path::Path;
    
    use url::Url;
    
    use crate::gen_media_ref;

    // nascast --movies-folder /media/storage/Movies:movies --base-url https://pi.nukembase
    // href - https://pi.nukembase/movies/somemovie.mp4
    #[test]
    fn test_media_ref() {
        let path = Path::new("./Movies/Some movie 1993/some movie 1993.mp4");
        assert_eq!(gen_media_ref(&Url::parse("https://someserver:8080/media/").ok(), Path::new("./Movies"), &"movies".to_string(), path), "https://someserver:8080/media/movies/Some%20movie%201993/some%20movie%201993.mp4");
        // technically speaking, the base url should end with / or the last component isn't "the base". Might be confusing but there we are 
        assert_eq!(gen_media_ref(&Url::parse("https://someserver:8080/media").ok(), Path::new("./Movies"), &"movies".to_string(), path), "https://someserver:8080/movies/Some%20movie%201993/some%20movie%201993.mp4");
        assert_eq!(gen_media_ref(&None, Path::new("./Movies"), &"movies".to_string(), path), "movies/Some%20movie%201993/some%20movie%201993.mp4");
    }
}
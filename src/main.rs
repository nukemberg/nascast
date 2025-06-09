use std::{path::Path, hash::{Hash, Hasher}};
use clap;
use movie::MovieInfo;
use serde::Serialize;
use tera;
use walkdir;
use url::Url;
use log;
#[macro_use]
extern crate lazy_static;

mod movie;
mod media;
mod tv; // Add tv module

use media::MediaInfoEquiv;
use tv::TvSeriesMediaInfo; // Removed TvEpisodeMediaInfo import

const DEFAULT_CSS_FILE: &str = include_str!("media.css");
const DEFAULT_MEDIA_HTML_TEMPLATE: &str = include_str!("media.html");
const DEFAULT_INDEX_HTML_TEMPLATE: &str = include_str!("index.html");
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

#[derive(Serialize)]
struct MovieIndexInfo {
    name: String,
    year: u16,
    director: String,
    poster_url: String,
    page_url: String,
}

#[derive(Serialize)]
struct TvSeriesIndexInfo {
    name: String,
    year: Option<u16>,
    episodes_count: usize,
    page_url: String,
    poster_url: String,
}

fn main() {
    let log_config = log4rs::config::Config::builder().appender(
        log4rs::config::Appender::builder().build("stdout", 
        Box::new(log4rs::append::console::ConsoleAppender::builder().build())))
        .logger(log4rs::config::Logger::builder().build("cli", log::LevelFilter::Info))
        .build(log4rs::config::Root::builder().appender("stdout").build(log::LevelFilter::Warn)).unwrap();
    let _log_config_handle = log4rs::init_config(log_config).unwrap();
    
    let app = clap::Command::new("nascast")
    .arg(clap::Arg::new("movies-folder").long("movies-folder").action(clap::ArgAction::Append))
    .arg(clap::Arg::new("tv-folder").long("tv-folder").action(clap::ArgAction::Append))
    .arg(clap::Arg::new("output-folder").long("output-folder").default_value("./pub"))
    .arg(clap::Arg::new("base-url").long("base-url"))
    .arg(clap::Arg::new("omdb-api-key").long("omdb-api-key"))
    .arg(clap::Arg::new("noop").long("noop").help("NoOp mode: only show metadata, does not write anything to disk").action(clap::ArgAction::SetTrue))
    .arg(clap::Arg::new("verbosity").long("verbosity").short('v').action(clap::ArgAction::Set))
    .get_matches();

    let mut template = tera::Tera::default();
    template.add_raw_template("movie.html", DEFAULT_MEDIA_HTML_TEMPLATE).unwrap();
    template.add_raw_template("index.html", DEFAULT_INDEX_HTML_TEMPLATE).unwrap();
    template.add_raw_template("movies.html", include_str!("movies.html")).unwrap();
    template.add_raw_template("tv.html", include_str!("tv.html")).unwrap();
    let output_dir = app.get_one::<String>("output-folder").expect("Output filter required");
    let base_url = app.get_one::<String>("base-url").and_then(|s| url::Url::parse(s).ok());
    let output_path = Path::new(&output_dir);
    let omdb_api_key = app.get_one::<String>("omdb-api-key").expect("OMDB API Key required");
    let noop = app.get_flag("noop");
    std::fs::create_dir_all(output_path).unwrap();
    
    let mut all_movies = Vec::new();
    
    for folder_spec in app.get_many::<String>("movies-folder").unwrap_or_default() {
        let (s_folder, mount) = split_2_or(&folder_spec, None);
        let folder = Path::new(&s_folder);
        let render = if noop {
            Box::new(logger)
        } else {
            render_factory(&template, output_path, &base_url, folder, &mount)
        };

        let media_infos = scan_folders(folder).iter()
            .filter_map(|file| movie::parse_movie_filename(&movie::MOVIE_PATTERNS_RE, file))
            .filter_map(|info| movie::get_movie_info_logged(omdb_api_key, info).ok() )
            .collect::<Vec<MovieInfo>>();

        for movie_info in media_infos {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            movie_info.path().to_str().hash(&mut hasher);
            let page_name = hasher.finish().to_string() + ".html";

            all_movies.push(MovieIndexInfo {
                name: movie_info.name.clone(),
                year: movie_info.year,
                director: movie_info.director.clone(),
                poster_url: movie_info.poster_url.to_string(),
                page_url: page_name,
            });

            if !noop {
                render(movie_info);
            }
        }
    }

    // Changed from HashMap to Vec<TvSeriesMediaInfo>
    let mut all_tv_series: Vec<TvSeriesMediaInfo> = Vec::new(); 

    for folder_spec in app.get_many::<String>("tv-folder").unwrap_or_default() {
        let (s_folder, _mount) = split_2_or(&folder_spec, None); // Mount point not used yet for TV
        let folder = Path::new(&s_folder);
        log::info!(target: "cli", "Scanning TV folder: {:?}", folder);

        match tv::scan_tv_directory(folder) { // scan_tv_directory now returns Vec<TvSeriesMediaInfo>
            Ok(series_list) => { // series_list is Vec<TvSeriesMediaInfo>
                log::info!(target: "cli", "Found {} series in TV folder: {:?}", series_list.len(), folder);
                for series_data in series_list { // series_data is TvSeriesMediaInfo
                    log::info!(target: "cli", "  Found Series: '{}', Year: {:?}, Path: {:?}, Episodes: {}", 
                               series_data.name, series_data.year, series_data.path, series_data.episodes.len());
                    for episode in &series_data.episodes { // Iterate over episodes within the series
                        log::debug!(target: "cli", "    Episode: S{:02}E{:02} - Path: {:?}", episode.season, episode.episode, episode.path);
                    }
                    all_tv_series.push(series_data); // Add the TvSeriesMediaInfo struct to the list
                }
            }
            Err(e) => {
                log::error!(target: "cli", "Error scanning TV directory {:?}: {}", folder, e);
            }
        }
    }

    // Log the collected series information (optional, already logged above)
    // for series_data in &all_tv_series {
    //     log::info!(target: "cli", "Collected Series: '{}', Episodes: {}", series_data.name, series_data.episodes.len());
    // }

    // Generate index pages
    if !noop {
        // Create TV series index info
        let tv_series_index: Vec<TvSeriesIndexInfo> = all_tv_series.iter().map(|series| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            series.path.to_str().hash(&mut hasher);
            let page_name = hasher.finish().to_string() + ".html";
            TvSeriesIndexInfo {
                name: series.name.clone(),
                year: series.year,
                episodes_count: series.episodes.len(),
                poster_url: series.poster_url.to_string(), // Make sure this field exists
                page_url: page_name,
            }
        }).collect();

        // Generate main index page
        let mut index_ctx = tera::Context::new();
        index_ctx.insert("movie_count", &all_movies.len());
        index_ctx.insert("tv_count", &all_tv_series.len());
        let index_html = template.render("index.html", &index_ctx).unwrap();
        std::fs::write(output_path.join("index.html"), index_html).unwrap();
        
        // Generate movies listing page
        let mut movies_ctx = tera::Context::new();
        movies_ctx.insert("movies", &all_movies);
        let movies_html = template.render("movies.html", &movies_ctx).unwrap();
        std::fs::write(output_path.join("movies.html"), movies_html).unwrap();
        
        // Generate TV series listing page
        let mut tv_ctx = tera::Context::new();
        tv_ctx.insert("series", &tv_series_index);
        let tv_html = template.render("tv.html", &tv_ctx).unwrap();
        std::fs::write(output_path.join("tv.html"), tv_html).unwrap();
        
        // Write CSS and JS files
        std::fs::write(output_path.join("media.css"), DEFAULT_CSS_FILE).unwrap();
        std::fs::write(output_path.join("media.js"), DEFAULT_JS_FILE).unwrap();
    }
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
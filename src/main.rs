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
mod cache; // Add cache module
mod search; // Add search module

use media::MediaInfoEquiv;
use tv::{TvSeriesMediaInfo, TvSeriesInfo};
use search::{SearchIndex, SearchIndexEntry, generate_id, build_meta_string};
// Removed: use crate::movie::generate_movie_html;
// Removed: use crate::tv::generate_tv_show_html_list;

const DEFAULT_CSS_FILE: &str = include_str!("media.css");
const DEFAULT_INDEX_HTML_TEMPLATE: &str = include_str!("index.html");
const DEFAULT_JS_FILE: &str = include_str!("./../static/media.js");
const DEFAULT_SEARCH_JS_FILE: &str = include_str!("./../static/search.js");

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
    
    let matches = clap::Command::new("nascast")
    .about("NASCast: A tool for generating HTML pages from movies and TV shows for streaming")
    .subcommand_required(true)
    .subcommand(
        clap::Command::new("index")
            .about("Generate HTML pages from movies and TV shows")
            .arg(clap::Arg::new("movies-folder").long("movies-folder").action(clap::ArgAction::Append))
            .arg(clap::Arg::new("tv-folder").long("tv-folder").action(clap::ArgAction::Append))
            .arg(clap::Arg::new("output-folder").long("output-folder").default_value("./pub"))
            .arg(clap::Arg::new("base-url").long("base-url"))
            .arg(clap::Arg::new("omdb-api-key").long("omdb-api-key").required(true))
            .arg(clap::Arg::new("cache-path").long("cache-path").default_value("./nascast_cache.sqlite"))
            .arg(clap::Arg::new("noop").long("noop").help("NoOp mode: only show metadata, does not write anything to disk").action(clap::ArgAction::SetTrue))
            .arg(clap::Arg::new("verbosity").long("verbosity").short('v').action(clap::ArgAction::Set))
    )
    .subcommand(
        clap::Command::new("webserver")
            .about("Start a web server to serve the generated content")
            .arg(clap::Arg::new("movies-folder").long("movies-folder").action(clap::ArgAction::Append))
            .arg(clap::Arg::new("tv-folder").long("tv-folder").action(clap::ArgAction::Append))
            .arg(clap::Arg::new("html-folder").long("html-folder").default_value("./pub"))
            .arg(clap::Arg::new("base-url").long("base-url"))
            .arg(clap::Arg::new("port").long("port").default_value("8000").help("Port to run the web server on"))
    )
    .get_matches();
    
    match matches.subcommand() {
        Some(("webserver", webserver_matches)) => {
            let html_dir = webserver_matches.get_one::<String>("html-folder").expect("HTML folder required").to_string();
            let port = webserver_matches.get_one::<String>("port").expect("Port required").parse::<u16>().expect("Port must be a number");
            
            // If any folders are specified, first generate the content
            let movies_folders = webserver_matches.get_many::<String>("movies-folder").unwrap().map(|s| split_2_or(s, None)).collect();
            let tv_folders = webserver_matches.get_many::<String>("tv-folder").unwrap().map(|s| split_2_or(s, None)).collect();
            let base_url = webserver_matches.get_one::<String>("base-url").and_then(|s| url::Url::parse(s).ok());
                        
            // Start the web server
            if let Err(e) = start_webserver(html_dir, tv_folders, movies_folders, port, base_url) {
                log::error!(target: "cli", "Failed to start web server: {}", e);
            }
        },
        Some(("index", index_matches)) => {
            // Original command processing moved here for the index subcommand
            // Process the index command with the content generation logic
            generate_content(index_matches);
        },
        _ => {
            // This should not happen because of subcommand_required(true),
            // but handle it just in case
            eprintln!("No subcommand was provided");
            std::process::exit(1);
        }
    }
}

// Function for content generation (the index subcommand)
fn generate_content(app: &clap::ArgMatches) {
    let mut template = tera::Tera::default();
    template.add_raw_template("base.html", include_str!("base.html")).unwrap();
    template.add_raw_template("movie.html", include_str!("movie.html")).unwrap();
    template.add_raw_template("index.html", DEFAULT_INDEX_HTML_TEMPLATE).unwrap();
    template.add_raw_template("movies.html", include_str!("movies.html")).unwrap();
    template.add_raw_template("tv.html", include_str!("tv.html")).unwrap();
    template.add_raw_template("series.html", include_str!("series.html")).unwrap();
    let output_dir = app.get_one::<String>("output-folder").expect("Output folder required");
    let base_url = app.get_one::<String>("base-url").and_then(|s| url::Url::parse(s).ok());
    let output_path = Path::new(&output_dir);
    let omdb_api_key = app.get_one::<String>("omdb-api-key").expect("OMDB API Key required");
    let noop = app.get_flag("noop");
    
    // Initialize the SQLite cache
    let cache_path_str = app.get_one::<String>("cache-path").expect("Cache path required");
    let cache_path = Path::new(cache_path_str); // Use the string slice directly
    let cache = match crate::cache::MediaCache::new(cache_path) {
        Ok(c) => {
            log::info!(target: "cli", "Media cache initialized at: {}", cache_path.display());
            Some(c)
        }
        Err(err) => {
            log::error!(target: "cli", "Failed to initialize media cache at {}: {}. Proceeding without cache.", cache_path.display(), err);
            None
        }
    };
    
    std::fs::create_dir_all(output_path).unwrap();
    
    let mut all_movies = Vec::new();
    let mut all_movie_infos = Vec::new(); // For search index generation
    
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
            .filter_map(|info| movie::get_movie_info_logged(omdb_api_key, info, &cache).ok() ) // Pass cache
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

            // Store for search index
            all_movie_infos.push(movie_info.clone());

            if !noop {
                render(movie_info);
            }
        }
    }

    // Changed from HashMap to Vec<(TvSeriesMediaInfo, Option<TvSeriesInfo>)>
    let mut all_tv_series: Vec<(TvSeriesMediaInfo, Option<TvSeriesInfo>)> = Vec::new(); 

    for folder_spec in app.get_many::<String>("tv-folder").unwrap_or_default() {
        let (s_folder, mount) = split_2_or(&folder_spec, None);
        let folder = Path::new(&s_folder);
        log::info!(target: "cli", "Scanning TV folder: {:?}", folder);

        match tv::scan_tv_directory(folder) {
            Ok(series_list) => {
                log::info!(target: "cli", "Found {} series in TV folder: {:?}" , series_list.len(), folder);
                for mut series_data in series_list {
                    log::info!(target: "cli", "  Found Series: '{}', Year: {:?}, Path: {:?}, Episodes: {}", 
                               series_data.name, series_data.year, series_data.path, series_data.episodes.len());
                    // Get OMDB data for the series ONCE and store Option<TvSeriesInfo>
                    let series_info = tv::get_series_info(omdb_api_key, &series_data.name, &cache).ok(); // Pass cache
                    if let Some(ref info) = series_info {
                        // Update series metadata with OMDB data
                        series_data.poster_url = Some(info.poster_url.to_string());
                        series_data.released = Some(info.released.clone());
                        series_data.genre = Some(info.genre.clone());
                        series_data.plot = Some(info.plot.clone());
                        series_data.actors = Some(info.actors.clone());
                        series_data.language = Some(info.language.clone());
                        series_data.country = Some(info.country.clone());
                        series_data.imdb_rating = Some(info.imdb_rating.clone());
                        series_data.total_seasons = Some(info.total_seasons.clone());
                        series_data.year = info.year;
                    }
                    // For each episode, get detailed info and set media_ref
                    for episode in series_data.episodes.iter_mut() {
                        if let Ok(ep_info) = tv::get_episode_info(omdb_api_key, &series_data.name, episode.season, episode.episode, &cache) { // Pass cache
                            episode.title = Some(ep_info.title);
                            episode.plot = ep_info.plot;
                            episode.imdb_rating = ep_info.imdb_rating;
                            episode.air_date = ep_info.aired_date;
                            episode.director = ep_info.director;
                        }
                        // Set media_ref for episode
                        let generated_ref = gen_media_ref(&base_url, folder, &mount, &episode.path);
                        episode.media_ref = Some(generated_ref);
                    }
                    all_tv_series.push((series_data, series_info));
                }
            }
            Err(e) => {
                log::error!(target: "cli", "Error scanning TV directory {:?}: {}", folder, e);
            }
        }
    }

    // Generate index pages
    if !noop {
        // Create TV series index info
        let tv_series_index: Vec<TvSeriesIndexInfo> = all_tv_series.iter().map(|(series, _series_info)| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            series.path.to_str().hash(&mut hasher);
            let page_name = hasher.finish().to_string() + ".html";
            TvSeriesIndexInfo {
                name: series.name.clone(),
                year: series.year,
                episodes_count: series.episodes.len(),
                poster_url: series.poster_url.as_deref().unwrap_or("https://via.placeholder.com/300x450.png?text=No+Poster").to_string(),
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
        
        // Generate TV series detail pages (one per series)
        for (series, series_info) in &all_tv_series {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            series.path.to_str().hash(&mut hasher);
            let page_name = hasher.finish().to_string() + ".html";

            // Use the OMDB info already fetched
            // Group episodes by season
            let mut seasons_map: std::collections::BTreeMap<u8, Vec<_>> = std::collections::BTreeMap::new();
            for ep in &series.episodes {
                seasons_map.entry(ep.season).or_default().push(ep);
            }
            let seasons = seasons_map.into_iter().map(|(season_number, episodes)| {
                tv::SeasonTemplateData {
                    season_number,
                    episodes: episodes.iter().map(|ep| tv::EpisodeTemplateData {
                        title: ep.title.clone().unwrap_or_default(),
                        episode_number: ep.episode,
                        plot: ep.plot.clone(),
                        imdb_rating: ep.imdb_rating.clone(),
                        aired_date: ep.air_date.clone(),
                        director: ep.director.clone(),
                        media_ref: ep.media_ref.clone().unwrap_or_default(),
                    }).collect(),
                }
            }).collect();
            let page_data = tv::SeriesPageTemplateData {
                series_info: series_info.clone(),
                seasons,
                name: series.name.clone(),
            };
            let mut ctx = tera::Context::new();
            ctx.insert("media_info", &page_data);
            let html = template.render("series.html", &ctx).unwrap();
            std::fs::write(output_path.join(page_name), html).unwrap();
        }
        
        // Generate search index
        let mut search_index = SearchIndex::new();
        
        // Add movies to search index
        for movie in &all_movies {
            // Find the corresponding MovieInfo to get metadata
            let movie_metadata = all_movie_infos.iter().find(|m| m.name == movie.name && m.year == movie.year);
            
            let meta = if let Some(movie_info) = movie_metadata {
                build_meta_string(
                    Some(&movie_info.genre),
                    Some(&movie_info.actors),
                    Some(&movie_info.director),
                    None // Movies don't typically have separate writer field in our data
                )
            } else {
                build_meta_string(
                    None,
                    None,
                    Some(&movie.director),
                    None
                )
            };
            
            search_index.add_entry(SearchIndexEntry {
                id: generate_id(&std::path::PathBuf::from(&movie.name), "movie"),
                title: format!("{} ({})", movie.name, movie.year),
                year: Some(movie.year),
                media_type: "movie".to_string(),
                url: movie.page_url.clone(),
                poster_url: movie.poster_url.clone(),
                meta,
            });
        }
        
        // Add TV series to search index
        for series_index in &tv_series_index {
            // Find the corresponding series info for metadata
            let series_data = all_tv_series.iter().find(|(s, _)| s.name == series_index.name);
            
            let meta = if let Some((series, Some(series_info))) = series_data {
                build_meta_string(
                    series.genre.as_deref(),
                    series.actors.as_deref(),
                    Some(&series_info.director),
                    None
                )
            } else if let Some((series, None)) = series_data {
                build_meta_string(
                    series.genre.as_deref(),
                    series.actors.as_deref(),
                    None,
                    None
                )
            } else {
                String::new()
            };
            
            let title = if let Some(year) = series_index.year {
                format!("{} ({})", series_index.name, year)
            } else {
                series_index.name.clone()
            };
            
            search_index.add_entry(SearchIndexEntry {
                id: generate_id(&std::path::PathBuf::from(&series_index.name), "series"),
                title,
                year: series_index.year,
                media_type: "series".to_string(),
                url: series_index.page_url.clone(),
                poster_url: series_index.poster_url.clone(),
                meta,
            });
        }
        
        // Add episodes to search index
        for (series, _series_info) in &all_tv_series {
            for episode in &series.episodes {
                let meta = build_meta_string(
                    None, // Episodes don't have genre
                    None, // Episodes don't have separate actors
                    episode.director.as_deref(),
                    None  // Could add writer if available in episode data
                );
                
                let title = format!("{} - {} S{:02}E{:02}{}", 
                    series.name, 
                    series.name,
                    episode.season, 
                    episode.episode,
                    episode.title.as_ref().map(|t| format!(": {}", t)).unwrap_or_default()
                );
                
                search_index.add_entry(SearchIndexEntry {
                    id: generate_id(&episode.path, "episode"),
                    title,
                    year: series.year,
                    media_type: "episode".to_string(),
                    url: format!("{}#{}-s{}e{}", 
                        // Use the series page URL and add anchor
                        tv_series_index.iter()
                            .find(|s| s.name == series.name)
                            .map(|s| s.page_url.clone())
                            .unwrap_or_default(),
                        series.name.to_lowercase().replace(" ", "-"),
                        episode.season,
                        episode.episode
                    ),
                    poster_url: series.poster_url.as_deref().unwrap_or("https://via.placeholder.com/300x450.png?text=No+Poster").to_string(),
                    meta,
                });
            }
        }
        
        // Write search index to file
        let search_index_json = serde_json::to_string_pretty(&search_index).unwrap();
        std::fs::write(output_path.join("search-index.json"), search_index_json).unwrap();
        
        // Write CSS and JS files
        std::fs::write(output_path.join("media.css"), DEFAULT_CSS_FILE).unwrap();
        std::fs::write(output_path.join("media.js"), DEFAULT_JS_FILE).unwrap();
        std::fs::write(output_path.join("search.js"), DEFAULT_SEARCH_JS_FILE).unwrap();
    }
}

// Function to start the web server
fn start_webserver(html_dir: String, tv_folders: Vec<(String, String)>, movies_folder: Vec<(String, String)>, port: u16, base_url: Option<url::Url>) -> std::io::Result<()> {
    use rouille::Response;
    use std::path::PathBuf;
    
    log::info!(target: "cli", "Starting web server on port {} serving files from {}", port, html_dir);
    
    let html_dir_path = PathBuf::from(html_dir);
    let address = format!("127.0.0.1:{}", port);
    
    // Determine the base path from base_url
    let base_path = if let Some(ref url) = base_url {
        url.path().trim_end_matches('/').to_string()
    } else {
        String::new()
    };
    
    // Rouille's start_server doesn't return, so we don't need to handle the result
    println!("Starting web server at http://{}", address);
    rouille::start_server(address, move |request| {
        log::info!(target: "cli", "{} {}", request.method(), request.url());
        
        // Strip base path from request URL if it exists
        let request_url = request.url();
        let request_path = if !base_path.is_empty() && request_url.starts_with(&base_path) {
            &request_url[base_path.len()..]
        } else if base_path.is_empty() {
            &request_url
        } else {
            // Request doesn't match base path, return 404
            return Response::html("Not found").with_status_code(404);
        };

        // First try to handle media folders
        for (folder, mount) in &movies_folder {
            let mount_str = format!("/{}", mount);
            if request_path.starts_with(&mount_str) {
                let rel_path = &request_path[mount_str.len()..];
                let file_path = PathBuf::from(folder).join(rel_path.trim_start_matches('/'));
                if file_path.exists() {
                    // Use mime_guess to determine content type
                    let mime = mime_guess::from_path(&file_path).first_or_octet_stream().to_string();
                    return Response::from_file(mime, std::fs::File::open(file_path).unwrap());
                }
            }
        }
        
        // Then try TV folders
        for (folder, mount) in &tv_folders {
            let mount_str = format!("/{}", mount);
            if request_path.starts_with(&mount_str) {
                let rel_path = &request_path[mount_str.len()..];
                let file_path = PathBuf::from(folder).join(rel_path.trim_start_matches('/'));
                if file_path.exists() {
                    // Use mime_guess to determine content type
                    let mime = mime_guess::from_path(&file_path).first_or_octet_stream().to_string();
                    return Response::from_file(mime, std::fs::File::open(file_path).unwrap());
                }
            }
        }
        
        // Finally serve the static HTML files using the stripped request path
        match request_path {
            "/" => {
                let index_path = html_dir_path.join("index.html");
                if index_path.exists() {
                    Response::from_file("text/html", std::fs::File::open(index_path).unwrap())
                } else {
                    Response::text("Index file not found").with_status_code(404)
                }
            },
            "/media.css" => {
                Response::from_file("text/css", std::fs::File::open(html_dir_path.join("media.css")).unwrap())
            },
            "/media.js" => {
                Response::from_file("application/javascript", std::fs::File::open(html_dir_path.join("media.js")).unwrap())
            },
            "/search.js" => {
                Response::from_file("application/javascript", std::fs::File::open(html_dir_path.join("search.js")).unwrap())
            },
            "/search-index.json" => {
                Response::from_file("application/json", std::fs::File::open(html_dir_path.join("search-index.json")).unwrap())
            },
            "/movies.html" => {
                Response::from_file("text/html", std::fs::File::open(html_dir_path.join("movies.html")).unwrap())
            },
            "/tv.html" => {
                Response::from_file("text/html", std::fs::File::open(html_dir_path.join("tv.html")).unwrap())
            },
            // Fallback for other HTML files (media pages)
            _ => {
                let path = html_dir_path.join(request_path.trim_start_matches('/'));
                if path.exists() && path.is_file() {
                    // Use mime_guess to determine content type
                    let mime = mime_guess::from_path(&path).first_or_octet_stream().to_string();
                    Response::from_file(mime, std::fs::File::open(path).unwrap())
                } else {
                    log::info!(target: "cli", "404 for path: {}", request_path);
                    Response::html("Not found").with_status_code(404)
                }
            }
        }
    });
    
    // This function never actually returns because start_server blocks indefinitely
    // We keep the return type for compatibility with the rest of the code
    #[allow(unreachable_code)]
    Ok(())
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
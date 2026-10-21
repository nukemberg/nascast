use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use serde::Serialize;
use url::Url;

use crate::cache::MediaCache;
use crate::media::{playability_rank, MediaVersion};
use crate::movie::{self, MovieInfo};
use crate::search::{build_meta_string, generate_id, SearchIndex, SearchIndexEntry};
use crate::tv::{self, SeriesPageTemplateData, TvSeriesInfo, TvSeriesMediaInfo};

const TEMPLATES: &[(&str, &str)] = &[
    ("base.html", include_str!("base.html")),
    ("movie.html", include_str!("movie.html")),
    ("index.html", include_str!("index.html")),
    ("movies.html", include_str!("movies.html")),
    ("tv.html", include_str!("tv.html")),
    ("series.html", include_str!("series.html")),
];

const STATIC_FILES: &[(&str, &str)] = &[
    ("media.css", include_str!("media.css")),
    ("media.js", include_str!("../static/media.js")),
    ("search.js", include_str!("../static/search.js")),
];

const PLACEHOLDER_POSTER_URL: &str = "https://via.placeholder.com/300x450.png?text=No+Poster";

/// A media folder on disk and the URL path segment it is served under.
pub struct MediaFolder {
    pub path: PathBuf,
    pub mount: String,
}

pub struct IndexConfig {
    pub movies_folders: Vec<MediaFolder>,
    pub tv_folders: Vec<MediaFolder>,
    pub output_dir: PathBuf,
    pub base_url: Option<Url>,
    pub omdb_api_key: String,
    pub cache_path: PathBuf,
    /// Only log metadata; don't write any pages.
    pub noop: bool,
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

type TvSeries = (TvSeriesMediaInfo, Option<TvSeriesInfo>);

/// A movie with all its files; `info.path` is the primary file.
struct Movie {
    info: MovieInfo,
    media_ref: String,
    alternates: Vec<MediaVersion>,
}

fn scan_movie_files(basepath: &Path) -> Vec<PathBuf> {
    walkdir::WalkDir::new(basepath)
        .max_depth(2)
        .into_iter()
        .filter_map(|entry| entry.ok().map(|e| e.into_path()))
        .filter(|path| path.is_file())
        .filter(|path| path.extension().is_some_and(|ext| ext == "mp4"))
        .collect()
}

/// Builds the link used to play a media file, either relative or under `base_url`.
pub fn gen_media_ref(base_url: &Option<Url>, folder_path: &Path, folder_mount: &str, media_path: &Path) -> String {
    let relative_path = media_path.strip_prefix(folder_path).unwrap();
    let components = relative_path.components().map(|c| c.as_os_str().to_str().unwrap());

    match base_url {
        // `Url::join` takes care of percent-encoding
        Some(url) => {
            let path = std::iter::once(folder_mount).chain(components).collect::<Vec<_>>().join("/");
            url.join(&path).unwrap().to_string()
        }
        None => std::iter::once(folder_mount.to_string())
            .chain(components.map(|c| urlencoding::encode(c).into_owned()))
            .collect::<Vec<_>>()
            .join("/"),
    }
}

/// Stable HTML file name for a media path.
fn page_name(path: &Path) -> String {
    let mut hasher = DefaultHasher::new();
    path.to_str().hash(&mut hasher);
    hasher.finish().to_string() + ".html"
}

fn series_poster_url(series: &TvSeriesMediaInfo) -> String {
    series.poster_url.as_deref().unwrap_or(PLACEHOLDER_POSTER_URL).to_string()
}

fn year_suffixed(name: &str, year: Option<u16>) -> String {
    match year {
        Some(year) => format!("{} ({})", name, year),
        None => name.to_string(),
    }
}

fn render(templates: &tera::Tera, template: &str, ctx: &tera::Context, output_path: &Path) {
    let html = templates.render(template, ctx).unwrap();
    std::fs::write(output_path, html).unwrap();
}

fn load_templates() -> tera::Tera {
    let mut templates = tera::Tera::default();
    templates.add_raw_templates(TEMPLATES.iter().copied()).unwrap();
    templates
}

fn open_cache(cache_path: &Path) -> Option<MediaCache> {
    match MediaCache::new(cache_path) {
        Ok(c) => {
            log::info!(target: "cli", "Media cache initialized at: {}", cache_path.display());
            Some(c)
        }
        Err(err) => {
            log::error!(target: "cli", "Failed to initialize media cache at {}: {}. Proceeding without cache.", cache_path.display(), err);
            None
        }
    }
}

fn collect_movies<'a>(config: &'a IndexConfig, cache: Option<&MediaCache>) -> Vec<(MovieInfo, &'a MediaFolder)> {
    config.movies_folders.iter()
        .flat_map(|folder| {
            scan_movie_files(&folder.path).into_iter()
                .filter_map(|file| movie::parse_movie_filename(&movie::MOVIE_PATTERNS_RE, &file))
                .filter_map(|info| movie::get_movie_info_logged(&config.omdb_api_key, info, cache).ok())
                .map(move |info| (info, folder))
        })
        .collect()
}

/// Groups files of the same movie (same IMDb entry) into one `Movie`, keeping the most
/// playable file as primary. Movies keep the order in which they were first found.
fn group_movies(config: &IndexConfig, found: Vec<(MovieInfo, &MediaFolder)>) -> Vec<Movie> {
    let mut groups: Vec<Vec<(MovieInfo, &MediaFolder)>> = Vec::new();
    for entry in found {
        match groups.iter_mut().find(|g| g[0].0.info_url == entry.0.info_url) {
            Some(group) => group.push(entry),
            None => groups.push(vec![entry]),
        }
    }

    let media_ref = |info: &MovieInfo, folder: &MediaFolder| gen_media_ref(&config.base_url, &folder.path, &folder.mount, &info.path);
    groups.into_iter().map(|mut group| {
        group.sort_by(|(a, _), (b, _)| (playability_rank(&a.path), &a.path).cmp(&(playability_rank(&b.path), &b.path)));
        let mut files = group.into_iter();
        let (info, folder) = files.next().unwrap();
        Movie {
            media_ref: media_ref(&info, folder),
            alternates: files.map(|(alt, folder)| MediaVersion::new(&alt.path, media_ref(&alt, folder))).collect(),
            info,
        }
    }).collect()
}

/// Merges series found under the same name in different TV folders and collapses
/// duplicate episode files. The result is sorted by series name.
fn merge_tv_series(found: Vec<TvSeries>) -> Vec<TvSeries> {
    let mut merged: Vec<TvSeries> = Vec::new();
    for (series, series_info) in found {
        match merged.iter_mut().find(|(s, _)| s.name == series.name) {
            Some((existing, _)) => existing.merge(series),
            None => merged.push((series, series_info)),
        }
    }
    for (series, _) in merged.iter_mut() {
        series.dedupe_episodes();
    }
    merged.sort_by(|(a, _), (b, _)| a.name.cmp(&b.name));
    merged
}

fn collect_tv_series(config: &IndexConfig, cache: Option<&MediaCache>) -> Vec<TvSeries> {
    let mut all_tv_series = Vec::new();
    for folder in &config.tv_folders {
        log::info!(target: "cli", "Scanning TV folder: {:?}", folder.path);
        let series_list = match tv::scan_tv_directory(&folder.path) {
            Ok(series_list) => series_list,
            Err(e) => {
                log::error!(target: "cli", "Error scanning TV directory {:?}: {}", folder.path, e);
                continue;
            }
        };
        log::info!(target: "cli", "Found {} series in TV folder: {:?}", series_list.len(), folder.path);

        for mut series in series_list {
            log::info!(target: "cli", "  Found Series: '{}', Year: {:?}, Path: {:?}, Episodes: {}",
                       series.name, series.year, series.path, series.episodes.len());
            let series_info = tv::get_series_info(&config.omdb_api_key, &series.name, cache).ok();
            if let Some(info) = &series_info {
                series.apply_series_info(info);
            }
            for episode in series.episodes.iter_mut() {
                if let Ok(ep_info) = tv::get_episode_info(&config.omdb_api_key, &series.name, episode.season, episode.episode, cache) {
                    episode.apply_episode_info(ep_info);
                }
                episode.media_ref = Some(gen_media_ref(&config.base_url, &folder.path, &folder.mount, &episode.path));
            }
            all_tv_series.push((series, series_info));
        }
    }
    merge_tv_series(all_tv_series)
}

fn build_search_index(movies: &[Movie], all_tv_series: &[TvSeries]) -> SearchIndex {
    let mut search_index = SearchIndex::new();

    for Movie { info: movie, .. } in movies {
        search_index.add_entry(SearchIndexEntry {
            id: generate_id(&movie.path, "movie"),
            title: format!("{} ({})", movie.name, movie.year),
            year: Some(movie.year),
            media_type: "movie".to_string(),
            url: page_name(&movie.path),
            poster_url: movie.poster_url.to_string(),
            meta: build_meta_string(Some(&movie.genre), Some(&movie.actors), Some(&movie.director), None),
        });
    }

    for (series, series_info) in all_tv_series {
        search_index.add_entry(SearchIndexEntry {
            id: generate_id(&series.path, "series"),
            title: year_suffixed(&series.name, series.year),
            year: series.year,
            media_type: "series".to_string(),
            url: page_name(&series.path),
            poster_url: series_poster_url(series),
            meta: build_meta_string(
                series.genre.as_deref(),
                series.actors.as_deref(),
                series_info.as_ref().map(|info| info.director.as_str()),
                None,
            ),
        });
    }

    for (series, _) in all_tv_series {
        let series_page = page_name(&series.path);
        for episode in &series.episodes {
            search_index.add_entry(SearchIndexEntry {
                id: generate_id(&episode.path, "episode"),
                title: format!("{} S{:02}E{:02}{}",
                    series.name,
                    episode.season,
                    episode.episode,
                    episode.title.as_ref().map(|t| format!(": {}", t)).unwrap_or_default()
                ),
                year: series.year,
                media_type: "episode".to_string(),
                url: format!("{}#{}", series_page, tv::episode_anchor(episode.season, episode.episode)),
                poster_url: series_poster_url(series),
                meta: build_meta_string(None, None, episode.director.as_deref(), None),
            });
        }
    }

    search_index
}

/// Scans the configured folders, fetches metadata and writes the static site.
pub fn generate_content(config: &IndexConfig) {
    let cache = open_cache(&config.cache_path);
    let cache = cache.as_ref();

    let movies = group_movies(config, collect_movies(config, cache));
    let all_tv_series = collect_tv_series(config, cache);

    if config.noop {
        for movie in &movies {
            println!("Media: {:?}", movie.info);
        }
        return;
    }

    let output_path = config.output_dir.as_path();
    std::fs::create_dir_all(output_path).unwrap();
    let templates = load_templates();

    // Movie detail pages
    for movie in &movies {
        let mut ctx = tera::Context::new();
        ctx.insert("media_ref", &movie.media_ref);
        ctx.insert("media_info", &movie.info);
        ctx.insert("alternates", &movie.alternates);
        render(&templates, "movie.html", &ctx, &output_path.join(page_name(&movie.info.path)));
    }

    // Main index page
    let mut index_ctx = tera::Context::new();
    index_ctx.insert("movie_count", &movies.len());
    index_ctx.insert("tv_count", &all_tv_series.len());
    render(&templates, "index.html", &index_ctx, &output_path.join("index.html"));

    // Movies listing page
    let movies_index: Vec<MovieIndexInfo> = movies.iter().map(|Movie { info: movie, .. }| MovieIndexInfo {
        name: movie.name.clone(),
        year: movie.year,
        director: movie.director.clone(),
        poster_url: movie.poster_url.to_string(),
        page_url: page_name(&movie.path),
    }).collect();
    let mut movies_ctx = tera::Context::new();
    movies_ctx.insert("movies", &movies_index);
    render(&templates, "movies.html", &movies_ctx, &output_path.join("movies.html"));

    // TV series listing page
    let tv_series_index: Vec<TvSeriesIndexInfo> = all_tv_series.iter().map(|(series, _)| TvSeriesIndexInfo {
        name: series.name.clone(),
        year: series.year,
        episodes_count: series.episodes.len(),
        poster_url: series_poster_url(series),
        page_url: page_name(&series.path),
    }).collect();
    let mut tv_ctx = tera::Context::new();
    tv_ctx.insert("series", &tv_series_index);
    render(&templates, "tv.html", &tv_ctx, &output_path.join("tv.html"));

    // TV series detail pages
    for (series, series_info) in &all_tv_series {
        let mut ctx = tera::Context::new();
        ctx.insert("media_info", &SeriesPageTemplateData::new(series, series_info.clone()));
        render(&templates, "series.html", &ctx, &output_path.join(page_name(&series.path)));
    }

    let search_index = build_search_index(&movies, &all_tv_series);
    std::fs::write(output_path.join("search-index.json"), serde_json::to_string_pretty(&search_index).unwrap()).unwrap();

    for (name, content) in STATIC_FILES {
        std::fs::write(output_path.join(name), content).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;
    use url::Url;
    use super::{gen_media_ref, merge_tv_series};
    use crate::tv::{scan_tv_directory, TvSeriesMediaInfo};

    // nascast --movies-folder /media/storage/Movies:movies --base-url https://pi.nukembase
    // href - https://pi.nukembase/movies/somemovie.mp4
    #[test]
    fn test_media_ref() {
        let path = Path::new("./Movies/Some movie 1993/some movie 1993.mp4");
        assert_eq!(gen_media_ref(&Url::parse("https://someserver:8080/media/").ok(), Path::new("./Movies"), "movies", path), "https://someserver:8080/media/movies/Some%20movie%201993/some%20movie%201993.mp4");
        // technically speaking, the base url should end with / or the last component isn't "the base". Might be confusing but there we are
        assert_eq!(gen_media_ref(&Url::parse("https://someserver:8080/media").ok(), Path::new("./Movies"), "movies", path), "https://someserver:8080/movies/Some%20movie%201993/some%20movie%201993.mp4");
        assert_eq!(gen_media_ref(&None, Path::new("./Movies"), "movies", path), "movies/Some%20movie%201993/some%20movie%201993.mp4");
    }

    #[test]
    fn test_merge_tv_series_across_folders() {
        let dirs = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        for (dir, file) in dirs.iter().zip(["Show.S01E01.mkv", "Show.S02E01.mkv"]) {
            std::fs::create_dir_all(dir.path().join("Show")).unwrap();
            std::fs::File::create(dir.path().join("Show").join(file)).unwrap();
        }
        let found: Vec<(TvSeriesMediaInfo, _)> = dirs.iter()
            .flat_map(|dir| scan_tv_directory(dir.path()).unwrap())
            .map(|series| (series, None))
            .collect();
        assert_eq!(found.len(), 2);

        let merged = merge_tv_series(found);
        assert_eq!(merged.len(), 1);
        let episodes: Vec<_> = merged[0].0.episodes.iter().map(|e| (e.season, e.episode)).collect();
        assert_eq!(episodes, vec![(1, 1), (2, 1)]);
        assert_eq!(merged[0].0.path, dirs[0].path().join("Show"));
    }
}

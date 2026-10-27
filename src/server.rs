use std::path::{Component, Path, PathBuf};
use rouille::Response;
use url::Url;

use crate::config::MediaFolder;

fn not_found() -> Response {
    Response::html("Not found").with_status_code(404)
}

fn serve_file(path: &Path) -> Response {
    match std::fs::File::open(path) {
        Ok(file) => Response::from_file(mime_guess::from_path(path).first_or_octet_stream().to_string(), file),
        Err(_) => not_found(),
    }
}

/// Resolves a URL path relative to `root`, refusing anything that could escape it.
fn resolve(root: &Path, rel_path: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel_path.trim_start_matches('/'));
    rel_path.components().all(|c| matches!(c, Component::Normal(_)))
        .then(|| root.join(rel_path))
}

fn handle(request: &rouille::Request, base_path: &str, html_dir: &Path, media_folders: &[MediaFolder]) -> Response {
    let Some(request_path) = request.url().strip_prefix(base_path).map(str::to_owned) else {
        return not_found();
    };
    let request_path = if request_path.is_empty() { "/" } else { request_path.as_str() };

    // Media folders take precedence over the generated HTML
    for folder in media_folders {
        let Some(rel_path) = request_path.strip_prefix(&format!("/{}", folder.mount)) else { continue };
        if let Some(file_path) = resolve(&folder.path, rel_path).filter(|p| p.is_file()) {
            return serve_file(&file_path);
        }
    }

    let rel_path = if request_path == "/" { "index.html" } else { request_path };
    match resolve(html_dir, rel_path).filter(|p| p.is_file()) {
        Some(path) => serve_file(&path),
        None => {
            log::info!(target: "cli", "404 for path: {}", request_path);
            not_found()
        }
    }
}

/// Serves the generated HTML and the media folders on `bind:port`. Never returns.
pub fn start_webserver(html_dir: PathBuf, media_folders: Vec<MediaFolder>, bind: &str, port: u16, base_url: Option<Url>) -> ! {
    log::info!(target: "cli", "Starting web server on port {} serving files from {}", port, html_dir.display());

    let address = format!("{}:{}", bind, port);
    let base_path = base_url.map(|url| url.path().trim_end_matches('/').to_string()).unwrap_or_default();

    println!("Starting web server at http://{}", address);
    rouille::start_server(address, move |request| {
        log::info!(target: "cli", "{} {}", request.method(), request.url());
        // The Chromecast fetches subtitle tracks cross-origin and refuses them without CORS
        handle(request, &base_path, &html_dir, &media_folders)
            .with_additional_header("Access-Control-Allow-Origin", "*")
    })
}

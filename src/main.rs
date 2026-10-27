use std::path::PathBuf;
use clap::{Arg, ArgAction, ArgMatches, Command};
use url::Url;

mod cache;
mod generate;
mod media;
mod movie;
mod opensubtitles;
mod search;
mod server;
mod subtitles;
mod tv;

use generate::{IndexConfig, MediaFolder};

fn init_logging() {
    use log4rs::append::console::ConsoleAppender;
    use log4rs::config::{Appender, Config, Logger, Root};

    let log_config = Config::builder()
        .appender(Appender::builder().build("stdout", Box::new(ConsoleAppender::builder().build())))
        .logger(Logger::builder().build("cli", log::LevelFilter::Info))
        .build(Root::builder().appender("stdout").build(log::LevelFilter::Warn))
        .unwrap();
    log4rs::init_config(log_config).unwrap();
}

/// Parses a `<folder>[:<mount>]` spec. Without a mount, the folder is also used as the mount.
fn parse_folder_spec(spec: &str) -> MediaFolder {
    let mut split = spec.split(':');
    let path = split.next().unwrap();
    let mount = split.next().unwrap_or(spec);
    MediaFolder { path: PathBuf::from(path), mount: mount.to_string() }
}

fn folders_arg(matches: &ArgMatches, name: &str) -> Vec<MediaFolder> {
    matches.get_many::<String>(name).unwrap_or_default().map(|s| parse_folder_spec(s)).collect()
}

fn base_url_arg(matches: &ArgMatches) -> Option<Url> {
    matches.get_one::<String>("base-url").and_then(|s| Url::parse(s).ok())
}

/// Normalizes `--subtitle-languages` to ISO 639-1 codes (`eng`, `English` -> `en`).
fn subtitle_languages_arg(matches: &ArgMatches) -> Vec<String> {
    matches.get_many::<String>("subtitle-languages").unwrap_or_default()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .map(|l| match subtitles::find_language(l) {
            Some(language) => language.code.to_string(),
            None => {
                log::warn!(target: "cli", "Unknown subtitle language '{}', passing it to OpenSubtitles as-is", l);
                l.to_lowercase()
            }
        })
        .collect()
}

fn cli() -> Command {
    let folder_args = [
        Arg::new("movies-folder").long("movies-folder").action(ArgAction::Append),
        Arg::new("tv-folder").long("tv-folder").action(ArgAction::Append),
    ];
    Command::new("nascast")
        .about("NASCast: A tool for generating HTML pages from movies and TV shows for streaming")
        .subcommand_required(true)
        .subcommand(
            Command::new("index")
                .about("Generate HTML pages from movies and TV shows")
                .args(folder_args.clone())
                .arg(Arg::new("output-folder").long("output-folder").default_value("./pub"))
                .arg(Arg::new("base-url").long("base-url"))
                .arg(Arg::new("omdb-api-key").long("omdb-api-key").env("OMDB_API_KEY").required(true))
                .arg(Arg::new("cache-path").long("cache-path").default_value("./nascast_cache.sqlite"))
                .arg(Arg::new("noop").long("noop").help("NoOp mode: only show metadata, does not write anything to disk").action(ArgAction::SetTrue))
                .arg(Arg::new("verbosity").long("verbosity").short('v').action(ArgAction::Set))
                .arg(Arg::new("subtitle-languages").long("subtitle-languages").value_delimiter(',')
                    .help("Comma-separated languages to download missing subtitles in, e.g. en,he (requires --opensubtitles-api-key)"))
                .arg(Arg::new("opensubtitles-api-key").long("opensubtitles-api-key").env("OPENSUBTITLES_API_KEY").hide_env_values(true)
                    .help("OpenSubtitles API (consumer) key; enables subtitle downloads"))
                .arg(Arg::new("opensubtitles-username").long("opensubtitles-username").env("OPENSUBTITLES_USERNAME")
                    .help("OpenSubtitles account, for a higher daily download quota"))
                .arg(Arg::new("opensubtitles-password").long("opensubtitles-password").env("OPENSUBTITLES_PASSWORD").hide_env_values(true))
                // For testing against a fake API
                .arg(Arg::new("opensubtitles-api-url").long("opensubtitles-api-url").hide(true).default_value(opensubtitles::DEFAULT_API_URL))
        )
        .subcommand(
            Command::new("webserver")
                .about("Start a web server to serve the generated content")
                .args(folder_args)
                .arg(Arg::new("html-folder").long("html-folder").default_value("./pub"))
                .arg(Arg::new("base-url").long("base-url"))
                .arg(Arg::new("port").long("port").default_value("8000").help("Port to run the web server on"))
                .arg(Arg::new("bind").long("bind").default_value("127.0.0.1").help("Address to listen on; use 0.0.0.0 so a Chromecast on the network can reach it"))
        )
}

fn main() {
    init_logging();

    match cli().get_matches().subcommand() {
        Some(("index", matches)) => {
            generate::generate_content(&IndexConfig {
                movies_folders: folders_arg(matches, "movies-folder"),
                tv_folders: folders_arg(matches, "tv-folder"),
                output_dir: PathBuf::from(matches.get_one::<String>("output-folder").unwrap()),
                base_url: base_url_arg(matches),
                omdb_api_key: matches.get_one::<String>("omdb-api-key").unwrap().clone(),
                cache_path: PathBuf::from(matches.get_one::<String>("cache-path").unwrap()),
                noop: matches.get_flag("noop"),
                subtitle_languages: subtitle_languages_arg(matches),
                opensubtitles: matches.get_one::<String>("opensubtitles-api-key").map(|api_key| opensubtitles::Credentials {
                    api_key: api_key.clone(),
                    username: matches.get_one::<String>("opensubtitles-username").cloned(),
                    password: matches.get_one::<String>("opensubtitles-password").cloned(),
                    api_url: matches.get_one::<String>("opensubtitles-api-url").unwrap().clone(),
                }),
            });
        }
        Some(("webserver", matches)) => {
            let html_dir = PathBuf::from(matches.get_one::<String>("html-folder").unwrap());
            let port = matches.get_one::<String>("port").unwrap().parse::<u16>().expect("Port must be a number");
            let mut media_folders = folders_arg(matches, "movies-folder");
            media_folders.extend(folders_arg(matches, "tv-folder"));
            let bind = matches.get_one::<String>("bind").unwrap();
            server::start_webserver(html_dir, media_folders, bind, port, base_url_arg(matches));
        }
        // subcommand_required(true) makes this unreachable
        _ => unreachable!(),
    }
}

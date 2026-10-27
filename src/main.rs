use std::path::{Path, PathBuf};
use clap::{Arg, ArgAction, ArgMatches, Command};
use url::Url;

mod cache;
mod config;
mod embedded;
mod generate;
mod media;
mod movie;
mod opensubtitles;
mod search;
mod server;
mod subtitles;
mod tv;

use config::{Config, MediaFolder};
use generate::IndexConfig;

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

fn fail(message: &str) -> ! {
    eprintln!("Error: {}", message);
    std::process::exit(1)
}

fn string_arg(matches: &ArgMatches, name: &str) -> Option<String> {
    matches.get_one::<String>(name).cloned()
}

/// Folders from the command line, or else from the config file.
fn folders_arg(matches: &ArgMatches, config: &Config) -> Vec<MediaFolder> {
    let folders = match matches.get_many::<String>("folder") {
        Some(specs) => specs.map(|s| MediaFolder::parse(s)).collect::<Result<Vec<_>, _>>().unwrap_or_else(|e| fail(&e)),
        None => config.folders.clone(),
    };
    config::check_mounts(&folders).unwrap_or_else(|e| fail(&e));
    folders
}

fn base_url_arg(matches: &ArgMatches, config: &Config) -> Option<Url> {
    match matches.get_one::<String>("base-url") {
        Some(s) => Some(Url::parse(s).unwrap_or_else(|e| fail(&format!("Invalid --base-url {}: {}", s, e)))),
        None => config.base_url.clone(),
    }
}

fn output_folder_arg(matches: &ArgMatches, name: &str, config: &Config) -> PathBuf {
    matches.get_one::<String>(name).map(PathBuf::from)
        .or_else(|| config.output_folder.clone())
        .unwrap_or_else(|| PathBuf::from(DEFAULT_OUTPUT_FOLDER))
}

/// Normalizes the preferred subtitle languages to ISO 639-1 codes (`eng`, `English` -> `en`).
fn subtitle_languages_arg(matches: &ArgMatches, config: &Config) -> Vec<String> {
    let languages: Vec<String> = match matches.get_many::<String>("subtitle-languages") {
        Some(languages) => languages.cloned().collect(),
        None => config.subtitles.languages.clone(),
    };
    languages.iter()
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

fn opensubtitles_arg(matches: &ArgMatches, config: &Config) -> Option<opensubtitles::Credentials> {
    let configured = &config.opensubtitles;
    let api_key = string_arg(matches, "opensubtitles-api-key").or_else(|| configured.api_key.clone())?;
    Some(opensubtitles::Credentials {
        api_key,
        username: string_arg(matches, "opensubtitles-username").or_else(|| configured.username.clone()),
        password: string_arg(matches, "opensubtitles-password").or_else(|| configured.password.clone()),
        api_url: string_arg(matches, "opensubtitles-api-url").unwrap(),
    })
}

const DEFAULT_OUTPUT_FOLDER: &str = "./pub";
const DEFAULT_CACHE_PATH: &str = "./nascast_cache.sqlite";
const DEFAULT_BIND: &str = "127.0.0.1";
const DEFAULT_PORT: u16 = 8000;

fn cli() -> Command {
    let folder_arg = Arg::new("folder").long("folder").action(ArgAction::Append).value_name("TYPE:PATH[:MOUNT]")
        .help("Media folder (repeatable): type movies or tv, its path, and the URL path it's served under (default: the folder's name), e.g. tv:/media/TV:tv");
    Command::new("nascast")
        .about("NASCast: A tool for generating HTML pages from movies and TV shows for streaming")
        .subcommand_required(true)
        .arg(Arg::new("config").long("config").short('c').global(true).env("NASCAST_CONFIG")
            .help("Config file (TOML); defaults to ./nascast.toml if it exists. Flags and environment variables override it"))
        .subcommand(
            Command::new("index")
                .about("Generate HTML pages from movies and TV shows")
                .arg(folder_arg.clone())
                .arg(Arg::new("output-folder").long("output-folder").help("Where to write the site [default: ./pub]"))
                .arg(Arg::new("base-url").long("base-url"))
                .arg(Arg::new("omdb-api-key").long("omdb-api-key").env("OMDB_API_KEY").hide_env_values(true))
                .arg(Arg::new("cache-path").long("cache-path").help("SQLite cache file [default: ./nascast_cache.sqlite]"))
                .arg(Arg::new("noop").long("noop").help("NoOp mode: only show metadata, does not write anything to disk").action(ArgAction::SetTrue))
                .arg(Arg::new("verbosity").long("verbosity").short('v').action(ArgAction::Set))
                .arg(Arg::new("subtitle-languages").long("subtitle-languages").value_delimiter(',')
                    .help("Preferred subtitle languages, most preferred first, e.g. en,he: pages turn the first available one on, and missing ones are downloaded when --opensubtitles-api-key is set"))
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
                .arg(folder_arg)
                .arg(Arg::new("html-folder").long("html-folder").help("Folder to serve; defaults to the config's output_folder, else ./pub"))
                .arg(Arg::new("base-url").long("base-url"))
                .arg(Arg::new("port").long("port").value_parser(clap::value_parser!(u16)).help("Port to run the web server on [default: 8000]"))
                .arg(Arg::new("bind").long("bind").help("Address to listen on; use 0.0.0.0 so a Chromecast on the network can reach it [default: 127.0.0.1]"))
        )
}

fn main() {
    init_logging();

    let matches = cli().get_matches();
    let config = Config::find(matches.get_one::<String>("config").map(Path::new)).unwrap_or_else(|e| fail(&e));
    match matches.subcommand() {
        Some(("index", matches)) => {
            let omdb_api_key = string_arg(matches, "omdb-api-key").or_else(|| config.omdb.api_key.clone())
                .unwrap_or_else(|| fail("No OMDB API key: set --omdb-api-key, OMDB_API_KEY or [omdb] api_key in the config file"));
            generate::generate_content(&IndexConfig {
                folders: folders_arg(matches, &config),
                output_dir: output_folder_arg(matches, "output-folder", &config),
                base_url: base_url_arg(matches, &config),
                omdb_api_key,
                cache_path: matches.get_one::<String>("cache-path").map(PathBuf::from)
                    .or_else(|| config.cache_path.clone())
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_CACHE_PATH)),
                noop: matches.get_flag("noop"),
                subtitle_languages: subtitle_languages_arg(matches, &config),
                opensubtitles: opensubtitles_arg(matches, &config),
            });
        }
        Some(("webserver", matches)) => {
            let html_dir = output_folder_arg(matches, "html-folder", &config);
            let port = matches.get_one::<u16>("port").copied().or(config.webserver.port).unwrap_or(DEFAULT_PORT);
            let media_folders = folders_arg(matches, &config);
            let bind = string_arg(matches, "bind").or_else(|| config.webserver.bind.clone()).unwrap_or_else(|| DEFAULT_BIND.to_string());
            server::start_webserver(html_dir, media_folders, &bind, port, base_url_arg(matches, &config));
        }
        // subcommand_required(true) makes this unreachable
        _ => unreachable!(),
    }
}

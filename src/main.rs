use std::path::PathBuf;
use clap::{Arg, ArgAction, ArgMatches, Command};
use url::Url;

mod cache;
mod generate;
mod media;
mod movie;
mod search;
mod server;
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
                .arg(Arg::new("omdb-api-key").long("omdb-api-key").required(true))
                .arg(Arg::new("cache-path").long("cache-path").default_value("./nascast_cache.sqlite"))
                .arg(Arg::new("noop").long("noop").help("NoOp mode: only show metadata, does not write anything to disk").action(ArgAction::SetTrue))
                .arg(Arg::new("verbosity").long("verbosity").short('v').action(ArgAction::Set))
        )
        .subcommand(
            Command::new("webserver")
                .about("Start a web server to serve the generated content")
                .args(folder_args)
                .arg(Arg::new("html-folder").long("html-folder").default_value("./pub"))
                .arg(Arg::new("base-url").long("base-url"))
                .arg(Arg::new("port").long("port").default_value("8000").help("Port to run the web server on"))
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
            });
        }
        Some(("webserver", matches)) => {
            let html_dir = PathBuf::from(matches.get_one::<String>("html-folder").unwrap());
            let port = matches.get_one::<String>("port").unwrap().parse::<u16>().expect("Port must be a number");
            let mut media_folders = folders_arg(matches, "movies-folder");
            media_folders.extend(folders_arg(matches, "tv-folder"));
            server::start_webserver(html_dir, media_folders, port, base_url_arg(matches));
        }
        // subcommand_required(true) makes this unreachable
        _ => unreachable!(),
    }
}

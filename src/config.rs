//! Per-setup configuration file (`nascast.toml`). Command-line flags and environment variables
//! override its values; relative paths in it are relative to the file's folder.

use std::fs;
use std::path::{Path, PathBuf};
use serde::Deserialize;
use url::Url;

/// Config file read when `--config` isn't given, if it exists.
pub const DEFAULT_CONFIG_PATH: &str = "nascast.toml";

#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    /// Where `index` writes the site, and what `webserver` serves.
    pub output_folder: Option<PathBuf>,
    pub cache_path: Option<PathBuf>,
    pub base_url: Option<Url>,
    /// `<folder>[:<mount>]` specs, as for `--movies-folder`.
    #[serde(default)]
    pub movies_folders: Vec<String>,
    /// `<folder>[:<mount>]` specs, as for `--tv-folder`.
    #[serde(default)]
    pub tv_folders: Vec<String>,
    #[serde(default)]
    pub omdb: OmdbConfig,
    #[serde(default)]
    pub subtitles: SubtitlesConfig,
    #[serde(default)]
    pub opensubtitles: OpenSubtitlesConfig,
    #[serde(default)]
    pub webserver: WebserverConfig,
}

#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OmdbConfig {
    pub api_key: Option<String>,
}

#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SubtitlesConfig {
    /// Preferred subtitle languages, most preferred first.
    #[serde(default)]
    pub languages: Vec<String>,
}

#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct OpenSubtitlesConfig {
    pub api_key: Option<String>,
    pub username: Option<String>,
    pub password: Option<String>,
}

#[derive(Deserialize, Default, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct WebserverConfig {
    pub bind: Option<String>,
    pub port: Option<u16>,
}

impl Config {
    /// Reads and parses a config file.
    pub fn load(path: &Path) -> Result<Config, String> {
        let text = fs::read_to_string(path).map_err(|e| format!("Can't read config file {}: {}", path.display(), e))?;
        let mut config: Config = toml::from_str(&text).map_err(|e| format!("Invalid config file {}: {}", path.display(), e))?;
        config.resolve_paths(path.parent().unwrap_or(Path::new("")));
        Ok(config)
    }

    /// The config at `path`, or else `nascast.toml` if it exists, or else an empty config.
    pub fn find(path: Option<&Path>) -> Result<Config, String> {
        match path {
            Some(path) => Config::load(path),
            None if Path::new(DEFAULT_CONFIG_PATH).is_file() => Config::load(Path::new(DEFAULT_CONFIG_PATH)),
            None => Ok(Config::default()),
        }
    }

    /// Makes relative paths relative to `base` (the config file's folder) instead of the
    /// working directory.
    fn resolve_paths(&mut self, base: &Path) {
        for path in [&mut self.output_folder, &mut self.cache_path].into_iter().flatten() {
            *path = base.join(&*path);
        }
        for spec in self.movies_folders.iter_mut().chain(self.tv_folders.iter_mut()) {
            let (folder, mount) = match spec.split_once(':') {
                Some((folder, mount)) => (folder, mount.to_string()),
                // Keep the mount as written, not the resolved path
                None => (spec.as_str(), spec.clone()),
            };
            *spec = format!("{}:{}", base.join(folder).display(), mount);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nascast.toml");
        fs::write(&path, r#"
            output_folder = "pub"
            cache_path = "/var/cache/nascast.sqlite"
            base_url = "https://nas.local/media/"
            movies_folders = ["/media/Movies:movies", "More Movies"]

            [omdb]
            api_key = "omdb-key"

            [subtitles]
            languages = ["en", "he"]

            [webserver]
            port = 8080
        "#).unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.output_folder, Some(dir.path().join("pub")));
        assert_eq!(config.cache_path, Some(PathBuf::from("/var/cache/nascast.sqlite")));
        assert_eq!(config.base_url.unwrap().as_str(), "https://nas.local/media/");
        assert_eq!(config.movies_folders, vec![
            "/media/Movies:movies".to_string(),
            format!("{}:More Movies", dir.path().join("More Movies").display()),
        ]);
        assert!(config.tv_folders.is_empty());
        assert_eq!(config.omdb.api_key.as_deref(), Some("omdb-key"));
        assert_eq!(config.subtitles.languages, vec!["en", "he"]);
        assert_eq!(config.opensubtitles, OpenSubtitlesConfig::default());
        assert_eq!(config.webserver, WebserverConfig { bind: None, port: Some(8080) });
    }

    #[test]
    fn test_example_config_parses() {
        let config: Config = toml::from_str(include_str!("../nascast.example.toml")).unwrap();
        assert_eq!(config.subtitles.languages, vec!["en", "he"]);
    }

    #[test]
    fn test_unknown_keys_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nascast.toml");
        fs::write(&path, "[subtitles]\nlanguage = [\"en\"]\n").unwrap();
        let error = Config::load(&path).unwrap_err();
        assert!(error.contains("unknown field `language`"), "{}", error);
    }
}

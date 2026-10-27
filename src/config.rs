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
    /// Media folders, in any combination of types.
    #[serde(default, rename = "folders")]
    folder_entries: Vec<FolderEntry>,
    #[serde(skip)]
    pub folders: Vec<MediaFolder>,
    #[serde(default)]
    pub omdb: OmdbConfig,
    #[serde(default)]
    pub subtitles: SubtitlesConfig,
    #[serde(default)]
    pub opensubtitles: OpenSubtitlesConfig,
    #[serde(default)]
    pub webserver: WebserverConfig,
}

/// What a media folder holds, which decides how it's scanned.
#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MediaKind {
    #[serde(alias = "movie")]
    Movies,
    #[serde(alias = "series")]
    Tv,
}

impl MediaKind {
    fn parse(s: &str) -> Option<MediaKind> {
        match s.to_lowercase().as_str() {
            "movies" | "movie" => Some(MediaKind::Movies),
            "tv" | "series" => Some(MediaKind::Tv),
            _ => None,
        }
    }
}

/// A media folder on disk and the URL path segment it is served under.
#[derive(Debug, PartialEq, Clone)]
pub struct MediaFolder {
    pub kind: MediaKind,
    pub path: PathBuf,
    pub mount: String,
}

impl MediaFolder {
    /// A folder whose mount defaults to the folder's name.
    fn new(kind: MediaKind, path: PathBuf, mount: Option<String>) -> Result<MediaFolder, String> {
        let mount = match mount {
            Some(mount) => mount,
            None => path.file_name().map(|n| n.to_string_lossy().into_owned())
                .ok_or_else(|| format!("Can't derive a mount from folder {}; set one", path.display()))?,
        };
        let mount = mount.trim_matches('/').to_string();
        if mount.is_empty() {
            return Err(format!("Empty mount for folder {}", path.display()));
        }
        Ok(MediaFolder { kind, path, mount })
    }

    /// Parses a `--folder` spec: `<type>:<path>[:<mount>]`, e.g. `tv:/media/TV:tv`.
    pub fn parse(spec: &str) -> Result<MediaFolder, String> {
        let mut parts = spec.splitn(3, ':');
        let (kind, path, mount) = (parts.next().unwrap_or_default(), parts.next(), parts.next());
        let kind = MediaKind::parse(kind)
            .ok_or_else(|| format!("Invalid folder '{}': expected <type>:<path>[:<mount>] with type movies or tv", spec))?;
        let path = path.filter(|p| !p.is_empty()).ok_or_else(|| format!("Invalid folder '{}': missing path", spec))?;
        MediaFolder::new(kind, PathBuf::from(path), mount.map(str::to_string))
    }
}

/// Checks that no two folders are served under the same mount.
pub fn check_mounts(folders: &[MediaFolder]) -> Result<(), String> {
    for (i, folder) in folders.iter().enumerate() {
        if let Some(other) = folders[..i].iter().find(|f| f.mount == folder.mount) {
            return Err(format!("Folders {} and {} are both mounted at '{}'; give one a different mount",
                               other.path.display(), folder.path.display(), folder.mount));
        }
    }
    Ok(())
}

#[derive(Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
struct FolderEntry {
    #[serde(rename = "type")]
    kind: MediaKind,
    path: PathBuf,
    mount: Option<String>,
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
        let base = path.parent().unwrap_or(Path::new(""));
        config.resolve_paths(base);
        config.folders = std::mem::take(&mut config.folder_entries).into_iter()
            .map(|entry| MediaFolder::new(entry.kind, base.join(entry.path), entry.mount))
            .collect::<Result<_, _>>()
            .map_err(|e| format!("Invalid config file {}: {}", path.display(), e))?;
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

            [omdb]
            api_key = "omdb-key"

            [subtitles]
            languages = ["en", "he"]

            [webserver]
            port = 8080

            [[folders]]
            type = "movies"
            path = "/media/Movies"
            mount = "films"

            [[folders]]
            type = "movies"
            path = "More Movies"

            [[folders]]
            type = "tv"
            path = "/media/TV"
        "#).unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.output_folder, Some(dir.path().join("pub")));
        assert_eq!(config.cache_path, Some(PathBuf::from("/var/cache/nascast.sqlite")));
        assert_eq!(config.base_url.unwrap().as_str(), "https://nas.local/media/");
        assert_eq!(config.folders, vec![
            MediaFolder { kind: MediaKind::Movies, path: PathBuf::from("/media/Movies"), mount: "films".into() },
            MediaFolder { kind: MediaKind::Movies, path: dir.path().join("More Movies"), mount: "More Movies".into() },
            MediaFolder { kind: MediaKind::Tv, path: PathBuf::from("/media/TV"), mount: "TV".into() },
        ]);
        assert_eq!(config.omdb.api_key.as_deref(), Some("omdb-key"));
        assert_eq!(config.subtitles.languages, vec!["en", "he"]);
        assert_eq!(config.opensubtitles, OpenSubtitlesConfig::default());
        assert_eq!(config.webserver, WebserverConfig { bind: None, port: Some(8080) });
    }

    #[test]
    fn test_parse_folder() {
        assert_eq!(MediaFolder::parse("tv:/media/TV:shows"), Ok(MediaFolder { kind: MediaKind::Tv, path: "/media/TV".into(), mount: "shows".into() }));
        assert_eq!(MediaFolder::parse("movies:/media/Movies/"), Ok(MediaFolder { kind: MediaKind::Movies, path: "/media/Movies/".into(), mount: "Movies".into() }));
        assert!(MediaFolder::parse("/media/Movies").is_err());
        assert!(MediaFolder::parse("music:/media/Music").is_err());
        assert!(MediaFolder::parse("tv:").is_err());

        let folders = ["movies:/a/Movies", "movies:/b/Movies"].map(|s| MediaFolder::parse(s).unwrap());
        assert!(check_mounts(&folders).unwrap_err().contains("both mounted at 'Movies'"));
        assert_eq!(check_mounts(&folders[..1]), Ok(()));
    }

    #[test]
    fn test_example_config_parses() {
        let config: Config = toml::from_str(include_str!("../nascast.example.toml")).unwrap();
        assert_eq!(config.subtitles.languages, vec!["en", "he"]);
        assert_eq!(config.folder_entries.iter().map(|f| f.kind).collect::<Vec<_>>(), vec![MediaKind::Movies, MediaKind::Movies, MediaKind::Tv]);
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

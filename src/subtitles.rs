//! Subtitle discovery, decoding and SRT -> WebVTT conversion.
//!
//! Browsers and Chromecast only understand WebVTT, so every subtitle is published as a `.vtt`
//! file in the output folder, next to the generated pages.

use std::collections::hash_map::DefaultHasher;
use std::fs;
use std::hash::{Hash, Hasher};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use encoding_rs::Encoding;
use regex::Regex;
use serde::{Deserialize, Serialize};

/// Folder (relative to the output folder) that published subtitles are written to.
pub const SUBTITLES_DIR: &str = "subtitles";

/// Subfolders commonly holding subtitles for the videos next to them.
const SUBTITLE_FOLDERS: &[&str] = &["subs", "subtitles"];

pub struct Language {
    /// ISO 639-1 code, also used as the BCP 47 tag sent to the Chromecast.
    pub code: &'static str,
    pub name: &'static str,
    /// Other spellings found in file names (ISO 639-2 codes, English names).
    aliases: &'static [&'static str],
    /// Legacy encoding most subtitle files in this language use when they aren't UTF-8.
    legacy_encoding: &'static Encoding,
}

macro_rules! language {
    ($code:literal, $name:literal, [$($alias:literal),*], $enc:ident) => {
        Language { code: $code, name: $name, aliases: &[$($alias),*], legacy_encoding: encoding_rs::$enc }
    };
}

pub static LANGUAGES: &[Language] = &[
    language!("en", "English", ["eng", "english"], WINDOWS_1252),
    language!("he", "Hebrew", ["heb", "hebrew", "iw"], WINDOWS_1255),
    language!("ar", "Arabic", ["ara", "arabic"], WINDOWS_1256),
    language!("es", "Spanish", ["spa", "spanish", "esp"], WINDOWS_1252),
    language!("fr", "French", ["fre", "fra", "french"], WINDOWS_1252),
    language!("de", "German", ["ger", "deu", "german"], WINDOWS_1252),
    language!("it", "Italian", ["ita", "italian"], WINDOWS_1252),
    language!("pt", "Portuguese", ["por", "portuguese"], WINDOWS_1252),
    language!("nl", "Dutch", ["dut", "nld", "dutch"], WINDOWS_1252),
    language!("sv", "Swedish", ["swe", "swedish"], WINDOWS_1252),
    language!("no", "Norwegian", ["nor", "nob", "norwegian"], WINDOWS_1252),
    language!("da", "Danish", ["dan", "danish"], WINDOWS_1252),
    language!("fi", "Finnish", ["fin", "finnish"], WINDOWS_1252),
    language!("pl", "Polish", ["pol", "polish"], WINDOWS_1250),
    language!("cs", "Czech", ["cze", "ces", "czech"], WINDOWS_1250),
    language!("hu", "Hungarian", ["hun", "hungarian"], WINDOWS_1250),
    language!("ro", "Romanian", ["rum", "ron", "romanian"], WINDOWS_1250),
    language!("ru", "Russian", ["rus", "russian"], WINDOWS_1251),
    language!("uk", "Ukrainian", ["ukr", "ukrainian"], WINDOWS_1251),
    language!("el", "Greek", ["gre", "ell", "greek"], WINDOWS_1253),
    language!("tr", "Turkish", ["tur", "turkish"], WINDOWS_1254),
    language!("ja", "Japanese", ["jpn", "japanese"], SHIFT_JIS),
    language!("zh", "Chinese", ["chi", "zho", "chinese"], GBK),
    language!("ko", "Korean", ["kor", "korean"], EUC_KR),
];

/// BCP 47 tag for subtitles whose language couldn't be determined.
pub const UNKNOWN_LANGUAGE: &str = "und";

/// Looks up a language by ISO code or English name, case-insensitively.
pub fn find_language(token: &str) -> Option<&'static Language> {
    let token = token.to_lowercase();
    LANGUAGES.iter().find(|l| l.code == token || l.aliases.contains(&token.as_str()))
}

/// Display name for a BCP 47 language tag.
pub fn language_name(code: &str) -> &str {
    find_language(code).map_or(if code == UNKNOWN_LANGUAGE { "Unknown" } else { code }, |l| l.name)
}

/// A subtitle found for a video, before publishing.
#[derive(Debug, PartialEq)]
pub struct SubtitleSource {
    /// Where the subtitle came from; also identifies it for the published file name.
    pub origin: SubtitleOrigin,
    /// BCP 47 tag, or [`UNKNOWN_LANGUAGE`].
    pub language: String,
    pub forced: bool,
    pub hearing_impaired: bool,
}

#[derive(Debug, PartialEq)]
pub enum SubtitleOrigin {
    /// A `.srt` or `.vtt` file on disk.
    File(PathBuf),
}

/// A published subtitle, as used by the templates and the cast sender.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct SubtitleTrack {
    /// BCP 47 tag, or [`UNKNOWN_LANGUAGE`].
    pub language: String,
    pub label: String,
    /// Path of the `.vtt` file relative to the output folder.
    pub url: String,
}

/// A language that some subtitle on a page is available in.
#[derive(Serialize, Debug, PartialEq, Clone)]
pub struct SubtitleLanguage {
    pub code: String,
    pub name: String,
}

/// Distinct languages of `tracks`, sorted by name.
pub fn languages_of<'a>(tracks: impl IntoIterator<Item = &'a SubtitleTrack>) -> Vec<SubtitleLanguage> {
    let mut languages: Vec<SubtitleLanguage> = Vec::new();
    for track in tracks {
        if !languages.iter().any(|l| l.code == track.language) {
            languages.push(SubtitleLanguage { code: track.language.clone(), name: language_name(&track.language).to_string() });
        }
    }
    languages.sort_by(|a, b| a.name.cmp(&b.name));
    languages
}

fn is_subtitle_file(path: &Path) -> bool {
    path.is_file()
        && path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "srt" | "vtt"))
}

fn is_video_file(path: &Path) -> bool {
    path.is_file()
        && path.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|ext| matches!(ext.to_lowercase().as_str(), "mkv" | "mp4" | "avi" | "mov" | "wmv" | "flv" | "webm" | "m4v"))
}

fn list_dir(dir: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok().map(|e| e.path())).collect())
        .unwrap_or_default();
    entries.sort();
    entries
}

/// Parses language and flags from the tags of a subtitle file name, e.g. `en.forced` or `2_English`.
fn parse_tags(tags: &str, path: PathBuf) -> SubtitleSource {
    let tokens: Vec<String> = tags.split(['.', '_', '-', ' ', '[', ']', '(', ')'])
        .filter(|t| !t.is_empty())
        .map(str::to_lowercase)
        .collect();
    SubtitleSource {
        language: tokens.iter().find_map(|t| find_language(t)).map_or(UNKNOWN_LANGUAGE, |l| l.code).to_string(),
        forced: tokens.iter().any(|t| t == "forced"),
        hearing_impaired: tokens.iter().any(|t| matches!(t.as_str(), "sdh" | "hi" | "cc")),
        origin: SubtitleOrigin::File(path),
    }
}

/// The part of a subtitle file name after the video's stem, if the subtitle belongs to it:
/// `Movie.srt` -> `""`, `Movie.en.forced.srt` -> `"en.forced"`.
fn tags_for_stem<'a>(subtitle: &'a Path, video_stem: &str) -> Option<&'a str> {
    let stem = subtitle.file_stem()?.to_str()?;
    if stem == video_stem {
        Some("")
    } else {
        stem.strip_prefix(video_stem)?.strip_prefix('.')
    }
}

/// Finds subtitle files belonging to `video`. Supported layouts:
/// - `Movie.srt`, `Movie.en.srt`, `Movie.en.forced.srt` next to the video
/// - the same names inside a `Subs`/`Subtitles` folder next to the video
/// - `Subs/<video stem>/*.srt` (e.g. `Subs/Show.S01E01/2_English.srt`)
/// - any subtitle in `Subs/` when the video is the only one in its folder (movie folders)
pub fn find_subtitles(video: &Path) -> Vec<SubtitleSource> {
    let (Some(dir), Some(video_stem)) = (video.parent(), video.file_stem().and_then(|s| s.to_str())) else {
        return Vec::new();
    };
    let dir_entries = list_dir(dir);
    let only_video = dir_entries.iter().filter(|p| is_video_file(p)).count() == 1;
    let subtitle_folders = dir_entries.iter().filter(|p| {
        p.is_dir() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| SUBTITLE_FOLDERS.contains(&n.to_lowercase().as_str()))
    });

    let mut found = Vec::new();
    let mut add_matching = |entries: &[PathBuf]| {
        for path in entries.iter().filter(|p| is_subtitle_file(p)) {
            if let Some(tags) = tags_for_stem(path, video_stem) {
                found.push(parse_tags(tags, path.clone()));
            }
        }
    };
    add_matching(&dir_entries);

    let mut subtitle_folders_found = Vec::new();
    for folder in subtitle_folders {
        let entries = list_dir(folder);
        add_matching(&entries);
        subtitle_folders_found.push((folder.clone(), entries));
    }
    for (folder, entries) in subtitle_folders_found {
        let per_video = folder.join(video_stem);
        if per_video.is_dir() {
            for path in list_dir(&per_video).into_iter().filter(|p| is_subtitle_file(p)) {
                let tags = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
                found.push(parse_tags(&tags, path));
            }
        } else if only_video {
            for path in entries.into_iter().filter(|p| is_subtitle_file(p) && tags_for_stem(p, video_stem).is_none()) {
                let tags = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
                found.push(parse_tags(&tags, path));
            }
        }
    }
    found
}

/// Decodes subtitle bytes to text: UTF-8 or UTF-16 with BOM, else UTF-8, else the
/// language's legacy encoding (e.g. windows-1255 for Hebrew).
pub fn decode_subtitle(bytes: &[u8], language: &str) -> String {
    if let Some((encoding, bom_len)) = Encoding::for_bom(bytes) {
        return encoding.decode_without_bom_handling(&bytes[bom_len..]).0.into_owned();
    }
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => {
            let encoding = find_language(language).map_or(encoding_rs::WINDOWS_1252, |l| l.legacy_encoding);
            encoding.decode_without_bom_handling(bytes).0.into_owned()
        }
    }
}

static SRT_TIMING_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\s*(\d+):(\d{1,2}):(\d{1,2})[,.](\d{1,3})\s*-->\s*(\d+):(\d{1,2}):(\d{1,2})[,.](\d{1,3})").unwrap()
});
/// ASS-style override tags (`{\an8}`) and `<font>` tags, which WebVTT doesn't support.
static UNSUPPORTED_TAGS_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\\[^}]*\}|</?font[^>]*>").unwrap());

fn vtt_timestamp(h: &str, m: &str, s: &str, ms: &str) -> String {
    format!("{:0>2}:{:0>2}:{:0>2}.{:0<3}", h, m, s, ms)
}

/// Converts SRT text to WebVTT. Cue numbers are kept (they are valid cue identifiers).
pub fn srt_to_vtt(srt: &str) -> String {
    let mut vtt = String::from("WEBVTT\n\n");
    for line in srt.trim_start_matches('\u{feff}').lines() {
        match SRT_TIMING_RE.captures(line) {
            Some(c) => {
                vtt.push_str(&vtt_timestamp(&c[1], &c[2], &c[3], &c[4]));
                vtt.push_str(" --> ");
                vtt.push_str(&vtt_timestamp(&c[5], &c[6], &c[7], &c[8]));
            }
            None => vtt.push_str(&UNSUPPORTED_TAGS_RE.replace_all(line.trim_end(), "")),
        }
        vtt.push('\n');
    }
    vtt
}

/// Normalizes a WebVTT file's text: strips the BOM and unifies line endings.
fn normalize_vtt(vtt: &str) -> String {
    vtt.trim_start_matches('\u{feff}').lines().collect::<Vec<_>>().join("\n") + "\n"
}

/// Reads a subtitle source and returns its content as WebVTT.
pub fn to_vtt(source: &SubtitleSource) -> io::Result<String> {
    match &source.origin {
        SubtitleOrigin::File(path) => {
            let text = decode_subtitle(&fs::read(path)?, &source.language);
            let is_vtt = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("vtt"));
            Ok(if is_vtt { normalize_vtt(&text) } else { srt_to_vtt(&text) })
        }
    }
}

fn source_id(source: &SubtitleSource) -> u64 {
    let mut hasher = DefaultHasher::new();
    match &source.origin {
        SubtitleOrigin::File(path) => path.to_str().hash(&mut hasher),
    }
    hasher.finish()
}

fn label(source: &SubtitleSource) -> String {
    let mut label = language_name(&source.language).to_string();
    if source.forced {
        label.push_str(" (Forced)");
    }
    if source.hearing_impaired {
        label.push_str(" (SDH)");
    }
    label
}

/// Converts `sources` to WebVTT files under `<output_dir>/subtitles/` and returns the tracks,
/// sorted by label. Unreadable subtitles are logged and skipped.
pub fn publish(sources: &[SubtitleSource], output_dir: &Path) -> Vec<SubtitleTrack> {
    let mut tracks: Vec<SubtitleTrack> = Vec::new();
    for source in sources {
        let vtt = match to_vtt(source) {
            Ok(vtt) => vtt,
            Err(e) => {
                log::warn!(target: "cli", "Failed to read subtitle {:?}: {}", source.origin, e);
                continue;
            }
        };
        let url = format!("{}/{}.{}.vtt", SUBTITLES_DIR, source_id(source), source.language);
        if let Err(e) = fs::create_dir_all(output_dir.join(SUBTITLES_DIR)).and_then(|_| fs::write(output_dir.join(&url), vtt)) {
            log::error!(target: "cli", "Failed to write subtitle {}: {}", url, e);
            continue;
        }
        tracks.push(SubtitleTrack { language: source.language.clone(), label: label(source), url });
    }
    tracks.sort_by(|a, b| (&a.label, &a.url).cmp(&(&b.label, &b.url)));
    // Disambiguate repeated labels: "English", "English 2", ...
    let labels: Vec<String> = tracks.iter().map(|t| t.label.clone()).collect();
    for (i, track) in tracks.iter_mut().enumerate() {
        let earlier = labels[..i].iter().filter(|l| **l == labels[i]).count();
        if earlier > 0 {
            track.label = format!("{} {}", labels[i], earlier + 1);
        }
    }
    tracks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_srt_to_vtt() {
        let srt = "\u{feff}1\r\n00:00:01,500 --> 00:00:03,000\r\n{\\an8}<i>Hello</i> <font color=\"#ff0\">world</font>\r\n\r\n2\r\n0:01:02,5 --> 0:01:04,25 X1:10 X2:20\r\nSecond line\r\n";
        assert_eq!(srt_to_vtt(srt), "WEBVTT\n\n1\n00:00:01.500 --> 00:00:03.000\n<i>Hello</i> world\n\n2\n00:01:02.500 --> 00:01:04.250\nSecond line\n");
    }

    #[test]
    fn test_decode_subtitle() {
        // "שלום" in windows-1255
        assert_eq!(decode_subtitle(&[0xf9, 0xec, 0xe5, 0xed], "he"), "שלום");
        assert_eq!(decode_subtitle("שלום".as_bytes(), "he"), "שלום");
        assert_eq!(decode_subtitle(b"\xef\xbb\xbfhi", "en"), "hi");
        assert_eq!(decode_subtitle(&[0xff, 0xfe, b'h', 0, b'i', 0], "en"), "hi");
        // "café" in windows-1252
        assert_eq!(decode_subtitle(b"caf\xe9", "en"), "café");
    }

    #[test]
    fn test_find_subtitles() {
        let dir = tempfile::tempdir().unwrap();
        let movie_dir = dir.path().join("Movie (2020)");
        fs::create_dir_all(movie_dir.join("Subs")).unwrap();
        for f in ["Movie.2020.mp4", "Movie.2020.srt", "Movie.2020.he.srt", "Movie.2020.eng.forced.srt",
                  "Other.srt", "Subs/2_English.srt", "Subs/3_Hebrew.SDH.srt", "Subs/notes.txt"] {
            fs::write(movie_dir.join(f), "").unwrap();
        }
        let found = find_subtitles(&movie_dir.join("Movie.2020.mp4"));
        let summary: Vec<_> = found.iter().map(|s| {
            let SubtitleOrigin::File(p) = &s.origin;
            (p.strip_prefix(&movie_dir).unwrap().to_str().unwrap().to_string(), s.language.as_str(), s.forced, s.hearing_impaired)
        }).collect();
        assert_eq!(summary, vec![
            ("Movie.2020.eng.forced.srt".to_string(), "en", true, false),
            ("Movie.2020.he.srt".to_string(), "he", false, false),
            ("Movie.2020.srt".to_string(), "und", false, false),
            ("Subs/2_English.srt".to_string(), "en", false, false),
            ("Subs/3_Hebrew.SDH.srt".to_string(), "he", false, true),
        ]);
    }

    #[test]
    fn test_find_subtitles_tv_layout() {
        let dir = tempfile::tempdir().unwrap();
        let season = dir.path().join("Season 01");
        fs::create_dir_all(season.join("Subs/Show.S01E01")).unwrap();
        for f in ["Show.S01E01.mkv", "Show.S01E02.mkv", "Show.S01E02.en.srt", "Subs/Show.S01E01/2_English.srt", "Subs/stray.srt"] {
            fs::write(season.join(f), "").unwrap();
        }
        // Episode folders hold several videos, so loose files in Subs/ aren't attributed to any of them
        let ep1: Vec<_> = find_subtitles(&season.join("Show.S01E01.mkv")).into_iter().map(|s| s.language).collect();
        assert_eq!(ep1, vec!["en"]);
        let ep2 = find_subtitles(&season.join("Show.S01E02.mkv"));
        assert_eq!(ep2.len(), 1);
        assert_eq!(ep2[0].origin, SubtitleOrigin::File(season.join("Show.S01E02.en.srt")));
    }

    #[test]
    fn test_publish() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a.en.srt"), dir.path().join("b.en.srt"));
        fs::write(&a, "1\n00:00:01,000 --> 00:00:02,000\nHi\n").unwrap();
        fs::write(&b, "1\n00:00:01,000 --> 00:00:02,000\nHi\n").unwrap();
        let out = dir.path().join("out");
        let sources = [a, b].map(|p| SubtitleSource { origin: SubtitleOrigin::File(p), language: "en".into(), forced: false, hearing_impaired: false });
        let tracks = publish(&sources, &out);
        assert_eq!(tracks.iter().map(|t| t.label.as_str()).collect::<Vec<_>>(), vec!["English", "English 2"]);
        assert!(fs::read_to_string(out.join(&tracks[0].url)).unwrap().starts_with("WEBVTT\n\n1\n00:00:01.000 --> 00:00:02.000\nHi\n"));
    }
}

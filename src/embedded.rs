//! Subtitle streams embedded in video files (typically MKV), extracted with ffprobe/ffmpeg.
//!
//! Extraction reads the whole video, so results are cached per file, keyed by its size and
//! modification time. Image-based subtitles (PGS, VobSub) can't become WebVTT and are skipped.

use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::time::UNIX_EPOCH;
use serde::{Deserialize, Serialize};

use crate::cache::MediaCache;
use crate::subtitles::{self, SubtitleOrigin, SubtitleSource, UNKNOWN_LANGUAGE};

/// Subtitle codecs ffmpeg can convert to WebVTT.
const TEXT_CODECS: &[&str] = &["subrip", "srt", "ass", "ssa", "mov_text", "webvtt", "text"];

/// An embedded subtitle stream, as cached.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct EmbeddedSubtitle {
    /// Stream index within the file.
    pub index: u32,
    /// BCP 47 tag, the stream's own language tag if it isn't a known language, or [`UNKNOWN_LANGUAGE`].
    pub language: String,
    pub forced: bool,
    pub hearing_impaired: bool,
    /// WebVTT content.
    pub content: String,
}

#[derive(Deserialize)]
struct ProbeOutput {
    #[serde(default)]
    streams: Vec<ProbeStream>,
}

#[derive(Deserialize)]
struct ProbeStream {
    index: u32,
    #[serde(default)]
    codec_name: String,
    #[serde(default)]
    tags: ProbeTags,
    #[serde(default)]
    disposition: ProbeDisposition,
}

#[derive(Deserialize, Default)]
struct ProbeTags {
    language: Option<String>,
    title: Option<String>,
}

#[derive(Deserialize, Default)]
struct ProbeDisposition {
    #[serde(default)]
    forced: u8,
    #[serde(default)]
    hearing_impaired: u8,
}

fn probe(video: &Path) -> io::Result<Vec<ProbeStream>> {
    let output = Command::new("ffprobe")
        .args(["-v", "error", "-select_streams", "s", "-show_entries",
               "stream=index,codec_name:stream_tags=language,title:stream_disposition=forced,hearing_impaired",
               "-of", "json"])
        .arg(video)
        .output()?;
    if !output.status.success() {
        return Err(io::Error::other(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    let parsed: ProbeOutput = serde_json::from_slice(&output.stdout).map_err(io::Error::other)?;
    Ok(parsed.streams)
}

/// Converts the given streams of `video` to WebVTT in a single ffmpeg pass.
fn extract(video: &Path, streams: &[&ProbeStream]) -> io::Result<Vec<String>> {
    let dir = tempfile::tempdir()?;
    let mut command = Command::new("ffmpeg");
    command.args(["-v", "error", "-nostdin", "-y", "-i"]).arg(video);
    for stream in streams {
        command.args(["-map", &format!("0:{}", stream.index), "-c:s", "webvtt", "-f", "webvtt"])
            .arg(dir.path().join(format!("{}.vtt", stream.index)));
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(io::Error::other(String::from_utf8_lossy(&output.stderr).trim().to_string()));
    }
    streams.iter()
        .map(|stream| fs::read(dir.path().join(format!("{}.vtt", stream.index))).map(|b| String::from_utf8_lossy(&b).into_owned()))
        .collect()
}

fn has_tag(title: &Option<String>, words: &[&str]) -> bool {
    title.as_deref().is_some_and(|t| {
        t.to_lowercase().split(|c: char| !c.is_alphanumeric()).any(|w| words.contains(&w))
    })
}

fn language_of(tag: Option<&str>) -> String {
    match tag.map(str::trim).filter(|t| !t.is_empty() && *t != UNKNOWN_LANGUAGE) {
        Some(tag) => subtitles::find_language(tag).map_or_else(|| tag.to_lowercase(), |l| l.code.to_string()),
        None => UNKNOWN_LANGUAGE.to_string(),
    }
}

/// Probes and extracts the text subtitle streams of `video`.
pub fn read_embedded(video: &Path) -> io::Result<Vec<EmbeddedSubtitle>> {
    let streams = probe(video)?;
    let (text, image): (Vec<&ProbeStream>, Vec<&ProbeStream>) = streams.iter().partition(|s| TEXT_CODECS.contains(&s.codec_name.as_str()));
    for stream in image {
        log::info!(target: "cli", "Skipping image-based subtitle stream {} ({}) in {:?}", stream.index, stream.codec_name, video);
    }
    if text.is_empty() {
        return Ok(Vec::new());
    }
    log::info!(target: "cli", "Extracting {} embedded subtitle stream(s) from {:?}", text.len(), video);
    let contents = extract(video, &text)?;
    Ok(text.into_iter().zip(contents).map(|(stream, content)| EmbeddedSubtitle {
        index: stream.index,
        language: language_of(stream.tags.language.as_deref()),
        forced: stream.disposition.forced != 0 || has_tag(&stream.tags.title, &["forced"]),
        hearing_impaired: stream.disposition.hearing_impaired != 0 || has_tag(&stream.tags.title, &["sdh", "cc", "hi"]),
        content,
    }).collect())
}

/// Identifies a version of a file, so cached results are dropped when it changes.
fn file_version(path: &Path) -> Option<(u64, i64)> {
    let metadata = fs::metadata(path).ok()?;
    let mtime = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?.as_secs() as i64;
    Some((metadata.len(), mtime))
}

/// Finds embedded subtitles, remembering them in the cache. Disables itself (with a warning)
/// when ffprobe isn't installed.
pub struct EmbeddedSubtitles<'a> {
    cache: Option<&'a MediaCache>,
    available: bool,
}

impl<'a> EmbeddedSubtitles<'a> {
    pub fn new(cache: Option<&'a MediaCache>) -> Self {
        let available = Command::new("ffprobe").arg("-version").output().is_ok_and(|o| o.status.success());
        if !available {
            log::warn!(target: "cli", "ffprobe/ffmpeg not found; embedded subtitles won't be used");
        }
        EmbeddedSubtitles { cache, available }
    }

    pub fn find(&self, video: &Path) -> Vec<SubtitleSource> {
        if !self.available {
            return Vec::new();
        }
        let Some((size, mtime)) = file_version(video) else {
            return Vec::new();
        };
        let cached = self.cache.and_then(|cache| match cache.get_embedded_subtitles(video, size, mtime) {
            Ok(cached) => cached,
            Err(e) => {
                log::warn!(target: "cli", "Failed to read embedded subtitle cache for {:?}: {}", video, e);
                None
            }
        });
        let embedded = match cached {
            Some(embedded) => embedded,
            None => match read_embedded(video) {
                Ok(embedded) => {
                    if let Some(cache) = self.cache {
                        if let Err(e) = cache.store_embedded_subtitles(video, size, mtime, &embedded) {
                            log::error!(target: "cli", "Failed to cache embedded subtitles for {:?}: {}", video, e);
                        }
                    }
                    embedded
                }
                Err(e) => {
                    // Not cached: may be a partially copied file
                    log::warn!(target: "cli", "Failed to read embedded subtitles from {:?}: {}", video, e);
                    Vec::new()
                }
            },
        };
        embedded.into_iter().map(|e| SubtitleSource {
            origin: SubtitleOrigin::Embedded { media_path: video.to_path_buf(), stream_index: e.index, content: e.content },
            language: e.language,
            forced: e.forced,
            hearing_impaired: e.hearing_impaired,
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ffmpeg_available() -> bool {
        Command::new("ffmpeg").arg("-version").output().is_ok_and(|o| o.status.success())
    }

    #[test]
    fn test_language_of() {
        assert_eq!(language_of(Some("eng")), "en");
        assert_eq!(language_of(Some("heb")), "he");
        assert_eq!(language_of(Some("und")), UNKNOWN_LANGUAGE);
        assert_eq!(language_of(None), UNKNOWN_LANGUAGE);
        assert_eq!(language_of(Some("tlh")), "tlh");
    }

    #[test]
    fn test_read_embedded() {
        if !ffmpeg_available() {
            eprintln!("ffmpeg not installed; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let (en, he) = (dir.path().join("en.srt"), dir.path().join("he.srt"));
        fs::write(&en, "1\n00:00:00,100 --> 00:00:00,900\nHello\n").unwrap();
        fs::write(&he, "1\n00:00:00,100 --> 00:00:00,900\nשלום\n").unwrap();
        let video = dir.path().join("video.mkv");
        let status = Command::new("ffmpeg")
            .args(["-v", "error", "-f", "lavfi", "-i", "color=size=16x16:duration=1", "-i"]).arg(&en).arg("-i").arg(&he)
            .args(["-map", "0", "-map", "1", "-map", "2", "-c:v", "mpeg4", "-c:s", "srt",
                   "-metadata:s:s:0", "language=eng", "-metadata:s:s:1", "language=heb", "-disposition:s:1", "forced"])
            .arg(&video)
            .status().unwrap();
        assert!(status.success());

        let embedded = read_embedded(&video).unwrap();
        let summary: Vec<_> = embedded.iter().map(|e| (e.language.as_str(), e.forced, e.content.starts_with("WEBVTT"))).collect();
        assert_eq!(summary, vec![("en", false, true), ("he", true, true)]);
        assert!(embedded[1].content.contains("שלום"));

        // Cached per file version
        let cache = MediaCache::new(&dir.path().join("cache.sqlite")).unwrap();
        let finder = EmbeddedSubtitles::new(Some(&cache));
        assert_eq!(finder.find(&video).len(), 2);
        let (size, mtime) = file_version(&video).unwrap();
        assert_eq!(cache.get_embedded_subtitles(&video, size, mtime).unwrap(), Some(embedded));
        assert_eq!(cache.get_embedded_subtitles(&video, size + 1, mtime).unwrap(), None);
    }
}

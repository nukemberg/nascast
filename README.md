# NASCast

NASCast is a minimalistic media center; it's a static page generator and the UI is meant to be viewed on smartphone/laptop/tablet and Google Cast to TV. NASCast is to Kodi what Hugo is to Wordpress.

## Features

- 📱 Responsive design for mobile, tablet, and desktop
- 📺 Google Cast support for streaming to TV
- 🎬 Movie and TV show organization
- 🔍 Fast search functionality
- 📊 OMDB integration for movie/show metadata
- 💬 Subtitles from `.srt`/`.vtt` files next to your videos, embedded in them, or downloaded from OpenSubtitles
- 🚀 Static site generation for fast loading
- 💾 SQLite caching for improved performance

## Usage

Your media library and generated static files should be exposed on HTTP/HTTPS. Many NAS boxes have a built-in webserver (Or just run Nginx).

```bash
nascast --movies-folder ~/Movies/Movies:movies \
    --tv-folder ~/Movies/TV:tv \
    --omdb-api-key YOUR_OMDB_API_KEY
```

### Command Line Options

- `--movies-folder`: Path to your movies folder (format: path:mount_point)
- `--tv-folder`: Path to your TV shows folder (format: path:mount_point)
- `--omdb-api-key`: Your OMDB API key for fetching movie/show metadata
- `--base-url`: (Optional) Base URL for serving the static files
- `--cache-path`: (Optional) Path to SQLite cache file (default: ./nascast_cache.sqlite)
- `--noop`: (Optional) Run in no-op mode (only show metadata, don't write files)
- `--verbosity`: (Optional) Set logging verbosity level
- `--subtitle-languages`: (Optional) Preferred subtitle languages, most preferred first, e.g. `en,he`: pages turn the first available one on, and missing ones are downloaded
- `--opensubtitles-api-key`: (Optional) OpenSubtitles API key; enables subtitle downloads (env: `OPENSUBTITLES_API_KEY`)
- `--opensubtitles-username`, `--opensubtitles-password`: (Optional) OpenSubtitles account, for a higher daily download quota (env: `OPENSUBTITLES_USERNAME`, `OPENSUBTITLES_PASSWORD`)

## Subtitles

NASCast picks up `.srt` and `.vtt` files that belong to a video and publishes them as WebVTT (the only format browsers and Chromecast read) under `subtitles/` in the output folder. Supported layouts:

- `Movie.srt`, `Movie.en.srt`, `Movie.en.forced.srt`, `Movie.heb.sdh.srt` next to `Movie.mp4`
- the same names inside a `Subs` or `Subtitles` folder next to the video
- `Subs/<video name>/2_English.srt` (one folder per episode)
- any subtitle in `Subs/` when the video is the only one in its folder (typical movie folders)

Text subtitles embedded in the video files themselves (common in MKVs) are extracted too, if `ffmpeg`/`ffprobe` are installed; their language comes from the stream's language tag. Extraction reads the whole file, so the result is cached until the file changes. Image-based subtitles (PGS, VobSub) can't be converted and are skipped.

For files, the language comes from an ISO code (`en`, `eng`) or English name (`English`) in the file name. Files that aren't UTF-8 are decoded with the language's usual legacy encoding (e.g. windows-1255 for Hebrew).

### Choosing subtitles

`--subtitle-languages en,he` lists the languages you want subtitles in, most preferred first. Pages turn on the first one a video has (English if there is one, else Hebrew); a language picked on a page instead is remembered in that browser, including "Off".

When a language has several subtitles, full ones come first, then SDH (for the hearing impaired), then forced ones (which only translate foreign-language parts); casting turns on the first. Forced subtitles alone don't count as having the language, so a full one is downloaded too.

### Downloading missing subtitles

With `--subtitle-languages` and an OpenSubtitles API key (create one at https://www.opensubtitles.com/consumers), `index` downloads subtitles in every listed language a video has no full subtitle for. Results are matched by IMDb id (from OMDB) and preferably by the file's hash, so they're synced to that exact release. Downloads, and misses, are remembered in the cache: unchanged media makes no API calls on later runs, and misses are retried after a week.

OpenSubtitles limits downloads per day (more when logged in), so a large library fills in over several runs.

### Casting with subtitles

Pages show a **Cast to TV** button (in Chrome, once a Chromecast is found) and a subtitle language picker; the chosen language is remembered across pages and can be changed while casting.

The Chromecast fetches the video and subtitle files itself, so:

- it must be able to reach the server: use `--base-url` with an address the Chromecast can reach, or run `nascast webserver --bind 0.0.0.0`
- subtitle responses need a CORS header. `nascast webserver` sends one; with another web server add it yourself, e.g. for nginx:

  ```nginx
  location /subtitles/ {
      add_header Access-Control-Allow-Origin *;
  }
  ```

## License

MIT License - see the [LICENSE](LICENSE) file for details
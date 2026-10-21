# NASCast

NASCast is a minimalistic media center; it's a static page generator and the UI is meant to be viewed on smartphone/laptop/tablet and Google Cast to TV. NASCast is to Kodi what Hugo is to Wordpress.

## Features

- 📱 Responsive design for mobile, tablet, and desktop
- 📺 Google Cast support for streaming to TV
- 🎬 Movie and TV show organization
- 🔍 Fast search functionality
- 📊 OMDB integration for movie/show metadata
- 💬 Subtitles from `.srt`/`.vtt` files next to your videos
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

## Subtitles

NASCast picks up `.srt` and `.vtt` files that belong to a video and publishes them as WebVTT (the only format browsers and Chromecast read) under `subtitles/` in the output folder. Supported layouts:

- `Movie.srt`, `Movie.en.srt`, `Movie.en.forced.srt`, `Movie.heb.sdh.srt` next to `Movie.mp4`
- the same names inside a `Subs` or `Subtitles` folder next to the video
- `Subs/<video name>/2_English.srt` (one folder per episode)
- any subtitle in `Subs/` when the video is the only one in its folder (typical movie folders)

The language comes from an ISO code (`en`, `eng`) or English name (`English`) in the file name. Files that aren't UTF-8 are decoded with the language's usual legacy encoding (e.g. windows-1255 for Hebrew).

## License

MIT License - see the [LICENSE](LICENSE) file for details
// Google Cast sender, built on Castjs.
//
// Elements with a `data-cast` attribute cast their media when clicked:
//   data-src        media URL
//   data-title      title shown on the TV
//   data-poster     poster image URL
//   data-subtitles  JSON list of {language, label, url} subtitle tracks
// A `data-subtitle-language` <select> chooses the subtitle language (empty value = off); the
// choice is remembered across pages. URLs are made absolute because the Chromecast fetches
// media and subtitles itself (subtitle responses also need CORS headers).

const cjs = new Castjs();
const SUBTITLE_LANGUAGE_KEY = 'nascast.subtitleLanguage';

// Subtitle tracks of the media currently being cast, in the order sent to the Chromecast
let castTracks = [];

function absoluteUrl(url) {
    return url ? new URL(url, document.baseURI).href : '';
}

function loadLanguagePreference() {
    try {
        return localStorage.getItem(SUBTITLE_LANGUAGE_KEY) || '';
    } catch (e) {
        return '';
    }
}

function saveLanguagePreference(language) {
    try {
        localStorage.setItem(SUBTITLE_LANGUAGE_KEY, language);
    } catch (e) {
        // Storage unavailable (private mode etc.); the choice just isn't remembered
    }
}

function selectedLanguage() {
    const select = document.querySelector('[data-subtitle-language]');
    return select ? select.value : '';
}

function castFromElement(el) {
    let tracks = [];
    try {
        tracks = JSON.parse(el.dataset.subtitles || '[]');
    } catch (e) {
        console.log('Invalid subtitle data', e);
    }
    castTracks = tracks.map(t => ({ language: t.language, label: t.label, src: absoluteUrl(t.url) }));
    const active = castTracks.findIndex(t => t.language === selectedLanguage());

    cjs.cast(absoluteUrl(el.dataset.src), {
        title: el.dataset.title || '',
        description: '',
        poster: absoluteUrl(el.dataset.poster),
        // Always pass the list: Castjs keeps the previous media's subtitles otherwise
        subtitles: castTracks.map((t, i) => ({ label: t.label, src: t.src, active: i === active })),
        time: 0,
        paused: false,
    });
}

function disableSubtitles() {
    const session = cast.framework.CastContext.getInstance().getCurrentSession();
    const media = session && session.getSessionObj().media[0];
    if (media) {
        media.editTracksInfo(new chrome.cast.media.EditTracksInfoRequest([]), () => {}, err => console.log('Failed to disable subtitles', err));
    }
}

function applySubtitleLanguage(language) {
    saveLanguagePreference(language);
    if (!cjs.connected || !castTracks.length) {
        return;
    }
    const index = castTracks.findIndex(t => t.language === language);
    if (index >= 0) {
        cjs.subtitle(index);
    } else {
        disableSubtitles();
    }
}

function showCastStatus() {
    const text = cjs.connected ? `Casting to ${cjs.device} (${cjs.state})` : '';
    document.querySelectorAll('[data-cast-status]').forEach(el => { el.textContent = text; });
}

document.addEventListener('click', event => {
    const el = event.target.closest('[data-cast]');
    if (el) {
        event.preventDefault();
        castFromElement(el);
    }
});

document.querySelectorAll('[data-subtitle-language]').forEach(select => {
    const preferred = loadLanguagePreference();
    if ([...select.options].some(o => o.value === preferred)) {
        select.value = preferred;
    }
    select.addEventListener('change', () => applySubtitleLanguage(select.value));
});

cjs.on('available', () => document.body.classList.add('cast-available'));
cjs.on('statechange', showCastStatus);
cjs.on('error', err => console.log('Cast error', err));

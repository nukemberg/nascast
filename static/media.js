const contentType = 'video/mp4';

async function playMedia(mediaURL) {
    const castSession = cast.framework.CastContext.getInstance().getCurrentSession();
    const mediaInfo = new chrome.cast.media.MediaInfo(mediaURL, contentType);
    const request = new chrome.cast.media.LoadRequest(mediaInfo);
    request.autoplay = true;

    console.log('Playing media: ' + mediaURL);
    try {
        await castSession.loadMedia(request);
        console.log('Load succeed');
        const player = new cast.framework.RemotePlayer();
        const playerController = new cast.framework.RemotePlayerController(player);
    } catch(errorCode) {
        console.log('Error code: ' + errorCode);
    }
    
}
    
const cjs = new Castjs();

function init(mediaURL) {    
    cjs.on('statechange', function(event) {
        console.log('State change')
        switch(event.sessionState) {
            case cast.framework.sessionState.SESSION_STARTED:
                playMedia(mediaURL);
                break;
        }
    });
}
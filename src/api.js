/**
 * window.api — same surface as Electron preload, backed by Tauri invoke/events.
 * Requires withGlobalTauri: true in tauri.conf.json.
 */
(function initSpiderApi(root) {
    const core = root.__TAURI__?.core;
    const eventApi = root.__TAURI__?.event;
    if (!core || !eventApi) {
        console.error('Tauri API not available (withGlobalTauri?)');
        return;
    }

    const { invoke } = core;
    const { listen } = eventApi;

    const listeners = {
        'spider-result': [],
        'spider-results-batch': [],
        'spider-end': [],
        'spider-progress': [],
        'spider-referrers-update': [],
        'session-dump-request-save': [],
        'session-dump-loaded': [],
        'about-show': [],
    };

    Object.keys(listeners).forEach((channel) => {
        listen(channel, (event) => {
            const payload = event.payload;
            listeners[channel].forEach((cb) => {
                try {
                    cb(payload);
                } catch (err) {
                    console.error(`api listener ${channel}:`, err);
                }
            });
        }).catch((err) => console.error(`listen ${channel}:`, err));
    });

    function on(channel, callback) {
        if (listeners[channel] && typeof callback === 'function') {
            listeners[channel].push(callback);
        }
    }

    root.api = {
        startSpider: (startUrl, options = {}) => {
            invoke('start_spider', { startUrl, options }).catch((err) => {
                console.error('start_spider:', err);
            });
        },
        pauseSpider: () => invoke('spider_pause'),
        resumeSpider: () => invoke('spider_resume'),
        stopSpider: () => {
            invoke('spider_stop').catch((err) => console.error('spider_stop:', err));
        },
        openExternal: (url) => invoke('open_external', { url }),
        getAboutInfo: () => invoke('get_about'),
        getSettings: () => invoke('settings_get'),
        saveSettings: (settings) => invoke('settings_save', { settings }),
        saveSessionDump: (payload) => invoke('session_save', { payload }),
        saveSessionDumpJson: ({ startUrl, dumpJson } = {}) =>
            invoke('session_save_json', { startUrl, dumpJson }),
        loadSessionDump: () => invoke('session_load'),
        onSpiderResult: (cb) => on('spider-result', cb),
        onSpiderResultsBatch: (cb) => on('spider-results-batch', cb),
        onSpiderEnd: (cb) => on('spider-end', cb),
        onSpiderProgress: (cb) => on('spider-progress', cb),
        onSpiderReferrersUpdate: (cb) => on('spider-referrers-update', cb),
        onSessionDumpRequestSave: (cb) => on('session-dump-request-save', cb),
        onSessionDumpLoaded: (cb) => on('session-dump-loaded', cb),
        onAboutShow: (cb) => on('about-show', cb),
    };
})(window);

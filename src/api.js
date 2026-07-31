/**
 * window.api — same surface as Electron preload, backed by Tauri invoke/events.
 * Requires withGlobalTauri: true in tauri.conf.json.
 */
(function initSpiderApi(root) {
    const core = root.__TAURI__?.core;
    const eventApi = root.__TAURI__?.event;
    const fsApi = root.__TAURI__?.fs;
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

    /** Read dump body from disk after session_load returns path-only (avoids IPC dumpJson peak). */
    async function readSessionDumpText(filePath) {
        if (!filePath) {
            throw new Error('Немає шляху до файлу дампу.');
        }
        if (fsApi?.readTextFile) {
            return fsApi.readTextFile(filePath);
        }
        // Vanilla fallback when plugin guest JS is not mounted on __TAURI__.fs
        const arr = await invoke('plugin:fs|read_text_file', { path: filePath, options: null });
        const bytes = arr instanceof ArrayBuffer ? new Uint8Array(arr) : Uint8Array.from(arr);
        return new TextDecoder('utf-8').decode(bytes);
    }

    function emitLocal(channel, payload) {
        const list = listeners[channel];
        if (!list) {
            return;
        }
        list.forEach((cb) => {
            try {
                cb(payload);
            } catch (err) {
                console.error(`api emitLocal ${channel}:`, err);
            }
        });
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
        readSessionDumpText,
        showAbout: () => emitLocal('about-show'),
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

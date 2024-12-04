use crate::auth::Credentials;
use crate::cmd::CommandChain;
use crate::handler::http::HttpRequest;
use case_insensitive_string::CaseInsensitiveString;
use chromiumoxide_cdp::cdp::browser_protocol::fetch::{
    self, AuthChallengeResponse, AuthChallengeResponseResponse, ContinueRequestParams,
    ContinueWithAuthParams, DisableParams, EventAuthRequired, EventRequestPaused, RequestPattern,
};
use chromiumoxide_cdp::cdp::browser_protocol::network::ResourceType;
use chromiumoxide_cdp::cdp::browser_protocol::network::{
    EmulateNetworkConditionsParams, EventLoadingFailed, EventLoadingFinished,
    EventRequestServedFromCache, EventRequestWillBeSent, EventResponseReceived, Headers,
    InterceptionId, RequestId, Response, SetCacheDisabledParams, SetExtraHttpHeadersParams,
};
use chromiumoxide_cdp::cdp::browser_protocol::{
    network::EnableParams, security::SetIgnoreCertificateErrorsParams,
};
use chromiumoxide_types::{Command, Method, MethodId};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

lazy_static::lazy_static! {
    /// allowed js frameworks and libs excluding some and adding additional URLs
    pub static ref JS_FRAMEWORK_ALLOW: phf::Set<&'static str> = {
        phf::phf_set! {
            // Add allowed assets from JS_FRAMEWORK_ASSETS except the excluded ones
            "jquery.min.js", "jquery.qtip.min.js", "jquery.js", "angular.js", "jquery.slim.js",
            "react.development.js", "react-dom.development.js", "react.production.min.js",
            "react-dom.production.min.js", "vue.global.js", "vue.esm-browser.js", "vue.js",
            "bootstrap.min.js", "bootstrap.bundle.min.js", "bootstrap.esm.min.js", "d3.min.js",
            "d3.js",
            "app.js",
            "main.js",
            "index.js",
            // Verified 3rd parties for request
            "https://m.stripe.network/inner.html",
            "https://m.stripe.network/out-4.5.43.js",
            "https://challenges.cloudflare.com/turnstile",
            "https://js.stripe.com/v3/"
        }
    };

    /// Ignore the content types.
    pub static ref IGNORE_CONTENT_TYPES: phf::Set<&'static str> = phf::phf_set! {
        "application/pdf",
        "application/zip",
        "application/x-rar-compressed",
        "application/x-tar",
        "image/png",
        "image/jpeg",
        "image/gif",
        "image/bmp",
        "image/svg+xml",
        "video/mp4",
        "video/x-msvideo",
        "video/x-matroska",
        "video/webm",
        "audio/mpeg",
        "audio/ogg",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "application/vnd.ms-excel",
        "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "application/vnd.ms-powerpoint",
        "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        "application/x-7z-compressed",
        "application/x-rpm",
        "application/x-shockwave-flash",
    };

    /// Ignore the resources for visual content types.
    pub static ref IGNORE_VISUAL_RESOURCE_MAP: phf::Set<&'static str> = phf::phf_set! {
        "Image",
        "Media",
        "Font",
        "Other",
    };

    /// Ignore the resources for visual content types.
    pub static ref IGNORE_NETWORKING_RESOURCE_MAP: phf::Set<&'static str> = phf::phf_set! {
        "Prefetch",
        "Ping",
    };
    /// Ignore list of scripts.
    static ref URL_IGNORE_TRIE: Trie = {
        let mut trie = Trie::new();
        let patterns = [
            "https://www.googletagservices.com/tag/",
            "https://js.hs-analytics.net/analytics/",
            "https://js.hsadspixel.net",
            "https://www.google.com/adsense/",
            "https://www.googleadservices.com",
            "https://adservice.google.com",
            "https://www.gstatic.com/cv/js/sender/",
            "https://googleads.g.doubleclick.net",
            "https://www.google-analytics.com",
            "https://www.googletagmanager.com",
            "https://iabusprivacy.pmc.com/geo-info.js",
            "https://cdn.onesignal.com",
            "https://cdn.cookielaw.org/",
            "https://static.doubleclick.net",
            "https://cdn.piano.io",
            "https://px.ads.linkedin.com",
            "https://connect.facebook.net",
            "https://tags.tiqcdn.com",
            "https://tr.snapchat.com",
            "https://ads.twitter.com",
            "https://cdn.segment.com",
            "https://stats.wp.com",
            "https://analytics.",
            "http://analytics.",
            "https://cdn.cxense.com",
            "https://cdn.tinypass.com",
            "https://cd.connatix.com",
            ".newrelic.com",
            ".googlesyndication.com",
            ".amazon-adsystem.com",
            ".onetrust.com",
            "sc.omtrdc.net",
            "doubleclick.net",
            "hotjar.com",
            "datadome.com",
            "datadog-logs-us.js",
            "tinypass.min.js",
            ".airship.com",
            ".adlightning.com",
            // explicit ignore tracking.js and ad files
            "privacy-notice.js",
            "tracking.js",
            "ads.js",
            "https://ads.",
            "http://ads.",
            "https://tracking.",
            "http://tracking.",
            // exp testin
            // used for possible location outside
            "https://geo.privacymanager.io/",
            // "https://www.recaptcha.net/recaptcha/",
            // "https://www.google.com/recaptcha/",
            // "https://www.gstatic.com/recaptcha/",
        ];
        for pattern in &patterns {
            trie.insert(pattern);
        }
        trie
    };

    /// Ignore list of XHR urls.
    static ref URL_IGNORE_XHR_TRIE: Trie = {
        let mut trie = Trie::new();
        let patterns = [
            "https://play.google.com/log?",
            "https://googleads.g.doubleclick.net/pagead/id",
            "https://js.monitor.azure.com/scripts",
            "https://securepubads.g.doubleclick.net",
            "https://pixel-config.reddit.com/pixels",
            // amazon product feedback
            "https://www.amazon.com/af/feedback-link?",
            "https://tr.snapchat.com/config/",
            "https://collect.tealiumiq.com/",
            "https://s.yimg.com/wi",
            "https://disney.my.sentry.io/api/",
            "https://www.redditstatic.com/ads",
            "https://buy.tinypass.com/",
            "https://idx.liadm.com",
            "https://geo.privacymanager.io/",
            "https://nimbleplot.com",
            "googlesyndication.com",
            ".piano.io/",
            ".browsiprod.com",
            ".onetrust.com/consent/",
            "https://logs.",
            "/track.php",
        ];
        for pattern in &patterns {
            trie.insert(pattern);
        }
        trie
    };

    /// Ignore list of scripts embedded or font extra.
    static ref URL_IGNORE_EMBEDED_TRIE: Trie = {
        let mut trie = Trie::new();
        let patterns = [
            "https://www.youtube.com/embed/",      // YouTube video embeds
            "https://www.google.com/maps/embed?",  // Google Maps embeds
            "https://player.vimeo.com/video/",     // Vimeo video embeds
            "https://open.spotify.com/embed/",     // Spotify music embeds
            "https://w.soundcloud.com/player/",    // SoundCloud embeds
            "https://platform.twitter.com/embed/", // Twitter embedded tweets
            "https://www.instagram.com/embed.js",  // Instagram embeds
            "https://www.facebook.com/plugins/",   // Facebook embeds (like posts and videos)
            "https://cdn.embedly.com/widgets/",    // Embedly embeds
            "https://player.twitch.tv/",           // Twitch video player embeds

            // insight tracker
            "https://insight.adsrvr.org/track/",
            "cxense.com/",
            // snapchat tracker
            "https://tr.snapchat.com/",
            "https://buy.tinypass.com",
            "https://nimbleplot.com/",
            // ignore font extras
            "https://kit.fontawesome.com/",
            "https://use.typekit.net",
            // ignore tailwind cdn
            "https://cdn.tailwindcss.com",
            // ignore extra ads
            "https://googleads.g.doubleclick.net",
            "amazon-adsystem.com",
            "g.doubleclick.net",
            "googlesyndication.com",
            "adsafeprotected.com",
            // more google tracking
            ".googlesyndication.com/safeframe/",
            // repeat consent js
            "/ccpa/user-consent.min.js",
            // privacy
            "privacy-notice.js",
            // // ignore amazon scripts for media
            // "https://m.media-amazon.com/images",
            // ".ssl-images-amazon.com/images/"
        ];
        for pattern in &patterns {
            trie.insert(pattern);
        }
        trie
    };

    /// Ignore list of XHR urls for media.
    static ref URL_IGNORE_XHR_MEDIA_TRIE: Trie = {
        let mut trie = Trie::new();
        let patterns = [
            "https://www.youtube.com/s/player/",
            "https://www.vimeo.com/player/",
            "https://soundcloud.com/player/",
            "https://open.spotify.com/",
            "https://api.spotify.com/v1/",
            "https://music.apple.com/"

        ];
        for pattern in &patterns {
            trie.insert(pattern);
        }
        trie
    };

    /// Visual assets to ignore for XHR request.
    pub(crate) static ref IGNORE_XHR_ASSETS: HashSet<CaseInsensitiveString> = {
        let mut m: HashSet<CaseInsensitiveString> = HashSet::with_capacity(36);

        m.extend([
            "jpg", "jpeg", "png", "gif", "svg", "webp",       // Image files
            "mp4", "avi", "mov", "wmv", "flv",               // Video files
            "mp3", "wav", "ogg",                             // Audio files
            "woff", "woff2", "ttf", "otf",                   // Font files
            "swf", "xap",                                    // Flash/Silverlight files
            "ico", "eot",                                    // Other resource files

            // Including extensions with extra dot
            ".jpg", ".jpeg", ".png", ".gif", ".svg", ".webp",
            ".mp4", ".avi", ".mov", ".wmv", ".flv",
            ".mp3", ".wav", ".ogg",
            ".woff", ".woff2", ".ttf", ".otf",
            ".swf", ".xap",
            ".ico", ".eot"
        ].map(|s| s.into()));

        m
    };

    /// Case insenstive css matching
    pub static ref CSS_EXTENSION: CaseInsensitiveString = CaseInsensitiveString::from("css");

}

// Trie node for ignore.
#[derive(Default)]
struct TrieNode {
    children: HashMap<char, TrieNode>,
    is_end_of_word: bool,
}

/// Basic Ignore trie.
struct Trie {
    root: TrieNode,
}

impl Trie {
    /// Setup a new trie.
    fn new() -> Self {
        Trie {
            root: TrieNode::default(),
        }
    }
    // Insert a word into the Trie.
    fn insert(&mut self, word: &str) {
        let mut node = &mut self.root;
        for ch in word.chars() {
            node = node.children.entry(ch).or_insert_with(TrieNode::default);
        }
        node.is_end_of_word = true;
    }
    // Check if the Trie contains any prefix of the given string.
    fn contains_prefix(&self, text: &str) -> bool {
        let mut node = &self.root;
        for ch in text.chars() {
            if let Some(next_node) = node.children.get(&ch) {
                node = next_node;
                if node.is_end_of_word {
                    return true;
                }
            } else {
                break;
            }
        }
        false
    }
}

/// Url matches analytics that we want to ignore or trackers.
pub(crate) fn ignore_script(url: &str) -> bool {
    let ignore_script = URL_IGNORE_TRIE.contains_prefix(url);

    // check for file ending in analytics.js
    if !ignore_script {
        url.ends_with("analytics.js")
            || url.ends_with("ads.js")
            || url.ends_with("tracking.js")
            || url.ends_with("track.js")
    } else {
        ignore_script
    }
}

/// Url matches analytics that we want to ignore or trackers.
pub(crate) fn ignore_script_embedded(url: &str) -> bool {
    URL_IGNORE_EMBEDED_TRIE.contains_prefix(url)
}

/// Url matches analytics that we want to ignore or trackers.
pub(crate) fn ignore_script_xhr(url: &str) -> bool {
    URL_IGNORE_XHR_TRIE.contains_prefix(url)
}

/// Url matches media that we want to ignore.
pub(crate) fn ignore_script_xhr_media(url: &str) -> bool {
    URL_IGNORE_XHR_MEDIA_TRIE.contains_prefix(url)
}

#[derive(Debug)]
pub struct NetworkManager {
    queued_events: VecDeque<NetworkEvent>,
    ignore_httpserrors: bool,
    requests: HashMap<RequestId, HttpRequest>,
    // TODO put event in an Arc?
    requests_will_be_sent: HashMap<RequestId, EventRequestWillBeSent>,
    extra_headers: HashMap<String, String>,
    request_id_to_interception_id: HashMap<RequestId, InterceptionId>,
    user_cache_disabled: bool,
    attempted_authentications: HashSet<RequestId>,
    credentials: Option<Credentials>,
    user_request_interception_enabled: bool,
    protocol_request_interception_enabled: bool,
    offline: bool,
    request_timeout: Duration,
    // made_request: bool,
    /// Ignore visuals (no pings, prefetching, and etc).
    pub ignore_visuals: bool,
    /// Block CSS stylesheets.
    pub block_stylesheets: bool,
    /// Block javascript that is not critical to rendering.
    pub block_javascript: bool,
    /// Block analytics from rendering
    pub block_analytics: bool,
    /// Only html from loading.
    pub only_html: bool,
}

impl NetworkManager {
    pub fn new(ignore_httpserrors: bool, request_timeout: Duration) -> Self {
        Self {
            queued_events: Default::default(),
            ignore_httpserrors,
            requests: Default::default(),
            requests_will_be_sent: Default::default(),
            extra_headers: Default::default(),
            request_id_to_interception_id: Default::default(),
            user_cache_disabled: false,
            attempted_authentications: Default::default(),
            credentials: None,
            user_request_interception_enabled: false,
            protocol_request_interception_enabled: false,
            offline: false,
            request_timeout,
            ignore_visuals: false,
            block_javascript: false,
            block_stylesheets: false,
            block_analytics: true,
            only_html: false,
        }
    }

    pub fn init_commands(&self) -> CommandChain {
        let enable = EnableParams::default();
        let mut v = vec![];

        if let Ok(c) = serde_json::to_value(&enable) {
            v.push((enable.identifier(), c));
        }

        let cmds = if self.ignore_httpserrors {
            let ignore = SetIgnoreCertificateErrorsParams::new(true);

            if let Ok(ignored) = serde_json::to_value(&ignore) {
                v.push((ignore.identifier(), ignored));
            }

            v
        } else {
            v
        };

        CommandChain::new(cmds, self.request_timeout)
    }

    fn push_cdp_request<T: Command>(&mut self, cmd: T) {
        let method = cmd.identifier();
        if let Ok(params) = serde_json::to_value(cmd) {
            self.queued_events
                .push_back(NetworkEvent::SendCdpRequest((method, params)));
        }
    }

    /// The next event to handle
    pub fn poll(&mut self) -> Option<NetworkEvent> {
        self.queued_events.pop_front()
    }

    pub fn extra_headers(&self) -> &HashMap<String, String> {
        &self.extra_headers
    }

    pub fn set_extra_headers(&mut self, headers: HashMap<String, String>) {
        self.extra_headers = headers;
        self.extra_headers.remove("proxy-authorization");
        if let Ok(headers) = serde_json::to_value(&self.extra_headers) {
            self.push_cdp_request(SetExtraHttpHeadersParams::new(Headers::new(headers)));
        }
    }

    pub fn set_request_interception(&mut self, enabled: bool) {
        self.user_request_interception_enabled = enabled;
        self.update_protocol_request_interception();
    }

    pub fn set_cache_enabled(&mut self, enabled: bool) {
        self.user_cache_disabled = !enabled;
        self.update_protocol_cache_disabled();
    }

    pub fn update_protocol_cache_disabled(&mut self) {
        self.push_cdp_request(SetCacheDisabledParams::new(
            self.user_cache_disabled || self.protocol_request_interception_enabled,
        ));
    }

    pub fn authenticate(&mut self, credentials: Credentials) {
        self.credentials = Some(credentials);
        self.update_protocol_request_interception()
    }

    fn update_protocol_request_interception(&mut self) {
        let enabled = self.user_request_interception_enabled || self.credentials.is_some();

        if enabled == self.protocol_request_interception_enabled {
            return;
        }
        self.update_protocol_cache_disabled();

        if enabled {
            self.push_cdp_request(
                fetch::EnableParams::builder()
                    .handle_auth_requests(true)
                    .pattern(RequestPattern::builder().url_pattern("*").build())
                    .build(),
            )
        } else {
            self.push_cdp_request(DisableParams::default())
        }
    }

    /// Determine if the request should be skipped.
    fn skip_xhr(&self, skip_networking: bool, event: &EventRequestPaused) -> bool {
        // XHR check
        if !skip_networking && event.resource_type == ResourceType::Xhr {
            let request_url = event.request.url.as_str();

            // check if part of ignore scripts.
            let skip_analytics = self.block_analytics && ignore_script_xhr(request_url);

            if skip_analytics {
                true
            } else if self.block_stylesheets || self.ignore_visuals {
                let block_css = self.block_stylesheets;
                let block_media = self.ignore_visuals && self.only_html;

                let mut block_request = false;

                if let Some(position) = request_url.rfind('.') {
                    let hlen = request_url.len();
                    let has_asset = hlen - position;

                    if has_asset >= 3 {
                        let next_position = position + 1;

                        if block_media
                            && IGNORE_XHR_ASSETS.contains::<CaseInsensitiveString>(
                                &request_url[next_position..].into(),
                            )
                        {
                            block_request = true;
                        } else if block_css {
                            block_request =
                                CaseInsensitiveString::from(request_url[next_position..].as_bytes())
                                    .contains(&**CSS_EXTENSION)
                        }
                    }
                }

                if !block_request {
                    block_request = ignore_script_xhr_media(request_url);
                }

                block_request
            } else {
                skip_networking
            }
        } else {
            skip_networking
        }
    }

    #[cfg(not(feature = "adblock"))]
    pub fn on_fetch_request_paused(&mut self, event: &EventRequestPaused) {
        if !self.user_request_interception_enabled && self.protocol_request_interception_enabled {
            self.push_cdp_request(ContinueRequestParams::new(event.request_id.clone()))
        } else {
            if let Some(network_id) = event.network_id.as_ref() {
                if let Some(request_will_be_sent) =
                    self.requests_will_be_sent.remove(network_id.as_ref())
                {
                    self.on_request(&request_will_be_sent, Some(event.request_id.clone().into()));
                } else {
                    let javascript_resource = ResourceType::Script == event.resource_type;

                    // main initial check
                    let skip_networking = IGNORE_NETWORKING_RESOURCE_MAP
                        .contains(event.resource_type.as_ref())
                        || self.ignore_visuals
                            && (IGNORE_VISUAL_RESOURCE_MAP.contains(event.resource_type.as_ref()))
                        || self.block_stylesheets
                            && ResourceType::Stylesheet == event.resource_type
                        || self.block_javascript
                            && javascript_resource
                            && !JS_FRAMEWORK_ALLOW.contains(event.request.url.as_str());

                    let skip_networking = if !skip_networking
                        && (self.only_html || self.ignore_visuals)
                        && (javascript_resource || event.resource_type == ResourceType::Document)
                    {
                        ignore_script_embedded(event.request.url.as_str())
                    } else {
                        skip_networking
                    };

                    // analytics check
                    let skip_networking =
                        if !skip_networking && javascript_resource && self.block_analytics {
                            ignore_script(event.request.url.as_str())
                        } else {
                            skip_networking
                        };

                    // XHR check
                    let skip_networking = self.skip_xhr(skip_networking, &event);

                    if skip_networking {
                        let fullfill_params =
                            crate::handler::network::fetch::FulfillRequestParams::new(
                                event.request_id.clone(),
                                200,
                            );
                        self.push_cdp_request(fullfill_params);
                    } else {
                        self.push_cdp_request(ContinueRequestParams::new(event.request_id.clone()))
                    }
                }
            } else {
                self.push_cdp_request(ContinueRequestParams::new(event.request_id.clone()))
            }
        }
    }

    #[cfg(feature = "adblock")]
    pub fn on_fetch_request_paused(&mut self, event: &EventRequestPaused) {
        if !self.user_request_interception_enabled && self.protocol_request_interception_enabled {
            self.push_cdp_request(ContinueRequestParams::new(event.request_id.clone()))
        } else {
            if let Some(network_id) = event.network_id.as_ref() {
                if let Some(request_will_be_sent) =
                    self.requests_will_be_sent.remove(network_id.as_ref())
                {
                    self.on_request(&request_will_be_sent, Some(event.request_id.clone().into()));
                } else {
                    let skip_networking = IGNORE_NETWORKING_RESOURCE_MAP
                        .contains(&event.resource_type.as_ref())
                        || self.ignore_visuals
                            && (IGNORE_VISUAL_RESOURCE_MAP.contains(&event.resource_type.as_ref())
                                || self.block_stylesheets
                                    && ResourceType::Stylesheet == event.resource_type)
                        || self.block_javascript
                            && ResourceType::Script == event.resource_type
                            && !JS_FRAMEWORK_ALLOW.contains(&event.request.url.as_str());

                    let skip_networking = if !skip_networking
                        && javascript_resource
                        && (self.only_html || self.ignore_visuals)
                    {
                        ignore_script_embedded(event.request.url.as_str())
                    } else {
                        skip_networking
                    };

                    // analytics check
                    let skip_networking =
                        if !skip_networking && javascript_resource && self.block_analytics {
                            ignore_script(event.request.url.as_str())
                        } else {
                            skip_networking
                        };

                    // XHR check
                    let skip_networking = self.skip_xhr(skip_networking, &event);

                    if self.detect_ad(event) || skip_networking {
                        let fullfill_params =
                            crate::handler::network::fetch::FulfillRequestParams::new(
                                event.request_id.clone(),
                                200,
                            );
                        self.push_cdp_request(fullfill_params);
                    } else {
                        self.push_cdp_request(ContinueRequestParams::new(event.request_id.clone()))
                    }
                }
            }
        }

        // if self.only_html {
        //     self.made_request = true;
        // }
    }

    /// Perform a page intercept for chrome
    #[cfg(feature = "adblock")]
    pub fn detect_ad(&self, event: &EventRequestPaused) -> bool {
        use adblock::{
            lists::{FilterSet, ParseOptions},
            Engine,
        };
        lazy_static::lazy_static! {
            static ref AD_ENGINE: Engine = {
                let mut filter_set = FilterSet::new(false);
                filter_set.add_filters(
                    &vec![
                        String::from("-advertisement."),
                        String::from("-ads."),
                        String::from("-ad."),
                        String::from("-advertisement-icon."),
                        String::from("-advertisement-management/"),
                        String::from("-advertisement/script."),
                        String::from("-ads/script."),
                    ],
                    ParseOptions::default(),
                );
                Engine::from_filter_set(filter_set, true)
            };
        };

        let asset = ResourceType::Image == event.resource_type
            || ResourceType::Media == event.resource_type
            || ResourceType::Stylesheet == event.resource_type;
        let u = &event.request.url;

        !self.ignore_visuals
            && (asset
                || event.resource_type == ResourceType::Fetch
                || event.resource_type == ResourceType::Xhr)
                // set it to example.com for 3rd party handling is_same_site
            &&   match adblock::request::Request::new(&u,  if event.request.is_same_site.unwrap_or_default() {&u } else { &"https://example.com" }, &event.resource_type.as_ref()) {
                Ok(adblock_request) => AD_ENGINE.check_network_request(&adblock_request).matched,
                _ => false,
            }
    }

    pub fn on_fetch_auth_required(&mut self, event: &EventAuthRequired) {
        let response = if self
            .attempted_authentications
            .contains(event.request_id.as_ref())
        {
            AuthChallengeResponseResponse::CancelAuth
        } else if self.credentials.is_some() {
            self.attempted_authentications
                .insert(event.request_id.clone().into());
            AuthChallengeResponseResponse::ProvideCredentials
        } else {
            AuthChallengeResponseResponse::Default
        };

        let mut auth = AuthChallengeResponse::new(response);
        if let Some(creds) = self.credentials.clone() {
            auth.username = Some(creds.username);
            auth.password = Some(creds.password);
        }
        self.push_cdp_request(ContinueWithAuthParams::new(event.request_id.clone(), auth));
    }

    pub fn set_offline_mode(&mut self, value: bool) {
        if self.offline == value {
            return;
        }
        self.offline = value;
        if let Ok(network) = EmulateNetworkConditionsParams::builder()
            .offline(self.offline)
            .latency(0)
            .download_throughput(-1.)
            .upload_throughput(-1.)
            .build()
        {
            self.push_cdp_request(network);
        }
    }

    /// Request interception doesn't happen for data URLs with Network Service.
    pub fn on_request_will_be_sent(&mut self, event: &EventRequestWillBeSent) {
        if self.protocol_request_interception_enabled && !event.request.url.starts_with("data:") {
            if let Some(interception_id) = self
                .request_id_to_interception_id
                .remove(event.request_id.as_ref())
            {
                self.on_request(event, Some(interception_id));
            } else {
                // TODO remove the clone for event
                self.requests_will_be_sent
                    .insert(event.request_id.clone(), event.clone());
            }
        } else {
            self.on_request(event, None);
        }
    }

    pub fn on_request_served_from_cache(&mut self, event: &EventRequestServedFromCache) {
        if let Some(request) = self.requests.get_mut(event.request_id.as_ref()) {
            request.from_memory_cache = true;
        }
    }

    pub fn on_response_received(&mut self, event: &EventResponseReceived) {
        if let Some(mut request) = self.requests.remove(event.request_id.as_ref()) {
            request.set_response(event.response.clone());
            self.queued_events
                .push_back(NetworkEvent::RequestFinished(request))
        }
    }

    pub fn on_network_loading_finished(&mut self, event: &EventLoadingFinished) {
        if let Some(request) = self.requests.remove(event.request_id.as_ref()) {
            if let Some(interception_id) = request.interception_id.as_ref() {
                self.attempted_authentications
                    .remove(interception_id.as_ref());
            }
            self.queued_events
                .push_back(NetworkEvent::RequestFinished(request));
        }
    }

    pub fn on_network_loading_failed(&mut self, event: &EventLoadingFailed) {
        if let Some(mut request) = self.requests.remove(event.request_id.as_ref()) {
            request.failure_text = Some(event.error_text.clone());
            if let Some(interception_id) = request.interception_id.as_ref() {
                self.attempted_authentications
                    .remove(interception_id.as_ref());
            }
            self.queued_events
                .push_back(NetworkEvent::RequestFailed(request));
        }
    }

    fn on_request(
        &mut self,
        event: &EventRequestWillBeSent,
        interception_id: Option<InterceptionId>,
    ) {
        let mut redirect_chain = Vec::new();
        if let Some(redirect_resp) = event.redirect_response.as_ref() {
            if let Some(mut request) = self.requests.remove(event.request_id.as_ref()) {
                self.handle_request_redirect(&mut request, redirect_resp.clone());
                redirect_chain = std::mem::take(&mut request.redirect_chain);
                redirect_chain.push(request);
            }
        }
        let request = HttpRequest::new(
            event.request_id.clone(),
            event.frame_id.clone(),
            interception_id,
            self.user_request_interception_enabled,
            redirect_chain,
        );

        self.requests.insert(event.request_id.clone(), request);
        self.queued_events
            .push_back(NetworkEvent::Request(event.request_id.clone()));
    }

    fn handle_request_redirect(&mut self, request: &mut HttpRequest, response: Response) {
        request.set_response(response);
        if let Some(interception_id) = request.interception_id.as_ref() {
            self.attempted_authentications
                .remove(interception_id.as_ref());
        }
    }
}

#[derive(Debug)]
pub enum NetworkEvent {
    SendCdpRequest((MethodId, serde_json::Value)),
    Request(RequestId),
    Response(RequestId),
    RequestFailed(HttpRequest),
    RequestFinished(HttpRequest),
}

//! Constants for the Twitter GraphQL API.

/// Base URL for Twitter's GraphQL API.
pub const TWITTER_API_BASE: &str = "https://x.com/i/api/graphql";

/// URL for GraphQL POST requests.
pub const TWITTER_GRAPHQL_POST_URL: &str = "https://x.com/i/api/graphql";

/// URL for media uploads.
pub const TWITTER_UPLOAD_URL: &str = "https://upload.twitter.com/i/media/upload.json";

/// URL for media metadata.
pub const TWITTER_MEDIA_METADATA_URL: &str = "https://x.com/i/api/1.1/media/metadata/create.json";

/// URL for status updates (legacy).
pub const TWITTER_STATUS_UPDATE_URL: &str = "https://x.com/i/api/1.1/statuses/update.json";

/// Bearer token used by Twitter's web client.
pub const BEARER_TOKEN: &str = "Bearer AAAAAAAAAAAAAAAAAAAAANRILgAAAAAAnNwIzUejRCOuH5E6I8xnZz4puTs%3D1Zv7ttfk8LF81IUq16cHjhLTvJu4FA33AGWWjCpTnA";

/// Default user agent string.
pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";

/// Default count for paginated requests.
pub const DEFAULT_PAGE_COUNT: u32 = 20;

/// GraphQL operation names and their query IDs.
/// Note: These IDs rotate frequently and may need updating.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    CreateTweet,
    CreateRetweet,
    DeleteRetweet,
    CreateFriendship,
    DestroyFriendship,
    FavoriteTweet,
    UnfavoriteTweet,
    CreateBookmark,
    DeleteBookmark,
    TweetDetail,
    SearchTimeline,
    UserArticlesTweets,
    UserTweets,
    Bookmarks,
    Following,
    Followers,
    Likes,
    BookmarkFolderTimeline,
    ListOwnerships,
    ListMemberships,
    ListLatestTweetsTimeline,
    ListByRestId,
    HomeTimeline,
    HomeLatestTimeline,
    ExploreSidebar,
    ExplorePage,
    GenericTimelineById,
    TrendHistory,
    AboutAccountQuery,
}

impl Operation {
    /// Get the default/fallback query ID for this operation.
    pub fn default_query_id(&self) -> &'static str {
        match self {
            Operation::CreateTweet => "TAJw1rBsjAtdNgTdlo2oeg",
            Operation::CreateRetweet => "ojPdsZsimiJrUGLR1sjUtA",
            Operation::DeleteRetweet => "iQtK4dl5hBmXewYZuEOKVw",
            Operation::CreateFriendship => "8h9JVdV8dlSyqyRDJEPCsA",
            Operation::DestroyFriendship => "ppXWuagMNXgvzx6WoXBW0Q",
            Operation::FavoriteTweet => "lI07N6Otwv1PhnEgXILM7A",
            Operation::UnfavoriteTweet => "ZYKSe-w7KEslx3JhSIk5LA",
            Operation::CreateBookmark => "aoDbu3RHznuiSkQ9aNM67Q",
            Operation::DeleteBookmark => "Wlmlj2-xzyS1GN3a6cj-mQ",
            Operation::TweetDetail => "97JF30KziU00483E_8elBA",
            Operation::SearchTimeline => "M1jEez78PEfVfbQLvlWMvQ",
            Operation::UserArticlesTweets => "8zBy9h4L90aDL02RsBcCFg",
            Operation::UserTweets => "Wms1GvIiHXAPBaCr9KblaA",
            Operation::Bookmarks => "RV1g3b8n_SGOHwkqKYSCFw",
            Operation::Following => "BEkNpEt5pNETESoqMsTEGA",
            Operation::Followers => "kuFUYP9eV1FPoEy4N-pi7w",
            Operation::Likes => "JR2gceKucIKcVNB_9JkhsA",
            Operation::BookmarkFolderTimeline => "KJIQpsvxrTfRIlbaRIySHQ",
            Operation::ListOwnerships => "wQcOSjSQ8NtgxIwvYl1lMg",
            Operation::ListMemberships => "BlEXXdARdSeL_0KyKHHvvg",
            Operation::ListLatestTweetsTimeline => "2TemLyqrMpTeAmysdbnVqw",
            Operation::ListByRestId => "wXzyA5vM_aVkBL9G8Vp3kw",
            Operation::HomeTimeline => "edseUwk9sP5Phz__9TIRnA",
            Operation::HomeLatestTimeline => "iOEZpOdfekFsxSlPQCQtPg",
            Operation::ExploreSidebar => "lpSN4M6qpimkF4nRFPE3nQ",
            Operation::ExplorePage => "kheAINB_4pzRDqkzG3K-ng",
            Operation::GenericTimelineById => "uGSr7alSjR9v6QJAIaqSKQ",
            Operation::TrendHistory => "Sj4T-jSB9pr0Mxtsc1UKZQ",
            Operation::AboutAccountQuery => "zs_jFPFT78rBpXv9Z3U2YQ",
        }
    }

    /// Get the operation name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            Operation::CreateTweet => "CreateTweet",
            Operation::CreateRetweet => "CreateRetweet",
            Operation::DeleteRetweet => "DeleteRetweet",
            Operation::CreateFriendship => "CreateFriendship",
            Operation::DestroyFriendship => "DestroyFriendship",
            Operation::FavoriteTweet => "FavoriteTweet",
            Operation::UnfavoriteTweet => "UnfavoriteTweet",
            Operation::CreateBookmark => "CreateBookmark",
            Operation::DeleteBookmark => "DeleteBookmark",
            Operation::TweetDetail => "TweetDetail",
            Operation::SearchTimeline => "SearchTimeline",
            Operation::UserArticlesTweets => "UserArticlesTweets",
            Operation::UserTweets => "UserTweets",
            Operation::Bookmarks => "Bookmarks",
            Operation::Following => "Following",
            Operation::Followers => "Followers",
            Operation::Likes => "Likes",
            Operation::BookmarkFolderTimeline => "BookmarkFolderTimeline",
            Operation::ListOwnerships => "ListOwnerships",
            Operation::ListMemberships => "ListMemberships",
            Operation::ListLatestTweetsTimeline => "ListLatestTweetsTimeline",
            Operation::ListByRestId => "ListByRestId",
            Operation::HomeTimeline => "HomeTimeline",
            Operation::HomeLatestTimeline => "HomeLatestTimeline",
            Operation::ExploreSidebar => "ExploreSidebar",
            Operation::ExplorePage => "ExplorePage",
            Operation::GenericTimelineById => "GenericTimelineById",
            Operation::TrendHistory => "TrendHistory",
            Operation::AboutAccountQuery => "AboutAccountQuery",
        }
    }

    /// Get fallback query IDs for operations that have multiple known IDs.
    pub fn fallback_query_ids(&self) -> &[&'static str] {
        match self {
            Operation::TweetDetail => &["97JF30KziU00483E_8elBA", "aFvUsJm2c-oDkJV75blV6g"],
            Operation::SearchTimeline => &[
                "M1jEez78PEfVfbQLvlWMvQ",
                "5h0kNbk3ii97rmfY6CdgAA",
                "Tp1sewRU1AsZpBWhqCZicQ",
            ],
            _ => &[],
        }
    }
}

/// GraphQL feature flags sent with requests.
pub mod features {
    use serde_json::{json, Value};

    /// Get the default feature flags for GraphQL requests.
    pub fn default_features() -> Value {
        json!({
            "creator_subscriptions_tweet_preview_api_enabled": true,
            "communities_web_enable_tweet_community_results_fetch": true,
            "c9s_tweet_anatomy_moderator_badge_enabled": true,
            "articles_preview_enabled": true,
            "responsive_web_edit_tweet_api_enabled": true,
            "graphql_is_translatable_rweb_tweet_is_translatable_enabled": true,
            "view_counts_everywhere_api_enabled": true,
            "longform_notetweets_consumption_enabled": true,
            "responsive_web_twitter_article_tweet_consumption_enabled": true,
            "tweet_awards_web_tipping_enabled": false,
            "creator_subscriptions_quote_tweet_preview_enabled": false,
            "freedom_of_speech_not_reach_fetch_enabled": true,
            "standardized_nudges_misinfo": true,
            "tweet_with_visibility_results_prefer_gql_limited_actions_policy_enabled": true,
            "rweb_video_timestamps_enabled": true,
            "longform_notetweets_rich_text_read_enabled": true,
            "longform_notetweets_inline_media_enabled": true,
            "rweb_tipjar_consumption_enabled": true,
            "responsive_web_graphql_exclude_directive_enabled": true,
            "verified_phone_label_enabled": false,
            "responsive_web_graphql_skip_user_profile_image_extensions_enabled": false,
            "responsive_web_graphql_timeline_navigation_enabled": true,
            "responsive_web_enhance_cards_enabled": false,
            "premium_content_api_read_enabled": false,
            "responsive_web_media_download_video_enabled": true,
            "responsive_web_twitter_article_notes_tab_enabled": true,
            "profile_label_improvements_pcf_label_in_post_enabled": true,
            "hidden_profile_subscriptions_enabled": true,
            "highlights_tweets_tab_ui_enabled": true,
            "subscriptions_feature_can_gift_premium": true,
            "responsive_web_grok_analyze_button_fetch_trends_enabled": false,
            "responsive_web_grok_analyze_post_followups_enabled": false,
            "responsive_web_grok_share_attachment_enabled": false,
            "responsive_web_jetfuel_frame": false
        })
    }
}

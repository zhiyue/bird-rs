//! Twitter client implementation.

mod tweet_detail;

use crate::constants::{features, Operation, BEARER_TOKEN, DEFAULT_USER_AGENT, TWITTER_API_BASE};
use crate::cookies::TwitterCookies;
use crate::error::Result;
use crate::types::{CurrentUser, CurrentUserResult, TweetData};
use crate::TwitterClientOptions;
use reqwest::header::{HeaderMap, HeaderValue, ACCEPT, CONTENT_TYPE, COOKIE, USER_AGENT};
use reqwest::Client;
use std::time::Duration;
use uuid::Uuid;

/// Twitter client for interacting with the GraphQL API.
pub struct TwitterClient {
    http_client: Client,
    #[allow(dead_code)]
    auth_token: String,
    ct0: String,
    cookie_header: String,
    user_agent: String,
    #[allow(dead_code)]
    timeout_ms: Option<u64>,
    quote_depth: u32,
    client_uuid: String,
    client_device_id: String,
    client_user_id: Option<String>,
}

impl TwitterClient {
    /// Create a new Twitter client with the given options.
    pub fn new(options: TwitterClientOptions) -> Self {
        let http_client = Client::builder()
            .timeout(Duration::from_millis(options.timeout_ms.unwrap_or(30000)))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            http_client,
            auth_token: options.cookies.auth_token,
            ct0: options.cookies.ct0,
            cookie_header: options.cookies.cookie_header,
            user_agent: DEFAULT_USER_AGENT.to_string(),
            timeout_ms: options.timeout_ms,
            quote_depth: options.quote_depth.unwrap_or(1),
            client_uuid: Uuid::new_v4().to_string(),
            client_device_id: Uuid::new_v4().to_string(),
            client_user_id: None,
        }
    }

    /// Create a new Twitter client from cookies.
    pub fn from_cookies(cookies: TwitterCookies) -> Self {
        Self::new(TwitterClientOptions {
            cookies,
            timeout_ms: None,
            quote_depth: None,
        })
    }

    /// Get the default headers for API requests.
    fn get_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        headers.insert(ACCEPT, HeaderValue::from_static("*/*"));
        headers.insert(
            "accept-language",
            HeaderValue::from_static("en-US,en;q=0.9"),
        );
        headers.insert("authorization", HeaderValue::from_static(BEARER_TOKEN));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        headers.insert("x-csrf-token", HeaderValue::from_str(&self.ct0).unwrap());
        headers.insert(
            "x-twitter-auth-type",
            HeaderValue::from_static("OAuth2Session"),
        );
        headers.insert("x-twitter-active-user", HeaderValue::from_static("yes"));
        headers.insert("x-twitter-client-language", HeaderValue::from_static("en"));
        headers.insert(
            "x-client-uuid",
            HeaderValue::from_str(&self.client_uuid).unwrap(),
        );
        headers.insert(
            "x-twitter-client-deviceid",
            HeaderValue::from_str(&self.client_device_id).unwrap(),
        );
        headers.insert(
            "x-client-transaction-id",
            HeaderValue::from_str(&self.create_transaction_id()).unwrap(),
        );
        headers.insert(COOKIE, HeaderValue::from_str(&self.cookie_header).unwrap());
        headers.insert(USER_AGENT, HeaderValue::from_str(&self.user_agent).unwrap());
        headers.insert("origin", HeaderValue::from_static("https://x.com"));
        headers.insert("referer", HeaderValue::from_static("https://x.com/"));

        if let Some(ref user_id) = self.client_user_id {
            headers.insert(
                "x-twitter-client-user-id",
                HeaderValue::from_str(user_id).unwrap(),
            );
        }

        headers
    }

    /// Create a random transaction ID.
    fn create_transaction_id(&self) -> String {
        use std::fmt::Write;
        let bytes: [u8; 16] = rand::random();
        let mut hex = String::with_capacity(32);
        for byte in bytes {
            write!(hex, "{:02x}", byte).unwrap();
        }
        hex
    }

    /// Build a GraphQL URL for the given operation.
    fn build_graphql_url(&self, operation: Operation, variables: &serde_json::Value) -> String {
        let query_id = operation.default_query_id();
        let features_json = serde_json::to_string(&features::default_features()).unwrap();
        let variables_json = serde_json::to_string(variables).unwrap();

        format!(
            "{}/{}/{}?variables={}&features={}",
            TWITTER_API_BASE,
            query_id,
            operation.name(),
            urlencoding::encode(&variables_json),
            urlencoding::encode(&features_json)
        )
    }

    /// Get the current authenticated user.
    pub async fn get_current_user(&mut self) -> CurrentUserResult {
        // Use the settings endpoint to get current user info
        let url = "https://x.com/i/api/1.1/account/settings.json";

        let response = match self
            .http_client
            .get(url)
            .headers(self.get_headers())
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => return CurrentUserResult::Error(e.to_string()),
        };

        if !response.status().is_success() {
            return CurrentUserResult::Error(format!("HTTP {}", response.status()));
        }

        let text = match response.text().await {
            Ok(t) => t,
            Err(e) => return CurrentUserResult::Error(e.to_string()),
        };

        // Parse screen_name from response
        let screen_name_regex = regex::Regex::new(r#""screen_name":"([^"]+)""#).unwrap();
        let user_id_regex = regex::Regex::new(r#""user_id"\s*:\s*"(\d+)""#).unwrap();
        let name_regex = regex::Regex::new(r#""name":"([^"\\]*(?:\\.[^"\\]*)*)""#).unwrap();

        let username = screen_name_regex
            .captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());

        let id = user_id_regex
            .captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());

        let name = name_regex
            .captures(&text)
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().to_string());

        match (id, username, name) {
            (Some(id), Some(username), Some(name)) => {
                self.client_user_id = Some(id.clone());
                CurrentUserResult::Success(CurrentUser { id, username, name })
            }
            _ => CurrentUserResult::Error("Failed to parse user info from response".to_string()),
        }
    }

    /// Get a tweet by ID.
    pub async fn get_tweet(&self, tweet_id: &str) -> Result<TweetData> {
        self.get_tweet_detail(tweet_id).await
    }
}

// Add rand dependency for transaction ID generation
mod rand {
    pub fn random<T: Default + AsMut<[u8]>>() -> T {
        let mut value = T::default();
        getrandom::getrandom(value.as_mut()).expect("Failed to generate random bytes");
        value
    }
}

// Add getrandom as a dependency
extern crate getrandom;

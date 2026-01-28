//! Cookie extraction and credential resolution.

use bird_core::{Error, Result};

/// Twitter authentication cookies.
#[derive(Debug, Clone)]
pub struct TwitterCookies {
    /// The auth_token cookie value.
    pub auth_token: String,
    /// The ct0 (CSRF token) cookie value.
    pub ct0: String,
    /// Full cookie header string.
    pub cookie_header: String,
}

impl TwitterCookies {
    /// Create new Twitter cookies from auth_token and ct0 values.
    pub fn new(auth_token: String, ct0: String) -> Self {
        let cookie_header = format!("auth_token={}; ct0={}", auth_token, ct0);
        Self {
            auth_token,
            ct0,
            cookie_header,
        }
    }

    /// Create new Twitter cookies with optional twid cookie for user ID extraction.
    pub fn new_with_twid(auth_token: String, ct0: String, twid: Option<String>) -> Self {
        let cookie_header = match twid {
            Some(ref tw) => format!("auth_token={}; ct0={}; twid={}", auth_token, ct0, tw),
            None => format!("auth_token={}; ct0={}", auth_token, ct0),
        };
        Self {
            auth_token,
            ct0,
            cookie_header,
        }
    }
}

/// Resolve Twitter credentials from various sources.
///
/// Priority order:
/// 1. Explicit auth_token and ct0 parameters
/// 2. Environment variables (AUTH_TOKEN, CT0 or TWITTER_AUTH_TOKEN, TWITTER_CT0)
/// 3. Browser cookies via sweet-cookie (currently Safari only)
///
/// # Arguments
///
/// * `auth_token` - Optional explicit auth_token
/// * `ct0` - Optional explicit ct0
/// * `_cookie_sources` - Cookie sources to try (currently unused, Safari only)
///
/// # Errors
///
/// Returns an error if no valid credentials could be found.
pub fn resolve_credentials(
    auth_token: Option<&str>,
    ct0: Option<&str>,
    _cookie_sources: &[&str],
) -> Result<TwitterCookies> {
    // 1. Check explicit parameters
    if let (Some(auth), Some(csrf)) = (auth_token, ct0) {
        return Ok(TwitterCookies::new(auth.to_string(), csrf.to_string()));
    }

    // 2. Check environment variables
    let env_auth = std::env::var("AUTH_TOKEN")
        .or_else(|_| std::env::var("TWITTER_AUTH_TOKEN"))
        .ok();
    let env_ct0 = std::env::var("CT0")
        .or_else(|_| std::env::var("TWITTER_CT0"))
        .ok();

    if let (Some(auth), Some(csrf)) = (env_auth, env_ct0) {
        return Ok(TwitterCookies::new(auth, csrf));
    }

    // 3. Try browser cookies via sweet-cookie
    extract_from_safari()
}

/// Extract Twitter cookies from Safari.
fn extract_from_safari() -> Result<TwitterCookies> {
    use sweet_cookie::{get_cookies, GetCookiesOptions};

    let options =
        GetCookiesOptions::new("https://x.com").with_names(["auth_token", "ct0", "twid"]);

    let result =
        get_cookies(&options).map_err(|e| Error::CookieExtraction(format!("Safari: {}", e)))?;

    // Also try twitter.com domain
    let twitter_options =
        GetCookiesOptions::new("https://twitter.com").with_names(["auth_token", "ct0", "twid"]);

    let twitter_result = get_cookies(&twitter_options).ok();

    // Merge cookies from both domains
    let mut auth_token: Option<String> = None;
    let mut ct0: Option<String> = None;
    let mut twid: Option<String> = None;

    for cookie in result.cookies.iter() {
        match cookie.name.as_str() {
            "auth_token" if auth_token.is_none() => auth_token = Some(cookie.value.clone()),
            "ct0" if ct0.is_none() => ct0 = Some(cookie.value.clone()),
            "twid" if twid.is_none() => twid = Some(cookie.value.clone()),
            _ => {}
        }
    }

    if let Some(twitter_cookies) = twitter_result {
        for cookie in twitter_cookies.cookies.iter() {
            match cookie.name.as_str() {
                "auth_token" if auth_token.is_none() => auth_token = Some(cookie.value.clone()),
                "ct0" if ct0.is_none() => ct0 = Some(cookie.value.clone()),
                "twid" if twid.is_none() => twid = Some(cookie.value.clone()),
                _ => {}
            }
        }
    }

    match (auth_token, ct0) {
        (Some(auth), Some(csrf)) => Ok(TwitterCookies::new_with_twid(auth, csrf, twid)),
        (Some(_), None) => Err(Error::CookieExtraction(
            "Found auth_token but ct0 is missing".to_string(),
        )),
        (None, Some(_)) => Err(Error::CookieExtraction(
            "Found ct0 but auth_token is missing".to_string(),
        )),
        (None, None) => Err(Error::CookieExtraction(
            "No Twitter cookies found in Safari. Please log in to x.com in Safari.".to_string(),
        )),
    }
}

/// Check which credential sources are available.
pub fn check_available_sources() -> Vec<CredentialSource> {
    let mut sources = Vec::new();

    // Check environment variables
    let env_auth = std::env::var("AUTH_TOKEN")
        .or_else(|_| std::env::var("TWITTER_AUTH_TOKEN"))
        .ok();
    let env_ct0 = std::env::var("CT0")
        .or_else(|_| std::env::var("TWITTER_CT0"))
        .ok();

    if env_auth.is_some() && env_ct0.is_some() {
        sources.push(CredentialSource::Environment);
    }

    // Check Safari cookies
    if extract_from_safari().is_ok() {
        sources.push(CredentialSource::Safari);
    }

    sources
}

/// Source of Twitter credentials.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// Credentials from environment variables.
    Environment,
    /// Credentials from Safari cookies.
    Safari,
    /// Credentials from Chrome cookies.
    Chrome,
    /// Credentials from Firefox cookies.
    Firefox,
    /// Credentials provided explicitly via CLI.
    Explicit,
}

impl std::fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CredentialSource::Environment => write!(f, "Environment variables"),
            CredentialSource::Safari => write!(f, "Safari"),
            CredentialSource::Chrome => write!(f, "Chrome"),
            CredentialSource::Firefox => write!(f, "Firefox"),
            CredentialSource::Explicit => write!(f, "CLI arguments"),
        }
    }
}

//! In-memory storage implementation for testing.

use async_trait::async_trait;
use bird_core::{
    Error, MentionedUser, Result, SyncState, SyncStateStore, TweetData, TweetStore, UserStore,
};
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

/// In-memory storage for testing purposes.
pub struct MemoryStorage {
    tweets: RwLock<HashMap<String, TweetData>>,
    collections: RwLock<HashMap<(String, String), HashSet<String>>>, // (collection, user_id) -> tweet_ids
    sync_states: RwLock<HashMap<(String, String), SyncState>>, // (collection, user_id) -> state
    users: RwLock<HashMap<String, MentionedUser>>,             // user_id -> user
    usernames: RwLock<HashMap<String, String>>,                // username_lower -> user_id
}

impl MemoryStorage {
    /// Create a new in-memory storage instance.
    pub fn new() -> Self {
        Self {
            tweets: RwLock::new(HashMap::new()),
            collections: RwLock::new(HashMap::new()),
            sync_states: RwLock::new(HashMap::new()),
            users: RwLock::new(HashMap::new()),
            usernames: RwLock::new(HashMap::new()),
        }
    }
}

impl Default for MemoryStorage {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl TweetStore for MemoryStorage {
    async fn upsert_tweet(&self, tweet: &TweetData) -> Result<()> {
        let mut tweets = self
            .tweets
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;
        tweets.insert(tweet.id.clone(), tweet.clone());
        Ok(())
    }

    async fn upsert_tweets(&self, tweets: &[TweetData]) -> Result<usize> {
        let mut storage = self
            .tweets
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let mut new_count = 0;
        for tweet in tweets {
            if !storage.contains_key(&tweet.id) {
                new_count += 1;
            }
            storage.insert(tweet.id.clone(), tweet.clone());
        }
        Ok(new_count)
    }

    async fn get_tweet(&self, id: &str) -> Result<Option<TweetData>> {
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(tweets.get(id).cloned())
    }

    async fn tweet_exists(&self, id: &str) -> Result<bool> {
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(tweets.contains_key(id))
    }

    async fn filter_existing_ids(&self, ids: &[&str]) -> Result<Vec<String>> {
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(ids
            .iter()
            .filter(|id| tweets.contains_key(**id))
            .map(|id| id.to_string())
            .collect())
    }

    async fn get_tweets_by_collection(
        &self,
        collection: &str,
        user_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let collections = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (collection.to_string(), user_id.to_string());
        let tweet_ids = collections.get(&key);

        if let Some(ids) = tweet_ids {
            let offset = offset.unwrap_or(0) as usize;
            let limit = limit.unwrap_or(100) as usize;

            let result: Vec<TweetData> = ids
                .iter()
                .skip(offset)
                .take(limit)
                .filter_map(|id| tweets.get(id).cloned())
                .collect();

            Ok(result)
        } else {
            Ok(Vec::new())
        }
    }

    async fn add_to_collection(
        &self,
        tweet_id: &str,
        collection: &str,
        user_id: &str,
    ) -> Result<()> {
        let mut collections = self
            .collections
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (collection.to_string(), user_id.to_string());
        collections
            .entry(key)
            .or_default()
            .insert(tweet_id.to_string());
        Ok(())
    }

    async fn is_in_collection(
        &self,
        tweet_id: &str,
        collection: &str,
        user_id: &str,
    ) -> Result<bool> {
        let collections = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (collection.to_string(), user_id.to_string());
        Ok(collections
            .get(&key)
            .map(|ids| ids.contains(tweet_id))
            .unwrap_or(false))
    }

    async fn collection_count(&self, collection: &str, user_id: &str) -> Result<u64> {
        let collections = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (collection.to_string(), user_id.to_string());
        Ok(collections
            .get(&key)
            .map(|ids| ids.len() as u64)
            .unwrap_or(0))
    }

    async fn get_tweets_by_collection_time_range(
        &self,
        collection: &str,
        user_id: &str,
        _start_time: chrono::DateTime<chrono::Utc>,
        _end_time: chrono::DateTime<chrono::Utc>,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        // Memory storage doesn't track added_at, so return all tweets in collection
        self.get_tweets_by_collection(collection, user_id, limit, None)
            .await
    }

    async fn get_tweets_missing_headlines(
        &self,
        min_length: usize,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let limit = limit.unwrap_or(100) as usize;

        let results: Vec<TweetData> = tweets
            .values()
            .filter(|t| t.headline.is_none() && t.text.chars().count() > min_length)
            .take(limit)
            .cloned()
            .collect();

        Ok(results)
    }

    async fn update_tweet_headlines(&self, headlines: &[(String, String)]) -> Result<usize> {
        let mut tweets = self
            .tweets
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut updated = 0;

        for (tweet_id, headline) in headlines {
            if let Some(tweet) = tweets.get_mut(tweet_id) {
                tweet.headline = Some(headline.clone());
                updated += 1;
            }
        }

        Ok(updated)
    }
}

#[async_trait]
impl SyncStateStore for MemoryStorage {
    async fn get_sync_state(&self, collection: &str, user_id: &str) -> Result<Option<SyncState>> {
        let states = self
            .sync_states
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (collection.to_string(), user_id.to_string());
        Ok(states.get(&key).cloned())
    }

    async fn update_sync_state(&self, state: &SyncState) -> Result<()> {
        let mut states = self
            .sync_states
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (state.collection.clone(), state.user_id.clone());
        states.insert(key, state.clone());
        Ok(())
    }

    async fn clear_sync_state(&self, collection: &str, user_id: &str) -> Result<()> {
        let mut states = self
            .sync_states
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (collection.to_string(), user_id.to_string());
        states.remove(&key);
        Ok(())
    }

    async fn get_all_sync_states(&self, user_id: &str) -> Result<Vec<SyncState>> {
        let states = self
            .sync_states
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(states
            .iter()
            .filter(|((_, uid), _)| uid == user_id)
            .map(|(_, state)| state.clone())
            .collect())
    }
}

#[async_trait]
impl UserStore for MemoryStorage {
    async fn upsert_user_from_mention(&self, user: &MentionedUser) -> Result<()> {
        let mut users = self
            .users
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let mut usernames = self
            .usernames
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        users.insert(user.id.clone(), user.clone());
        usernames.insert(user.username.to_lowercase(), user.id.clone());
        Ok(())
    }

    async fn get_user_by_username(&self, username: &str) -> Result<Option<MentionedUser>> {
        let usernames = self
            .usernames
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let users = self
            .users
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        if let Some(user_id) = usernames.get(&username.to_lowercase()) {
            Ok(users.get(user_id).cloned())
        } else {
            Ok(None)
        }
    }

    async fn get_user_by_id(&self, id: &str) -> Result<Option<MentionedUser>> {
        let users = self
            .users
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        Ok(users.get(id).cloned())
    }

    async fn get_tweets_mentioning_user(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let limit = limit.unwrap_or(20) as usize;

        let mut results = Vec::new();
        for tweet in tweets.values() {
            if tweet.mentions.iter().any(|mention| mention.id == user_id) {
                results.push(tweet.clone());
                if results.len() >= limit {
                    break;
                }
            }
        }

        Ok(results)
    }

    async fn get_tweets_replying_to_user(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<TweetData>> {
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let limit = limit.unwrap_or(20) as usize;

        let mut results = Vec::new();
        for tweet in tweets.values() {
            if tweet
                .in_reply_to_user_id
                .as_deref()
                .map(|id| id == user_id)
                .unwrap_or(false)
            {
                results.push(tweet.clone());
                if results.len() >= limit {
                    break;
                }
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_upsert_and_get_tweet() {
        let storage = MemoryStorage::new();
        let tweet = TweetData {
            id: "123".to_string(),
            text: "Hello world".to_string(),
            author: bird_core::TweetAuthor {
                username: "test".to_string(),
                name: "Test User".to_string(),
            },
            author_id: Some("456".to_string()),
            created_at: None,
            reply_count: None,
            retweet_count: None,
            like_count: None,
            conversation_id: None,
            in_reply_to_status_id: None,
            in_reply_to_user_id: None,
            mentions: Vec::new(),
            quoted_tweet: None,
            media: None,
            article: None,
            headline: None,
            _raw: None,
        };

        storage.upsert_tweet(&tweet).await.unwrap();
        let retrieved = storage.get_tweet("123").await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().text, "Hello world");
    }

    #[tokio::test]
    async fn test_collection() {
        let storage = MemoryStorage::new();
        let tweet = TweetData {
            id: "123".to_string(),
            text: "Hello world".to_string(),
            author: bird_core::TweetAuthor {
                username: "test".to_string(),
                name: "Test User".to_string(),
            },
            author_id: None,
            created_at: None,
            reply_count: None,
            retweet_count: None,
            like_count: None,
            conversation_id: None,
            in_reply_to_status_id: None,
            in_reply_to_user_id: None,
            mentions: Vec::new(),
            quoted_tweet: None,
            media: None,
            article: None,
            headline: None,
            _raw: None,
        };

        storage.upsert_tweet(&tweet).await.unwrap();
        storage
            .add_to_collection("123", "likes", "user1")
            .await
            .unwrap();

        assert!(storage
            .is_in_collection("123", "likes", "user1")
            .await
            .unwrap());
        assert!(!storage
            .is_in_collection("123", "bookmarks", "user1")
            .await
            .unwrap());

        let count = storage.collection_count("likes", "user1").await.unwrap();
        assert_eq!(count, 1);
    }

    #[tokio::test]
    async fn test_user_store_queries() {
        let storage = MemoryStorage::new();
        let user = MentionedUser {
            id: "u1".to_string(),
            username: "Alice".to_string(),
            name: Some("Alice A".to_string()),
        };
        storage.upsert_user_from_mention(&user).await.unwrap();

        let by_username = storage
            .get_user_by_username("alice")
            .await
            .unwrap()
            .expect("user by username");
        assert_eq!(by_username.id, "u1");

        let tweet = TweetData {
            id: "t1".to_string(),
            text: "Hello @Alice".to_string(),
            author: bird_core::TweetAuthor {
                username: "bob".to_string(),
                name: "Bob".to_string(),
            },
            author_id: Some("u2".to_string()),
            created_at: None,
            reply_count: None,
            retweet_count: None,
            like_count: None,
            conversation_id: None,
            in_reply_to_status_id: None,
            in_reply_to_user_id: Some("u1".to_string()),
            mentions: vec![user],
            quoted_tweet: None,
            media: None,
            article: None,
            headline: None,
            _raw: None,
        };

        storage.upsert_tweet(&tweet).await.unwrap();

        let mentions = storage
            .get_tweets_mentioning_user("u1", Some(10))
            .await
            .unwrap();
        assert_eq!(mentions.len(), 1);

        let replies = storage
            .get_tweets_replying_to_user("u1", Some(10))
            .await
            .unwrap();
        assert_eq!(replies.len(), 1);
    }
}

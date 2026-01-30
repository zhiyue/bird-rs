//! In-memory storage implementation for testing.

use async_trait::async_trait;
use bird_core::{
    Error, MentionedUser, ResonanceScore, ResonanceStore, Result, SyncState, SyncStateStore,
    TweetData, TweetStore, TweetWithCollections, UserStore,
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
    resonance_scores: RwLock<HashMap<(String, String), ResonanceScore>>, // (tweet_id, user_id) -> score
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
            resonance_scores: RwLock::new(HashMap::new()),
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

    async fn get_tweets_by_ids(&self, ids: &[&str]) -> Result<Vec<TweetData>> {
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(ids
            .iter()
            .filter_map(|id| tweets.get(*id).cloned())
            .collect())
    }

    async fn get_collection_tweet_ids(
        &self,
        collection: &str,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<String>> {
        let collections = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (collection.to_string(), user_id.to_string());
        let tweet_ids = collections.get(&key);

        if let Some(ids) = tweet_ids {
            let limit = limit.map(|l| l as usize).unwrap_or(usize::MAX);
            Ok(ids.iter().take(limit).cloned().collect())
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_user_reply_tweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>> {
        let collections = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = ("user_tweets".to_string(), user_id.to_string());
        let tweet_ids = collections.get(&key);

        let limit = limit.map(|l| l as usize).unwrap_or(usize::MAX);

        if let Some(ids) = tweet_ids {
            let results: Vec<(String, String)> = ids
                .iter()
                .filter_map(|id| {
                    tweets.get(id).and_then(|t| {
                        t.in_reply_to_status_id
                            .as_ref()
                            .map(|reply_to| (t.id.clone(), reply_to.clone()))
                    })
                })
                .take(limit)
                .collect();
            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_user_quote_tweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>> {
        let collections = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = ("user_tweets".to_string(), user_id.to_string());
        let tweet_ids = collections.get(&key);

        let limit = limit.map(|l| l as usize).unwrap_or(usize::MAX);

        if let Some(ids) = tweet_ids {
            let results: Vec<(String, String)> = ids
                .iter()
                .filter_map(|id| {
                    tweets.get(id).and_then(|t| {
                        t.quoted_tweet
                            .as_ref()
                            .map(|qt| (t.id.clone(), qt.id.clone()))
                    })
                })
                .take(limit)
                .collect();
            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_user_retweets(
        &self,
        user_id: &str,
        limit: Option<u32>,
    ) -> Result<Vec<(String, String)>> {
        let collections = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = ("user_tweets".to_string(), user_id.to_string());
        let tweet_ids = collections.get(&key);

        let limit = limit.map(|l| l as usize).unwrap_or(usize::MAX);

        if let Some(ids) = tweet_ids {
            let results: Vec<(String, String)> = ids
                .iter()
                .filter_map(|id| {
                    tweets.get(id).and_then(|t| {
                        t.retweeted_tweet
                            .as_ref()
                            .map(|rt| (t.id.clone(), rt.id.clone()))
                    })
                })
                .take(limit)
                .collect();
            Ok(results)
        } else {
            Ok(Vec::new())
        }
    }

    async fn get_tweets_interleaved(
        &self,
        collections: &[&str],
        user_id: &str,
        limit: Option<u32>,
        offset: Option<u32>,
    ) -> Result<Vec<TweetWithCollections>> {
        let collections_map = self
            .collections
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;
        let tweets = self
            .tweets
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let mut seen_ids = HashSet::new();
        let mut result = Vec::new();

        // Collect all tweets with their collection memberships
        for collection in collections {
            let key = (collection.to_string(), user_id.to_string());
            if let Some(ids) = collections_map.get(&key) {
                for id in ids {
                    if seen_ids.insert(id.clone()) {
                        if let Some(tweet) = tweets.get(id) {
                            result.push((id.clone(), tweet.clone(), vec![collection.to_string()]));
                        }
                    } else {
                        // Add collection to existing tweet
                        if let Some(entry) =
                            result.iter_mut().find(|(entry_id, _, _)| entry_id == id)
                        {
                            if !entry.2.contains(&collection.to_string()) {
                                entry.2.push(collection.to_string());
                            }
                        }
                    }
                }
            }
        }

        // Apply pagination
        let offset = offset.unwrap_or(0) as usize;
        let limit = limit.unwrap_or(100) as usize;

        let results: Vec<TweetWithCollections> = result
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, tweet, colls)| TweetWithCollections {
                tweet,
                collections: colls,
            })
            .collect();

        Ok(results)
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

    async fn get_any_synced_user_id(&self) -> Result<Option<String>> {
        let states = self
            .sync_states
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        Ok(states.keys().next().map(|(_, user_id)| user_id.clone()))
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

#[async_trait]
impl ResonanceStore for MemoryStorage {
    async fn get_resonance_score(
        &self,
        tweet_id: &str,
        user_id: &str,
    ) -> Result<Option<ResonanceScore>> {
        let scores = self
            .resonance_scores
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (tweet_id.to_string(), user_id.to_string());
        Ok(scores.get(&key).cloned())
    }

    async fn get_top_resonance_scores(
        &self,
        user_id: &str,
        limit: u32,
        offset: Option<u32>,
    ) -> Result<Vec<ResonanceScore>> {
        let scores = self
            .resonance_scores
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let offset = offset.unwrap_or(0) as usize;
        let limit = limit as usize;

        let mut user_scores: Vec<_> = scores
            .values()
            .filter(|s| s.user_id == user_id)
            .cloned()
            .collect();

        // Sort by total descending
        user_scores.sort_by(|a, b| {
            b.total
                .partial_cmp(&a.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        Ok(user_scores.into_iter().skip(offset).take(limit).collect())
    }

    async fn upsert_resonance_score(&self, score: &ResonanceScore) -> Result<()> {
        let mut scores = self
            .resonance_scores
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let key = (score.tweet_id.clone(), score.user_id.clone());
        scores.insert(key, score.clone());
        Ok(())
    }

    async fn upsert_resonance_scores(&self, scores: &[ResonanceScore]) -> Result<usize> {
        let mut storage = self
            .resonance_scores
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        for score in scores {
            let key = (score.tweet_id.clone(), score.user_id.clone());
            storage.insert(key, score.clone());
        }

        Ok(scores.len())
    }

    async fn clear_resonance_scores(&self, user_id: &str) -> Result<u64> {
        let mut scores = self
            .resonance_scores
            .write()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let keys_to_remove: Vec<_> = scores
            .keys()
            .filter(|(_, uid)| uid == user_id)
            .cloned()
            .collect();

        let count = keys_to_remove.len() as u64;
        for key in keys_to_remove {
            scores.remove(&key);
        }

        Ok(count)
    }

    async fn resonance_score_count(&self, user_id: &str) -> Result<u64> {
        let scores = self
            .resonance_scores
            .read()
            .map_err(|e| Error::Storage(e.to_string()))?;

        let count = scores.values().filter(|s| s.user_id == user_id).count() as u64;
        Ok(count)
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
            retweeted_tweet: None,
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
            retweeted_tweet: None,
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
            retweeted_tweet: None,
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

    #[tokio::test]
    async fn test_get_tweets_interleaved() {
        let storage = MemoryStorage::new();
        let user_id = "user1";

        // Create 3 tweets
        let tweet1 = TweetData {
            id: "1".to_string(),
            text: "Tweet 1".to_string(),
            author: bird_core::TweetAuthor {
                username: "test".to_string(),
                name: "Test".to_string(),
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
            retweeted_tweet: None,
            media: None,
            article: None,
            headline: None,
            _raw: None,
        };

        let tweet2 = TweetData {
            id: "2".to_string(),
            text: "Tweet 2".to_string(),
            ..tweet1.clone()
        };

        let tweet3 = TweetData {
            id: "3".to_string(),
            text: "Tweet 3".to_string(),
            ..tweet1.clone()
        };

        storage.upsert_tweet(&tweet1).await.unwrap();
        storage.upsert_tweet(&tweet2).await.unwrap();
        storage.upsert_tweet(&tweet3).await.unwrap();

        // Add tweets to different collections
        storage
            .add_to_collection("1", "likes", user_id)
            .await
            .unwrap();
        storage
            .add_to_collection("2", "bookmarks", user_id)
            .await
            .unwrap();
        storage
            .add_to_collection("3", "likes", user_id)
            .await
            .unwrap();
        storage
            .add_to_collection("3", "bookmarks", user_id)
            .await
            .unwrap();

        // Get interleaved tweets
        let result = storage
            .get_tweets_interleaved(&["likes", "bookmarks"], user_id, None, None)
            .await
            .unwrap();

        // Should get 3 unique tweets
        assert_eq!(result.len(), 3);

        // Check collections are properly associated
        let tweet_ids: Vec<&str> = result.iter().map(|t| t.tweet.id.as_str()).collect();
        assert!(tweet_ids.contains(&"1"));
        assert!(tweet_ids.contains(&"2"));
        assert!(tweet_ids.contains(&"3"));

        // Check tweet 3 has both collections
        let tweet3_result = result.iter().find(|t| t.tweet.id == "3").unwrap();
        assert_eq!(tweet3_result.collections.len(), 2);
        assert!(tweet3_result.collections.contains(&"likes".to_string()));
        assert!(tweet3_result.collections.contains(&"bookmarks".to_string()));
    }
}

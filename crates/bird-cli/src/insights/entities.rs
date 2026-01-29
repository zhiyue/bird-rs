//! Entity types extracted from tweet insights.

use serde::{Deserialize, Serialize};

/// Category of a tool or technology.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCategory {
    Language,
    Framework,
    Library,
    Database,
    DevTool,
    AiTool,
    Platform,
    Service,
    Other,
}

impl std::fmt::Display for ToolCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ToolCategory::Language => write!(f, "Language"),
            ToolCategory::Framework => write!(f, "Framework"),
            ToolCategory::Library => write!(f, "Library"),
            ToolCategory::Database => write!(f, "Database"),
            ToolCategory::DevTool => write!(f, "DevTool"),
            ToolCategory::AiTool => write!(f, "AI Tool"),
            ToolCategory::Platform => write!(f, "Platform"),
            ToolCategory::Service => write!(f, "Service"),
            ToolCategory::Other => write!(f, "Other"),
        }
    }
}

/// A tool or technology discovered in tweets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEntity {
    /// Name of the tool.
    pub name: String,
    /// Category of the tool.
    pub category: ToolCategory,
    /// Brief description (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A topic or theme discovered in tweets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TopicEntity {
    /// Name of the topic.
    pub name: String,
    /// Brief description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A technical concept worth remembering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConceptEntity {
    /// Name of the concept.
    pub name: String,
    /// Brief explanation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
}

/// A notable person mentioned in tweets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersonEntity {
    /// Name or handle.
    pub name: String,
    /// Twitter handle (without @).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    /// Why they're notable in context.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// Type of resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResourceType {
    Article,
    Repository,
    Documentation,
    Video,
    Thread,
    Paper,
    Tutorial,
    Other,
}

impl std::fmt::Display for ResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ResourceType::Article => write!(f, "Article"),
            ResourceType::Repository => write!(f, "Repository"),
            ResourceType::Documentation => write!(f, "Documentation"),
            ResourceType::Video => write!(f, "Video"),
            ResourceType::Thread => write!(f, "Thread"),
            ResourceType::Paper => write!(f, "Paper"),
            ResourceType::Tutorial => write!(f, "Tutorial"),
            ResourceType::Other => write!(f, "Other"),
        }
    }
}

/// A resource (article, repo, docs, etc.) shared in tweets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceEntity {
    /// Title or name of the resource.
    pub title: String,
    /// Type of resource.
    pub resource_type: ResourceType,
    /// URL if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// Brief description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// Complete insights result from analyzing tweets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct InsightsResult {
    /// High-level summary of the insights.
    pub summary: String,
    /// Number of tweets analyzed.
    pub tweets_analyzed: usize,
    /// Tools and technologies discovered.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tools: Vec<ToolEntity>,
    /// Topics and themes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub topics: Vec<TopicEntity>,
    /// Technical concepts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub concepts: Vec<ConceptEntity>,
    /// Notable people.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub people: Vec<PersonEntity>,
    /// Resources shared.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub resources: Vec<ResourceEntity>,
    /// Overall themes/tags.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub themes: Vec<String>,
}


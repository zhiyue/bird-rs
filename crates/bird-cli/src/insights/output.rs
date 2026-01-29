//! Output formatting for insights.

use super::entities::InsightsResult;
use super::TimePeriod;
use colored::Colorize;

/// Format insights for terminal output.
pub fn format_insights(result: &InsightsResult, period: TimePeriod, show_emoji: bool) -> String {
    let mut output = String::new();

    // Header
    let icon = if show_emoji { "  " } else { "" };
    output.push_str(&format!(
        "{}{}",
        icon,
        format!("Insights for {}", period.description()).bold()
    ));
    output.push('\n');
    output.push_str(&"─".repeat(60).dimmed().to_string());
    output.push('\n');
    output.push('\n');

    // Summary
    if !result.summary.is_empty() {
        output.push_str(&result.summary);
        output.push('\n');
        output.push('\n');
    }

    // Tweet count
    output.push_str(
        &format!("{} tweets analyzed", result.tweets_analyzed)
            .dimmed()
            .to_string(),
    );
    output.push('\n');
    output.push('\n');

    // Tools & Technologies
    if !result.tools.is_empty() {
        let icon = if show_emoji { "   " } else { "" };
        output.push_str(&format!("{}{}\n", icon, "Tools & Technologies".bold()));
        for tool in &result.tools {
            let category = format!("[{}]", tool.category).dimmed();
            output.push_str(&format!("  {} {}\n", tool.name.cyan(), category));
        }
        output.push('\n');
    }

    // Topics
    if !result.topics.is_empty() {
        let icon = if show_emoji { "  " } else { "" };
        output.push_str(&format!("{}{}\n", icon, "Topics".bold()));
        for topic in &result.topics {
            output.push_str(&format!("  {}\n", topic.name));
        }
        output.push('\n');
    }

    // Concepts
    if !result.concepts.is_empty() {
        let icon = if show_emoji { "  " } else { "" };
        output.push_str(&format!("{}{}\n", icon, "Concepts".bold()));
        for concept in &result.concepts {
            if let Some(explanation) = &concept.explanation {
                output.push_str(&format!(
                    "  {} ({})\n",
                    concept.name.yellow(),
                    explanation.dimmed()
                ));
            } else {
                output.push_str(&format!("  {}\n", concept.name.yellow()));
            }
        }
        output.push('\n');
    }

    // People
    if !result.people.is_empty() {
        let icon = if show_emoji { "  " } else { "" };
        output.push_str(&format!("{}{}\n", icon, "People".bold()));
        for person in &result.people {
            if let Some(handle) = &person.handle {
                output.push_str(&format!("  @{}\n", handle.cyan()));
            } else {
                output.push_str(&format!("  {}\n", person.name));
            }
        }
        output.push('\n');
    }

    // Resources
    if !result.resources.is_empty() {
        let icon = if show_emoji { "  " } else { "" };
        output.push_str(&format!("{}{}\n", icon, "Resources".bold()));
        for resource in &result.resources {
            let resource_type = format!("[{}]", resource.resource_type).dimmed();
            output.push_str(&format!("  {} {}\n", resource.title, resource_type));
            if let Some(url) = &resource.url {
                output.push_str(&format!("    {}\n", url.blue().underline()));
            }
        }
        output.push('\n');
    }

    // Themes
    if !result.themes.is_empty() {
        let icon = if show_emoji { "  " } else { "" };
        let themes_str = result.themes.join(", ");
        output.push_str(&format!(
            "{}{}: {}\n",
            icon,
            "Themes".bold(),
            themes_str.green()
        ));
    }

    output
}

/// Format insights as JSON.
pub fn format_insights_json(result: &InsightsResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|_| "{}".to_string())
}

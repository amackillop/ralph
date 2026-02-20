//! Notification system for Ralph loop events.
//!
//! Supports webhook POST, desktop notifications, and sound alerts
//! for loop completion and error events.

use anyhow::Result;
use chrono::Utc;
use serde_json::json;
use std::process::Command;
use tracing::{debug, warn};

use crate::config::NotificationConfig;

/// Notification event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotificationEvent {
    /// Loop completed successfully.
    Complete,
    /// Loop encountered an error.
    Error,
}

/// Sends notifications based on configuration.
pub(crate) struct Notifier {
    config: NotificationConfig,
}

impl Notifier {
    /// Create a new notifier from configuration.
    pub fn new(config: NotificationConfig) -> Self {
        Self { config }
    }

    /// Send notification for an event.
    ///
    /// This is a fire-and-forget operation - errors are logged but don't
    /// affect the main loop execution.
    pub async fn notify(&self, event: NotificationEvent, details: &NotificationDetails) {
        match event {
            NotificationEvent::Complete => {
                self.notify_complete(details).await;
            }
            NotificationEvent::Error => {
                self.notify_error(details).await;
            }
        }
    }

    /// Send completion notification.
    async fn notify_complete(&self, details: &NotificationDetails) {
        if let Some(ref value) = self.config.on_complete {
            self.send_notification(value, "complete", "Ralph Loop Complete", details)
                .await;
        }
    }

    /// Send error notification.
    async fn notify_error(&self, details: &NotificationDetails) {
        if let Some(ref value) = self.config.on_error {
            self.send_notification(value, "error", "Ralph Loop Error", details)
                .await;
        }
    }

    /// Parse notification value and send appropriate notification.
    ///
    /// Supports:
    /// - `"webhook:<url>"` - POST to webhook
    /// - `"desktop"` - Desktop notification
    /// - `"sound"` - Sound alert
    /// - Bare URL (backward compat) - Treated as webhook
    async fn send_notification(
        &self,
        value: &str,
        event_type: &str,
        title: &str,
        details: &NotificationDetails,
    ) {
        if value.starts_with("webhook:") {
            let url = value.strip_prefix("webhook:").unwrap_or("");
            if !url.is_empty() {
                if let Err(e) = self.send_webhook(url, event_type, details).await {
                    warn!("Failed to send {} webhook: {}", event_type, e);
                }
            }
        } else if value == "desktop" {
            if let Err(e) = send_desktop_notification(title, &details.message) {
                warn!("Failed to send desktop notification: {}", e);
            }
        } else if value == "sound" {
            play_sound();
        } else if value == "none" {
            // Explicitly disabled
        } else if value.starts_with("http://") || value.starts_with("https://") {
            // Backward compatibility: bare URL treated as webhook
            if let Err(e) = self.send_webhook(value, event_type, details).await {
                warn!("Failed to send {} webhook: {}", event_type, e);
            }
        }
    }

    /// Send webhook POST request with exponential backoff retry.
    ///
    /// Retries up to 3 times with delays of 1s, 2s, 4s on transient failures.
    #[allow(tail_expr_drop_order)] // Drop order changes are harmless for HTTP responses
    async fn send_webhook(
        &self,
        url: &str,
        event_type: &str,
        details: &NotificationDetails,
    ) -> Result<()> {
        let payload = json!({
            "event": event_type,
            "iteration": details.iteration,
            "message": details.message,
            "timestamp": details.timestamp,
            "context": details.context,
        });

        debug!("Sending webhook to {}: {:?}", url, payload);

        let client = reqwest::Client::new();
        let max_attempts = 3;
        let mut last_error = None;

        for attempt in 0..max_attempts {
            if attempt > 0 {
                let delay_secs = 1u64 << attempt; // 2, 4 seconds for attempts 1, 2
                debug!(
                    "Webhook retry attempt {} after {}s delay",
                    attempt + 1,
                    delay_secs
                );
                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
            }

            match client.post(url).json(&payload).send().await {
                Ok(response) => {
                    if response.status().is_success() {
                        debug!("Webhook sent successfully");
                        return Ok(());
                    }

                    let status = response.status();
                    let body = response.text().await.unwrap_or_default();

                    // Retry on 5xx server errors and 429 rate limit
                    if status.is_server_error() || status.as_u16() == 429 {
                        last_error = Some(format!("Webhook returned {status}: {body}"));
                        continue;
                    }

                    // Don't retry client errors (4xx except 429)
                    anyhow::bail!("Webhook returned error status {status}: {body}");
                }
                Err(e) => {
                    // Retry on network errors
                    last_error = Some(e.to_string());
                }
            }
        }

        anyhow::bail!(
            "Webhook failed after {max_attempts} attempts: {}",
            last_error.unwrap_or_else(|| "unknown error".to_string())
        )
    }
}

/// Send desktop notification (cross-platform).
fn send_desktop_notification(title: &str, body: &str) -> Result<()> {
    // Try notify-send (Linux) first
    if Command::new("notify-send")
        .args([title, body])
        .output()
        .is_ok()
    {
        return Ok(());
    }

    // Try osascript (macOS)
    if Command::new("osascript")
        .args([
            "-e",
            &format!(
                "display notification \"{}\" with title \"{}\"",
                body.replace('"', "\\\""),
                title.replace('"', "\\\"")
            ),
        ])
        .output()
        .is_ok()
    {
        return Ok(());
    }

    // Try growlnotify (macOS alternative)
    if Command::new("growlnotify")
        .args(["-t", title, "-m", body])
        .output()
        .is_ok()
    {
        return Ok(());
    }

    anyhow::bail!(
        "No desktop notification command available (tried notify-send, osascript, growlnotify)"
    );
}

/// Play sound alert (cross-platform).
/// Always succeeds (falls back to bell character if no sound command available).
fn play_sound() {
    // Try paplay (Linux PulseAudio)
    if Command::new("paplay")
        .args(["/usr/share/sounds/freedesktop/stereo/complete.oga"])
        .output()
        .is_ok()
    {
        return;
    }

    // Try aplay (Linux ALSA)
    if Command::new("aplay")
        .args(["/usr/share/sounds/alsa/Front_Left.wav"])
        .output()
        .is_ok()
    {
        return;
    }

    // Try afplay (macOS)
    if Command::new("afplay")
        .args(["/System/Library/Sounds/Glass.aiff"])
        .output()
        .is_ok()
    {
        return;
    }

    // Try beep (Linux, if available)
    if Command::new("beep").output().is_ok() {
        return;
    }

    // Try printf bell character as fallback (always succeeds)
    print!("\x07");
}

/// Details for a notification event.
#[derive(Debug, Clone)]
pub(crate) struct NotificationDetails {
    /// Iteration number (if applicable).
    pub iteration: Option<u32>,
    /// Message describing the event.
    pub message: String,
    /// Timestamp of the event.
    pub timestamp: String,
    /// Optional additional context.
    pub context: Option<serde_json::Value>,
}

impl NotificationDetails {
    /// Create details for a completion event.
    pub fn complete(iteration: u32, total_iterations: u32, reason: &str) -> Self {
        Self {
            iteration: Some(iteration),
            message: format!("Loop completed after {total_iterations} iterations: {reason}"),
            timestamp: Utc::now().to_rfc3339(),
            context: Some(json!({
                "total_iterations": total_iterations,
                "reason": reason,
            })),
        }
    }

    /// Create details for an error event.
    pub fn error(iteration: Option<u32>, error: &str, context: Option<serde_json::Value>) -> Self {
        Self {
            iteration,
            message: error.to_string(),
            timestamp: Utc::now().to_rfc3339(),
            context,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_notification_details_complete() {
        let details = NotificationDetails::complete(5, 10, "completion_detected");
        assert_eq!(details.iteration, Some(5));
        assert!(details.message.contains("completed"));
        assert!(details.message.contains("10"));
        assert!(details.context.is_some());
    }

    #[test]
    fn test_notification_details_error() {
        let details = NotificationDetails::error(Some(3), "Test error", None);
        assert_eq!(details.iteration, Some(3));
        assert_eq!(details.message, "Test error");
    }

    #[test]
    fn test_notification_details_error_with_context() {
        let ctx = json!({"key": "value"});
        let details = NotificationDetails::error(None, "err", Some(ctx.clone()));
        assert_eq!(details.iteration, None);
        assert_eq!(details.context, Some(ctx));
    }

    #[test]
    fn test_notifier_creation() {
        let config = NotificationConfig::default();
        let _notifier = Notifier::new(config);
        // Just verify it can be created
        // Test passes if we reach here
    }

    #[test]
    fn test_notification_config_parse_webhook() {
        let config = NotificationConfig {
            on_complete: Some("https://example.com/webhook".to_string()),
            on_error: Some("webhook:https://example.com/error".to_string()),
        };
        assert_eq!(
            config.on_complete,
            Some("https://example.com/webhook".to_string())
        );
        assert_eq!(
            config.on_error,
            Some("webhook:https://example.com/error".to_string())
        );
    }

    #[test]
    fn test_notification_config_parse_desktop() {
        let config = NotificationConfig {
            on_complete: None,
            on_error: Some("desktop".to_string()),
        };
        assert_eq!(config.on_error, Some("desktop".to_string()));
    }

    #[test]
    fn test_notification_config_parse_sound() {
        let config = NotificationConfig {
            on_complete: None,
            on_error: Some("sound".to_string()),
        };
        assert_eq!(config.on_error, Some("sound".to_string()));
    }

    #[test]
    fn test_notification_event_equality() {
        assert_eq!(NotificationEvent::Complete, NotificationEvent::Complete);
        assert_eq!(NotificationEvent::Error, NotificationEvent::Error);
        assert_ne!(NotificationEvent::Complete, NotificationEvent::Error);
    }

    #[test]
    fn test_notification_event_debug() {
        assert_eq!(format!("{:?}", NotificationEvent::Complete), "Complete");
        assert_eq!(format!("{:?}", NotificationEvent::Error), "Error");
    }

    #[test]
    fn test_notification_event_clone() {
        let event = NotificationEvent::Complete;
        let cloned = event;
        assert_eq!(event, cloned);
    }

    #[test]
    fn test_notification_details_clone() {
        let details = NotificationDetails::complete(1, 2, "test");
        let cloned = details.clone();
        assert_eq!(details.iteration, cloned.iteration);
        assert_eq!(details.message, cloned.message);
    }

    #[tokio::test]
    async fn test_notifier_notify_complete_no_config() {
        // No on_complete configured - should just return without error
        let config = NotificationConfig::default();
        let notifier = Notifier::new(config);
        let details = NotificationDetails::complete(1, 1, "done");
        notifier.notify(NotificationEvent::Complete, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_error_no_config() {
        // No on_error configured - should just return without error
        let config = NotificationConfig::default();
        let notifier = Notifier::new(config);
        let details = NotificationDetails::error(Some(1), "err", None);
        notifier.notify(NotificationEvent::Error, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_error_empty_webhook() {
        // webhook: prefix but empty URL
        let config = NotificationConfig {
            on_complete: None,
            on_error: Some("webhook:".to_string()),
        };
        let notifier = Notifier::new(config);
        let details = NotificationDetails::error(Some(1), "err", None);
        // Should handle empty URL gracefully
        notifier.notify(NotificationEvent::Error, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_error_sound() {
        // Sound notification - fires and forgets
        let config = NotificationConfig {
            on_complete: None,
            on_error: Some("sound".to_string()),
        };
        let notifier = Notifier::new(config);
        let details = NotificationDetails::error(Some(1), "err", None);
        notifier.notify(NotificationEvent::Error, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_error_desktop() {
        // Desktop notification - may fail but shouldn't panic
        let config = NotificationConfig {
            on_complete: None,
            on_error: Some("desktop".to_string()),
        };
        let notifier = Notifier::new(config);
        let details = NotificationDetails::error(Some(1), "err", None);
        notifier.notify(NotificationEvent::Error, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_complete_desktop() {
        // Desktop notification on complete - may fail but shouldn't panic
        let config = NotificationConfig {
            on_complete: Some("desktop".to_string()),
            on_error: None,
        };
        let notifier = Notifier::new(config);
        let details = NotificationDetails::complete(1, 1, "done");
        notifier.notify(NotificationEvent::Complete, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_complete_sound() {
        // Sound notification on complete - fires and forgets
        let config = NotificationConfig {
            on_complete: Some("sound".to_string()),
            on_error: None,
        };
        let notifier = Notifier::new(config);
        let details = NotificationDetails::complete(1, 1, "done");
        notifier.notify(NotificationEvent::Complete, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_complete_webhook_prefixed() {
        // webhook: prefix on complete
        let config = NotificationConfig {
            on_complete: Some("webhook:".to_string()),
            on_error: None,
        };
        let notifier = Notifier::new(config);
        let details = NotificationDetails::complete(1, 1, "done");
        // Empty URL should be handled gracefully
        notifier.notify(NotificationEvent::Complete, &details).await;
    }

    #[tokio::test]
    async fn test_notifier_notify_complete_none() {
        // Explicit "none" disables notification
        let config = NotificationConfig {
            on_complete: Some("none".to_string()),
            on_error: None,
        };
        let notifier = Notifier::new(config);
        let details = NotificationDetails::complete(1, 1, "done");
        notifier.notify(NotificationEvent::Complete, &details).await;
    }

    #[test]
    fn test_play_sound_fallback() {
        // play_sound always succeeds (falls back to bell character)
        play_sound();
    }

    #[test]
    fn test_send_desktop_notification_not_available() {
        // May fail but should return an error, not panic
        let result = send_desktop_notification("Test", "Body");
        // Result depends on system; just verify it doesn't panic
        let _ = result;
    }
}

//! Completion detection for Ralph loops.
//!
//! Detects when a loop should complete by looking for completion promise
//! phrases in agent output or `IMPLEMENTATION_PLAN.md`.

use anyhow::Result;
use std::path::Path;
use tracing::debug;

/// Detects when a Ralph loop should complete based on promise phrases.
///
/// Looks for `<promise>PHRASE</promise>` tags in agent output and project files.
pub(crate) struct CompletionDetector {
    /// The completion promise phrase to look for.
    promise: Option<String>,
}

impl CompletionDetector {
    /// Create a new completion detector with an optional promise phrase.
    pub fn new(promise: Option<&str>) -> Self {
        Self {
            promise: promise.map(String::from),
        }
    }

    /// Check if the loop should complete based on output and file changes
    pub fn is_complete(&self, output: &str, project_dir: &Path) -> Result<bool> {
        // Check for completion promise in output
        if let Some(ref promise) = self.promise {
            if Self::check_promise_in_text(output, promise) {
                debug!("Found completion promise in output");
                return Ok(true);
            }

            // Also check IMPLEMENTATION_PLAN.md for completion markers
            let plan_path = project_dir.join("IMPLEMENTATION_PLAN.md");
            if plan_path.exists() {
                let plan_content = std::fs::read_to_string(&plan_path)?;
                if Self::check_promise_in_text(&plan_content, promise) {
                    debug!("Found completion promise in IMPLEMENTATION_PLAN.md");
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    /// Check if the promise text appears in <promise> tags
    fn check_promise_in_text(text: &str, promise: &str) -> bool {
        // Look for <promise>PROMISE_TEXT</promise> pattern
        let pattern = format!("<promise>{promise}</promise>");
        if text.contains(&pattern) {
            return true;
        }

        // Also try with whitespace normalization
        if let Some(extracted) = Self::extract_promise_content(text) {
            let normalized = extracted.split_whitespace().collect::<Vec<_>>().join(" ");
            let promise_normalized = promise.split_whitespace().collect::<Vec<_>>().join(" ");
            if normalized == promise_normalized {
                return true;
            }
        }

        false
    }

    /// Extract content from <promise>...</promise> tags
    fn extract_promise_content(text: &str) -> Option<String> {
        let start_tag = "<promise>";
        let end_tag = "</promise>";

        let start_idx = text.find(start_tag)?;
        let content_start = start_idx + start_tag.len();
        let end_idx = text[content_start..].find(end_tag)?;

        Some(
            text[content_start..content_start + end_idx]
                .trim()
                .to_string(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_detect_promise_exact() {
        let detector = CompletionDetector::new(Some("DONE"));
        let output = "Task completed. <promise>DONE</promise>";
        let dir = tempdir().unwrap();

        assert!(detector.is_complete(output, dir.path()).unwrap());
    }

    #[test]
    fn test_detect_promise_with_whitespace() {
        let detector = CompletionDetector::new(Some("All tests passing"));
        let output = "Result: <promise>All tests passing</promise>";
        let dir = tempdir().unwrap();

        assert!(detector.is_complete(output, dir.path()).unwrap());
    }

    #[test]
    fn test_no_promise_no_completion() {
        let detector = CompletionDetector::new(Some("DONE"));
        let output = "Still working on it...";
        let dir = tempdir().unwrap();

        assert!(!detector.is_complete(output, dir.path()).unwrap());
    }

    #[test]
    fn test_no_promise_configured() {
        let detector = CompletionDetector::new(None);
        let output = "Output with <promise>DONE</promise>";
        let dir = tempdir().unwrap();

        // Without a promise configured, we never auto-complete
        assert!(!detector.is_complete(output, dir.path()).unwrap());
    }

    #[test]
    fn test_extract_promise_content() {
        assert_eq!(
            CompletionDetector::extract_promise_content("foo <promise>DONE</promise> bar"),
            Some("DONE".to_string())
        );

        assert_eq!(
            CompletionDetector::extract_promise_content("no promise here"),
            None
        );
    }
}

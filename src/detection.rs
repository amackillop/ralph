//! Completion detection for Ralph loops.
//!
//! Detects when a loop should complete based on agent activity:
//! validation passes and the agent stops making changes (no new commits).

use std::path::Path;
use tracing::debug;

/// Detects when a Ralph loop should complete based on agent idleness.
///
/// The agent is considered "done" when:
/// - Validation passes (no errors)
/// - No new commits are created for `idle_threshold` consecutive iterations
#[derive(Debug)]
pub(crate) struct CompletionDetector {
    /// Last known commit hash.
    last_commit: Option<String>,
    /// Consecutive iterations with no changes (and validation passing).
    idle_count: u32,
    /// Number of idle iterations before considering complete.
    idle_threshold: u32,
}

impl CompletionDetector {
    /// Create a new completion detector with the given idle threshold.
    pub fn new(idle_threshold: u32) -> Self {
        Self {
            last_commit: None,
            idle_count: 0,
            idle_threshold,
        }
    }

    /// Record the current commit hash at the start of an iteration.
    pub fn record_commit(&mut self, commit_hash: Option<String>) {
        if self.last_commit.is_none() {
            // First iteration - just record, don't compare
            self.last_commit = commit_hash;
        }
    }

    /// Check if the loop should complete.
    ///
    /// Call this after validation passes. Compares current commit to last known.
    /// Returns true if agent has been idle for `idle_threshold` iterations.
    pub fn check_completion(&mut self, current_commit: Option<&str>) -> bool {
        let changed = match (&self.last_commit, current_commit) {
            (Some(last), Some(current)) => last != current,
            (None, Some(_)) => true,           // First commit
            (Some(_) | None, None) => false,   // No commit info, assume no change
        };

        if changed {
            debug!(
                "Commit changed: {:?} -> {:?}, resetting idle count",
                self.last_commit, current_commit
            );
            self.idle_count = 0;
            self.last_commit = current_commit.map(String::from);
        } else {
            self.idle_count += 1;
            debug!(
                "No commit change, idle count: {}/{}",
                self.idle_count, self.idle_threshold
            );
        }

        self.idle_count >= self.idle_threshold
    }

    /// Get current idle count (for display/logging).
    pub fn idle_count(&self) -> u32 {
        self.idle_count
    }
}

/// Get current git HEAD commit hash.
pub(crate) async fn get_commit_hash(project_dir: &Path) -> Option<String> {
    let output = tokio::process::Command::new("git")
        .current_dir(project_dir)
        .args(["rev-parse", "HEAD"])
        .output()
        .await
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let hash = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if hash.is_empty() {
        None
    } else {
        Some(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEFAULT_THRESHOLD: u32 = 2;

    #[test]
    fn test_first_iteration_records_commit() {
        let mut detector = CompletionDetector::new(DEFAULT_THRESHOLD);
        detector.record_commit(Some("abc123".to_string()));

        assert_eq!(detector.last_commit, Some("abc123".to_string()));
        assert_eq!(detector.idle_count, 0);
    }

    #[test]
    fn test_commit_change_resets_idle() {
        let mut detector = CompletionDetector::new(DEFAULT_THRESHOLD);
        detector.record_commit(Some("abc123".to_string()));

        // First check - different commit
        assert!(!detector.check_completion(Some("def456")));
        assert_eq!(detector.idle_count, 0);
        assert_eq!(detector.last_commit, Some("def456".to_string()));
    }

    #[test]
    fn test_no_change_increments_idle() {
        let mut detector = CompletionDetector::new(DEFAULT_THRESHOLD);
        detector.record_commit(Some("abc123".to_string()));

        // Same commit
        assert!(!detector.check_completion(Some("abc123")));
        assert_eq!(detector.idle_count, 1);

        // Still same commit
        assert!(detector.check_completion(Some("abc123")));
        assert_eq!(detector.idle_count, 2);
    }

    #[test]
    fn test_idle_threshold_triggers_completion() {
        let threshold = 3;
        let mut detector = CompletionDetector::new(threshold);
        detector.record_commit(Some("abc123".to_string()));

        for i in 0..threshold {
            let complete = detector.check_completion(Some("abc123"));
            if i + 1 >= threshold {
                assert!(complete, "Should complete after {} idles", i + 1);
            } else {
                assert!(!complete, "Should not complete after {} idles", i + 1);
            }
        }
    }

    #[test]
    fn test_change_after_idle_resets() {
        let mut detector = CompletionDetector::new(DEFAULT_THRESHOLD);
        detector.record_commit(Some("abc123".to_string()));

        // Build up idle count
        detector.check_completion(Some("abc123"));
        assert_eq!(detector.idle_count, 1);

        // New commit resets
        detector.check_completion(Some("def456"));
        assert_eq!(detector.idle_count, 0);
    }

    #[test]
    fn test_no_commits_stays_idle() {
        let mut detector = CompletionDetector::new(DEFAULT_THRESHOLD);
        detector.record_commit(None);

        assert!(!detector.check_completion(None));
        assert_eq!(detector.idle_count, 1);

        assert!(detector.check_completion(None));
        assert_eq!(detector.idle_count, 2);
    }
}

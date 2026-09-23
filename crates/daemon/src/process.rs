use std::fs;
use std::time::{Duration, Instant};
use tracing::{debug, info};

/// Safe, non-invasive process detector for Linux systems using /proc.
pub struct ProcessDetector {
    /// Process names to look for (e.g., "cs2", "cs2.exe")
    target_names: Vec<String>,
    /// Debounce delay after process exits before notifying
    debounce_duration: Duration,
    /// Last time the process was observed running
    last_seen_running: Option<Instant>,
    /// Whether the process was considered running in the previous cycle
    was_running: bool,
}

impl ProcessDetector {
    pub fn new_cs2(debounce_seconds: u64) -> Self {
        Self {
            target_names: vec!["cs2".to_string(), "cs2.exe".to_string()],
            debounce_duration: Duration::from_secs(debounce_seconds),
            last_seen_running: None,
            was_running: false,
        }
    }

    /// Set custom debounce duration
    #[allow(dead_code)]
    pub fn set_debounce_duration(&mut self, duration: Duration) {
        self.debounce_duration = duration;
    }

    /// Scan /proc to determine if any real game process is actively running.
    pub fn is_process_running(&self) -> bool {
        let my_pid = std::process::id();
        let proc_dir = match fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return false,
        };

        for entry in proc_dir.flatten() {
            let name = entry.file_name();
            let name_str = match name.to_str() {
                Some(s) => s,
                None => continue,
            };

            // Check if directory name is numeric (PID)
            let pid: u32 = match name_str.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };

            // Ignore our own daemon process
            if pid == my_pid {
                continue;
            }

            let path = entry.path();

            // Method 1: Check /proc/[pid]/comm
            let comm_path = path.join("comm");
            if let Ok(comm) = fs::read_to_string(comm_path) {
                let trimmed = comm.trim();
                for target in &self.target_names {
                    if trimmed.eq_ignore_ascii_case(target) {
                        return true;
                    }
                }
            }

            // Method 2: Check /proc/[pid]/exe symlink
            let exe_path = path.join("exe");
            if let Ok(target_path) = fs::read_link(exe_path) {
                if let Some(file_name) = target_path.file_name().and_then(|f| f.to_str()) {
                    for target in &self.target_names {
                        if file_name.eq_ignore_ascii_case(target) {
                            return true;
                        }
                    }
                }
            }
        }

        false
    }

    /// Update detection state and return whether the process is currently considered active,
    /// taking the debounce delay into account.
    ///
    /// Returns `(is_active, just_started, just_exited)`
    pub fn update(&mut self) -> (bool, bool, bool) {
        let raw_running = self.is_process_running();
        let now = Instant::now();

        let mut just_started = false;
        let mut just_exited = false;

        if raw_running {
            self.last_seen_running = Some(now);
            if !self.was_running {
                info!("Real game process detected running!");
                self.was_running = true;
                just_started = true;
            }
        } else if self.was_running {
            if let Some(last_seen) = self.last_seen_running {
                let elapsed = now.duration_since(last_seen);
                if elapsed >= self.debounce_duration {
                    info!(
                        "Real game process has exited (debounced for {:.1}s)",
                        elapsed.as_secs_f32()
                    );
                    self.was_running = false;
                    just_exited = true;
                    self.last_seen_running = None;
                } else {
                    debug!(
                        "Real game process not seen for {:.1}s, awaiting debounce ({:.1}s)",
                        elapsed.as_secs_f32(),
                        self.debounce_duration.as_secs_f32()
                    );
                }
            } else {
                self.was_running = false;
                just_exited = true;
            }
        }

        (self.was_running, just_started, just_exited)
    }

    /// Check if currently considered running
    pub fn is_active(&self) -> bool {
        self.was_running
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_detector_creation() {
        let detector = ProcessDetector::new_cs2(5);
        assert!(!detector.is_active());
    }

    #[test]
    fn test_debounce_logic() {
        let mut detector = ProcessDetector::new_cs2(1);
        detector.debounce_duration = Duration::from_millis(50);

        // Manually simulate state
        detector.was_running = true;
        detector.last_seen_running = Some(Instant::now());

        // Process not in /proc, but within debounce
        let (active, started, exited) = detector.update();
        assert!(active);
        assert!(!started);
        assert!(!exited);

        // Wait for debounce duration
        std::thread::sleep(Duration::from_millis(60));
        let (active, started, exited) = detector.update();
        assert!(!active);
        assert!(!started);
        assert!(exited);
    }
}

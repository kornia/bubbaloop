//! Managed process abstraction

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::mpsc;

/// Process status
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessStatus {
    /// Process is pending start
    Pending,
    /// Process is starting
    Starting,
    /// Process is running
    Running,
    /// Process has stopped with exit code
    Stopped(Option<i32>),
    /// Process failed to start
    Failed,
}

impl ProcessStatus {
    /// Check if process is running
    pub fn is_running(&self) -> bool {
        matches!(self, ProcessStatus::Running | ProcessStatus::Starting)
    }

    /// Check if process has stopped
    pub fn is_stopped(&self) -> bool {
        matches!(self, ProcessStatus::Stopped(_) | ProcessStatus::Failed)
    }
}

/// Configuration for spawning a process
#[derive(Debug, Clone)]
pub struct ProcessConfig {
    /// Process name (for logging)
    pub name: String,
    /// Executable path
    pub executable: String,
    /// Command line arguments
    pub args: Vec<String>,
    /// Environment variables
    pub env: HashMap<String, String>,
    /// Working directory
    pub working_dir: Option<PathBuf>,
}

/// Event emitted by a managed process
#[derive(Debug, Clone)]
pub enum ProcessEvent {
    /// Process started
    Started { pid: u32 },
    /// Process output line (stdout or stderr)
    Output { line: String, is_stderr: bool },
    /// Process exited
    Exited { code: Option<i32> },
    /// Process failed to start
    Failed { error: String },
}

/// A managed child process
pub struct ManagedProcess {
    /// Process configuration
    pub config: ProcessConfig,
    /// Current status
    pub status: ProcessStatus,
    /// Process ID (if running)
    pub pid: Option<u32>,
    /// Start time
    pub started_at: Option<Instant>,
    /// Restart count
    pub restart_count: u32,
    /// Child process handle
    child: Option<Child>,
    /// Event sender
    event_tx: Option<mpsc::UnboundedSender<(String, ProcessEvent)>>,
}

impl ManagedProcess {
    /// Create a new managed process
    pub fn new(config: ProcessConfig) -> Self {
        Self {
            config,
            status: ProcessStatus::Pending,
            pid: None,
            started_at: None,
            restart_count: 0,
            child: None,
            event_tx: None,
        }
    }

    /// Set the event sender for this process
    pub fn with_event_sender(
        mut self,
        tx: mpsc::UnboundedSender<(String, ProcessEvent)>,
    ) -> Self {
        self.event_tx = Some(tx);
        self
    }

    /// Start the process
    pub async fn start(&mut self) -> Result<(), ProcessError> {
        if self.status.is_running() {
            return Err(ProcessError::AlreadyRunning(self.config.name.clone()));
        }

        self.status = ProcessStatus::Starting;
        log::info!(
            "[{}] Starting: {} {}",
            self.config.name,
            self.config.executable,
            self.config.args.join(" ")
        );

        let mut cmd = Command::new(&self.config.executable);
        cmd.args(&self.config.args)
            .envs(&self.config.env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(dir) = &self.config.working_dir {
            cmd.current_dir(dir);
        }

        match cmd.spawn() {
            Ok(mut child) => {
                let pid = child.id().unwrap_or(0);
                self.pid = Some(pid);
                self.status = ProcessStatus::Running;
                self.started_at = Some(Instant::now());

                // Send started event
                if let Some(tx) = &self.event_tx {
                    let _ = tx.send((
                        self.config.name.clone(),
                        ProcessEvent::Started { pid },
                    ));
                }

                // Spawn output readers
                let name = self.config.name.clone();
                if let Some(tx) = self.event_tx.clone() {
                    // Read stdout
                    if let Some(stdout) = child.stdout.take() {
                        let name_clone = name.clone();
                        let tx_clone = tx.clone();
                        tokio::spawn(async move {
                            let reader = BufReader::new(stdout);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                let _ = tx_clone.send((
                                    name_clone.clone(),
                                    ProcessEvent::Output {
                                        line,
                                        is_stderr: false,
                                    },
                                ));
                            }
                        });
                    }

                    // Read stderr
                    if let Some(stderr) = child.stderr.take() {
                        let name_clone = name.clone();
                        let tx_clone = tx.clone();
                        tokio::spawn(async move {
                            let reader = BufReader::new(stderr);
                            let mut lines = reader.lines();
                            while let Ok(Some(line)) = lines.next_line().await {
                                let _ = tx_clone.send((
                                    name_clone.clone(),
                                    ProcessEvent::Output {
                                        line,
                                        is_stderr: true,
                                    },
                                ));
                            }
                        });
                    }
                }

                self.child = Some(child);
                Ok(())
            }
            Err(e) => {
                self.status = ProcessStatus::Failed;
                let error = format!("Failed to spawn process: {}", e);
                log::error!("[{}] {}", self.config.name, error);

                if let Some(tx) = &self.event_tx {
                    let _ = tx.send((
                        self.config.name.clone(),
                        ProcessEvent::Failed { error: error.clone() },
                    ));
                }

                Err(ProcessError::SpawnFailed {
                    name: self.config.name.clone(),
                    source: e,
                })
            }
        }
    }

    /// Stop the process gracefully (SIGTERM, then SIGKILL after timeout)
    pub async fn stop(&mut self, timeout: Duration) -> Result<(), ProcessError> {
        if let Some(mut child) = self.child.take() {
            log::info!("[{}] Stopping process...", self.config.name);

            // Try graceful shutdown first (SIGTERM on Unix)
            #[cfg(unix)]
            {
                use nix::sys::signal::{kill, Signal};
                use nix::unistd::Pid;

                if let Some(pid) = self.pid {
                    let _ = kill(Pid::from_raw(pid as i32), Signal::SIGTERM);
                }
            }

            #[cfg(not(unix))]
            {
                // On non-Unix, just kill immediately
                let _ = child.kill().await;
            }

            // Wait for process to exit with timeout
            let exit_result = tokio::time::timeout(timeout, child.wait()).await;

            match exit_result {
                Ok(Ok(status)) => {
                    let code = status.code();
                    self.status = ProcessStatus::Stopped(code);
                    log::info!(
                        "[{}] Process exited with code: {:?}",
                        self.config.name,
                        code
                    );

                    if let Some(tx) = &self.event_tx {
                        let _ = tx.send((
                            self.config.name.clone(),
                            ProcessEvent::Exited { code },
                        ));
                    }
                }
                Ok(Err(e)) => {
                    log::error!("[{}] Error waiting for process: {}", self.config.name, e);
                    self.status = ProcessStatus::Stopped(None);
                }
                Err(_) => {
                    // Timeout - force kill
                    log::warn!(
                        "[{}] Process did not exit gracefully, forcing kill",
                        self.config.name
                    );

                    // Need to reconstruct child to kill it
                    // Since we already took it, we need to kill by PID
                    #[cfg(unix)]
                    {
                        use nix::sys::signal::{kill, Signal};
                        use nix::unistd::Pid;

                        if let Some(pid) = self.pid {
                            let _ = kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
                        }
                    }

                    self.status = ProcessStatus::Stopped(None);

                    if let Some(tx) = &self.event_tx {
                        let _ = tx.send((
                            self.config.name.clone(),
                            ProcessEvent::Exited { code: None },
                        ));
                    }
                }
            }

            self.pid = None;
        }

        Ok(())
    }

    /// Check if the process is still running
    pub async fn check_status(&mut self) -> ProcessStatus {
        if let Some(child) = &mut self.child {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let code = status.code();
                    self.status = ProcessStatus::Stopped(code);
                    self.pid = None;

                    if let Some(tx) = &self.event_tx {
                        let _ = tx.send((
                            self.config.name.clone(),
                            ProcessEvent::Exited { code },
                        ));
                    }

                    self.child = None;
                }
                Ok(None) => {
                    // Still running
                }
                Err(e) => {
                    log::error!(
                        "[{}] Error checking process status: {}",
                        self.config.name,
                        e
                    );
                }
            }
        }

        self.status
    }

    /// Get uptime duration
    pub fn uptime(&self) -> Option<Duration> {
        self.started_at.map(|t| t.elapsed())
    }
}

/// Errors that can occur with managed processes
#[derive(Debug, thiserror::Error)]
pub enum ProcessError {
    #[error("Process '{0}' is already running")]
    AlreadyRunning(String),

    #[error("Failed to spawn process '{name}': {source}")]
    SpawnFailed {
        name: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Process '{0}' is not running")]
    NotRunning(String),
}

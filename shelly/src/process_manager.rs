use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::SystemTime;
use tokio::sync::{watch, RwLock};
use tokio::task::JoinHandle;
use tokio::time::Duration;
use uuid::Uuid;

use crate::runtime::{process, HandlerRuntime};
use crate::output;

#[derive(Debug, Clone, Serialize, Deserialize, Hash, Eq, PartialEq)]
pub struct ProcessId(pub String);

impl ProcessId {
    pub fn new() -> Self {
        Self(Uuid::new_v4().to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum ProcessState {
    Running,
    Completed { exit_code: i32 },
    Cancelled,
    Failed { error: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub id: ProcessId,
    pub command: String,
    pub state: ProcessState,
    pub started_at: SystemTime,
    pub raw_stdout: String,
    pub raw_stderr: String,
    pub output_file: Option<PathBuf>,
}

#[derive(Serialize)]
pub struct ProcessUpdate {
    pub incremental_summary: String,
    pub status: ProcessState,
}

// Simplified ProcessTask that just stores updates and state
pub struct ProcessTask {
    pub info: ProcessInfo,
    pub delta_summary: String,
    pub executor_handle: Option<JoinHandle<anyhow::Result<()>>>,
    pub complete_tx: watch::Sender<bool>,
    pub complete_rx: watch::Receiver<bool>,
}

pub struct ProcessManager {
    pub processes: Arc<RwLock<HashMap<ProcessId, ProcessTask>>>,
}

impl ProcessManager {
    pub fn new() -> Self {
        let processes: Arc<RwLock<HashMap<ProcessId, ProcessTask>>> =
            Arc::new(RwLock::new(HashMap::new()));

        // Spawn cleanup task
        let processes_cleanup = processes.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            loop {
                interval.tick().await;
                let max_age = Duration::from_secs(3600);

                let mut processes = processes_cleanup.write().await;
                let now = SystemTime::now();

                let to_remove: Vec<_> = processes
                    .iter()
                    .filter(|(_, task)| {
                        matches!(
                            task.info.state,
                            ProcessState::Completed { .. }
                                | ProcessState::Cancelled
                                | ProcessState::Failed { .. }
                        ) && now
                            .duration_since(task.info.started_at)
                            .unwrap_or(Duration::ZERO)
                            > max_age
                    })
                    .map(|(id, _)| id.clone())
                    .collect();

                for id in to_remove {
                    processes.remove(&id);
                }
            }
        });

        Self { processes }
    }

    pub async fn start_process(&self, command: String, output_file: PathBuf) -> ProcessId {
        let process_id = ProcessId::new();
        let info = ProcessInfo {
            id: process_id.clone(),
            command,
            state: ProcessState::Running,
            started_at: SystemTime::now(),
            raw_stdout: String::new(),
            raw_stderr: String::new(),
            output_file: Some(output_file),
        };

        let (tx, rx) = watch::channel(false);
        let process_task = ProcessTask {
            info,
            executor_handle: None,
            delta_summary: String::new(),
            complete_tx: tx,
            complete_rx: rx,
        };

        let mut processes = self.processes.write().await;
        processes.insert(process_id.clone(), process_task);

        process_id
    }

    pub async fn register_handle(
        &self,
        process_id: &ProcessId,
        handle: JoinHandle<anyhow::Result<()>>,
    ) {
        let mut processes = self.processes.write().await;
        if let Some(task) = processes.get_mut(process_id) {
            task.executor_handle = Some(handle);
        }
    }

    pub async fn update_process_output(
        &self,
        process_id: &ProcessId,
        stdout: String,
        stderr: String,
        handler: &Option<HandlerRuntime>,
    ) {
        let mut processes = self.processes.write().await;
        let task = processes.get_mut(process_id).unwrap();
        task.info.raw_stdout.push_str(&stdout);
        task.info.raw_stderr.push_str(&stderr);
        let summary = process(&stdout, &stderr, &handler).await.unwrap();
        task.delta_summary
            .push_str(&summary.summary.unwrap_or_default());
    }

    pub async fn final_process_summary(
        &self,
        process_id: &ProcessId,
        exit_code: i32,
        handler: &HandlerRuntime,
    ) {
        let mut processes = self.processes.write().await;
        let task = processes.get_mut(process_id).unwrap();
        
        // Call handler with final exit code
        let summary = handler.summarize(
            &task.info.raw_stdout,
            &task.info.raw_stderr,
            Some(exit_code),
        ).await.unwrap();
        
        if let Some(final_summary) = summary.summary {
            task.delta_summary = final_summary;
        }
    }

    pub async fn complete_process(&self, process_id: &ProcessId, exit_code: i32) {
        let mut processes = self.processes.write().await;
        let task = processes.get_mut(process_id).unwrap();
        task.info.state = ProcessState::Completed { exit_code };
        
        // Write output to file if path is set
        if let Some(output_file) = &task.info.output_file {
            let _ = output::write_output(
                output_file,
                &task.info.raw_stdout,
                &task.info.raw_stderr,
                exit_code,
            );
        }
        
        let _ = task.complete_tx.send(true);
    }

    pub async fn cancel_process(&self, process_id: &ProcessId) -> bool {
        let mut processes = self.processes.write().await;
        if let Some(task) = processes.get_mut(process_id) {
            if let Some(handle) = &task.executor_handle {
                handle.abort();
                task.info.state = ProcessState::Cancelled;
                return true;
            }
        }
        false
    }

    pub async fn get_process_status(&self, process_id: &ProcessId) -> Option<ProcessInfo> {
        let processes = self.processes.read().await;
        processes.get(process_id).map(|task| task.info.clone())
    }

    pub async fn wait_for(&self, process_id: &ProcessId) {
        let processes = self.processes.read().await;
        let task = processes.get(process_id).unwrap();
        let mut waiter = task.complete_rx.clone();
        drop(processes);
        let _ = waiter.wait_for(|t| *t).await;
    }

    // New method: get incremental updates since last join
    pub async fn join_process(
        &self,
        process_id: &ProcessId,
        timeout: Duration,
    ) -> Option<ProcessUpdate> {
        let _ = tokio::time::timeout(timeout, self.wait_for(process_id)).await;
        let mut processes = self.processes.write().await;

        let task = processes.get_mut(process_id)?;
        let summary = std::mem::take(&mut task.delta_summary);
        Some(ProcessUpdate {
            incremental_summary: last_n_chars(&summary, 1000).to_string(),
            status: task.info.state.clone(),
        })
    }
}

fn last_n_chars(s: &str, n: usize) -> &str {
    let char_count = s.chars().count();
    if char_count <= n {
        return s;
    }

    s.char_indices()
        .nth(char_count - n)
        .map(|(i, _)| &s[i..])
        .unwrap_or(s)
}

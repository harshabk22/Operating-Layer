use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct IdempotencyKey(pub String);

#[derive(Debug, Clone)]
pub struct Action<P> {
    pub key: IdempotencyKey,
    pub payload: P,
    pub max_retries: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionStatus {
    Pending,
    Running,
    Success,
    Failed,
    DeadLetter,
}

#[derive(Debug, Clone)]
pub struct Execution<R> {
    pub key: IdempotencyKey,
    pub status: ExecutionStatus,
    pub attempts: usize,
    pub result: Option<R>,
    pub error: Option<String>,
}

#[async_trait]
pub trait ExecutionWorker<P, R>: Send + Sync + 'static {
    async fn process(&self, action: Action<P>) -> Result<R, String>;
}

pub struct ExecutionQueue<P, R> {
    executions: Arc<Mutex<HashMap<IdempotencyKey, Execution<R>>>>,
    dlq: Arc<Mutex<Vec<Action<P>>>>,
    worker: Arc<dyn ExecutionWorker<P, R>>,
}

impl<P: Send + Sync + Clone + 'static, R: Send + Sync + Clone + 'static> ExecutionQueue<P, R> {
    pub fn new(worker: Arc<dyn ExecutionWorker<P, R>>) -> Self {
        Self {
            executions: Arc::new(Mutex::new(HashMap::new())),
            dlq: Arc::new(Mutex::new(Vec::new())),
            worker,
        }
    }

    pub async fn submit(&self, action: Action<P>) {
        let mut execs = self.executions.lock().await;
        if execs.contains_key(&action.key) {
            return;
        }

        execs.insert(
            action.key.clone(),
            Execution {
                key: action.key.clone(),
                status: ExecutionStatus::Pending,
                attempts: 0,
                result: None,
                error: None,
            },
        );

        let executions = self.executions.clone();
        let dlq = self.dlq.clone();
        let worker = self.worker.clone();
        let act = action.clone();

        tokio::spawn(async move {
            let mut current_attempt = 0;
            loop {
                {
                    let mut ex_guard = executions.lock().await;
                    if let Some(ex) = ex_guard.get_mut(&act.key) {
                        ex.status = ExecutionStatus::Running;
                        ex.attempts += 1;
                        current_attempt = ex.attempts;
                    }
                }

                let result = worker.process(act.clone()).await;

                match result {
                    Ok(r) => {
                        let mut ex_guard = executions.lock().await;
                        if let Some(ex) = ex_guard.get_mut(&act.key) {
                            ex.status = ExecutionStatus::Success;
                            ex.result = Some(r);
                            ex.error = None;
                        }
                        break;
                    }
                    Err(e) => {
                        let mut ex_guard = executions.lock().await;
                        if let Some(ex) = ex_guard.get_mut(&act.key) {
                            if current_attempt > act.max_retries {
                                ex.status = ExecutionStatus::DeadLetter;
                                ex.error = Some(e.clone());
                                dlq.lock().await.push(act.clone());
                                break;
                            } else {
                                ex.status = ExecutionStatus::Failed;
                                ex.error = Some(e);
                            }
                        }
                    }
                }
            }
        });
    }

    pub async fn get_execution(&self, key: &IdempotencyKey) -> Option<Execution<R>> {
        let execs = self.executions.lock().await;
        execs.get(key).cloned()
    }

    pub async fn get_dlq(&self) -> Vec<Action<P>> {
        let dlq = self.dlq.lock().await;
        dlq.clone()
    }
}

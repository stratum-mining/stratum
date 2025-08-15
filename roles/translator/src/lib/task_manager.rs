use std::sync::Mutex as StdMutex;
use tokio::task::JoinHandle;

/// Manages a collection of spawned tokio tasks.
///
/// This struct provides a centralized way to spawn, track, and manage the lifecycle
/// of async tasks in the translator. It maintains a list of join handles that can
/// be used to wait for all tasks to complete or abort them during shutdown.
pub struct TaskManager {
    tasks: StdMutex<Vec<JoinHandle<()>>>,
}

impl Default for TaskManager {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskManager {
    /// Creates a new TaskManager instance.
    ///
    /// Initializes an empty task manager ready to spawn and track tasks.
    pub fn new() -> Self {
        Self {
            tasks: StdMutex::new(Vec::new()),
        }
    }

    /// Spawns a new async task and adds it to the managed collection.
    ///
    /// The task will be tracked by this manager and can be waited for or aborted
    /// using the other methods.
    ///
    /// # Arguments
    /// * `fut` - The future to spawn as a task
    #[track_caller]
    pub fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        use tracing::Instrument;
        let location = std::panic::Location::caller();
        let span = tracing::trace_span!(
            "task",
            file = location.file(),
            line = location.line(),
            column = location.column(),
        );

        let handle = tokio::spawn(fut.instrument(span));
        self.tasks.lock().unwrap().push(handle);
    }

    /// Waits for all managed tasks to complete.
    ///
    /// This method will block until all tasks that were spawned through this
    /// manager have finished executing. Tasks are joined in reverse order
    /// (most recently spawned first).
    pub async fn join_all(&self) {
        let handles = {
            let mut tasks = self.tasks.lock().unwrap();
            std::mem::take(&mut *tasks)
        };

        for handle in handles {
            let _ = handle.await;
        }
    }

    /// Aborts all managed tasks.
    ///
    /// This method immediately cancels all tasks that were spawned through this
    /// manager. The tasks will be terminated without waiting for them to complete.
    pub async fn abort_all(&self) {
        let mut tasks = self.tasks.lock().unwrap();
        for handle in tasks.drain(..) {
            handle.abort();
        }
    }
}

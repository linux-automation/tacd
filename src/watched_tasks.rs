use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use anyhow::{Context as AnyhowContext, Result};
use async_std::task;
use log::info;

// This is a wrapper around async_std::task:spawn() that keeps track of the
// tasks it spawned. This solves the problem of error propagation from tasks
// for us.
// When using async_std::task:spawn() you get a handle back that can be used
// to check if and how the task has completed, but there is no common way in
// async_std to have a list of tasks to watch at the same time.
// We want to keep track of the various long running tasks we spawn in the
// tacd and want to propagate errors from back to main().
// This is what WatchedTasks does.
//
// There are other solutions that do basically the same, but did not quite
// fit our needs:
//
//   - async_nursery - Works for async_std but does not look that
//     great code-wise.
//   - tokio JoinSet - Does roughly the same as WatchedTasks, but
//     for tokio (which we do not use).

type TaskResult = Result<()>;
type TaskHandle = task::JoinHandle<TaskResult>;

pub struct WatchedTasksBuilder {
    tasks: Vec<TaskHandle>,
}

pub struct WatchedTasks {
    tasks: Vec<TaskHandle>,
}

impl WatchedTasksBuilder {
    pub fn new() -> Self {
        Self { tasks: Vec::new() }
    }

    /// Spawn an async task that runs until the end of the program
    ///
    /// If any of the tasks spawned this way returns, the WatchedTasks
    /// Future will return the Result of said task.
    /// The WatchedTasks Future should be .awaited at the end of main() so
    /// that the program ends if any of the watched tasks ends.
    pub fn spawn_task<S, F>(&mut self, name: S, future: F)
    where
        S: Into<String>,
        F: Future<Output = TaskResult> + Send + 'static,
    {
        let task = task::Builder::new()
            .name(name.into())
            .spawn(future)
            .expect("cannot spawn task");

        self.tasks.push(task);
    }

    /// Complete the task and thread creation and enter the steady state of the program
    ///
    /// The returned WatchedTasks should be .awaited at the end of `main()` to end the
    /// program if any of the watched threads or tasks ends.
    pub fn watch(self) -> WatchedTasks {
        info!("Spawned {} tasks", self.tasks.len(),);

        WatchedTasks { tasks: self.tasks }
    }
}

impl Future for WatchedTasks {
    type Output = TaskResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        for task in self.tasks.iter_mut() {
            let name = task.task().name().unwrap_or("<unknown>").to_owned();

            if let Poll::Ready(res) = Pin::new(task).poll(cx) {
                info!("Task {name} has completed");

                let res = res.with_context(|| format!("Failed in task {name}"));

                // The first tasks task to finish determines when all
                // other should finish as well.
                return Poll::Ready(res);
            }
        }

        Poll::Pending
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use anyhow::Result;
    use async_std::channel::{unbounded, Sender};
    use async_std::future::timeout;
    use async_std::task::block_on;

    use super::{TaskResult, WatchedTasks, WatchedTasksBuilder};

    const TIMEOUT: Duration = Duration::from_millis(100);

    fn setup_tasks() -> (WatchedTasks, Vec<Sender<TaskResult>>) {
        let mut wtb = WatchedTasksBuilder::new();

        // Spawn ten tasks that each wait for a message on a channel and
        // complete if they receive it.
        let senders_tasks: Vec<_> = (0..5)
            .map(|i| {
                let (tx, rx) = unbounded();

                wtb.spawn_task(format!("task-{i}"), async move {
                    println!("Hi from task {i}!");
                    let res = rx.recv().await?;
                    println!("Bye from task {i}!");
                    res
                });

                tx
            })
            .collect();

        (wtb.watch(), senders_tasks)
    }

    #[test]
    fn tasks_end_execution() -> Result<()> {
        let (mut wt, senders_tasks) = setup_tasks();

        // At this point none of tasks have completed yet.
        // Make sure wt reflects that.
        let wt_early_res = block_on(timeout(TIMEOUT, async { (&mut wt).await }));
        assert!(wt_early_res.is_err());

        // Make one of the tasks complete.
        senders_tasks[3].try_send(Ok(()))?;

        // Now wt should complete as well.
        let wt_late_res = block_on(timeout(TIMEOUT, async { (&mut wt).await }));
        assert!(matches!(wt_late_res, Ok(Ok(()))));

        Ok(())
    }
}

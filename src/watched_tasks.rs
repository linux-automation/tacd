use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll, Waker};
use std::thread;

use anyhow::{anyhow, Context as AnyhowContext, Result};
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
// We also want the same, with a similar API for actual threads instead of
// async tasks. WatchedTasks does both.
//
// There are other solutions that do basically the same, but did not quite
// fit our needs:
//
//   - async_nursery - Works for async_std but does not handle actual threads
//     and does not look that great code-wise.
//   - tokio JoinSet - Does roughly the same as WatchedTasks, but also without
//     native thread support and - more importantly for tokio (which we do
//     not use).

type TaskResult = Result<()>;
type TaskHandle = task::JoinHandle<TaskResult>;

struct ThreadHandle {
    handle: Option<thread::JoinHandle<TaskResult>>,
    wake_on_exit: Arc<Mutex<Option<Waker>>>,
}

pub struct WatchedTasksBuilder {
    tasks: Vec<TaskHandle>,
    threads: Vec<ThreadHandle>,
}

pub struct WatchedTasks {
    tasks: Vec<TaskHandle>,
    threads: Vec<ThreadHandle>,
}

impl ThreadHandle {
    fn new<F>(name: String, function: F) -> Result<Self>
    where
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        let wake_on_exit = Arc::new(Mutex::new(None::<Waker>));
        let wake_on_exit_thread = wake_on_exit.clone();

        // We initially used async_std::task::spawn_blocking() here,
        // but that does not seem to be intended for long-running threads but instead
        // to run blocking operations and get the result of them as a Future.
        // There is a maximum amount of threads that can be spawned via spawn_blocking()
        // (configurable via an environment variable) and if more tasks are spawned they
        // will not start exeuting until enough tasks exit (which in our case they won't).
        // We also do configurations like setting realtime priorities for threads,
        // which we should not to for threads that are recycled in a thread pool.
        // Instead spawn a thread the normal way and handle completion-notifications
        // manually.

        let handle = thread::Builder::new().name(name).spawn(move || {
            // We could std::panic::catch_unwind() here in the future to handle
            // panics inside of spawned threads.
            let res = function();

            // Keep the Mutex locked until exiting the thread to prevent the case
            // following race condition:
            //
            // - Another thread checks if this one ended (which it did not)
            // - This thread is about to end and checks wake_on_exit
            // - The other thread sets wake_on_exit
            let mut wake_on_exit = wake_on_exit_thread
                .lock()
                .map_err(|_| anyhow!("Tried to lock a tainted Mutex"))?;

            if let Some(waker) = wake_on_exit.take() {
                waker.wake();
            }

            res
        })?;

        Ok(Self {
            handle: Some(handle),
            wake_on_exit,
        })
    }

    fn name(&self) -> Option<&str> {
        self.handle
            .as_ref()
            .and_then(|handle| handle.thread().name())
    }
}

impl Future for ThreadHandle {
    type Output = TaskResult;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        {
            // Lock the Mutex before checking for thread completion to prevent
            // the thread from completing while we are setting up the waker.
            let mut wake_on_exit = self
                .wake_on_exit
                .lock()
                .expect("Tried to lock a tainted Mutex");

            let ready = self
                .handle
                .as_ref()
                .map(|handle| handle.is_finished())
                .unwrap_or(true);

            if !ready {
                // The thread is not yet ready. Make sure the current task is polled again
                // if the thread becomes ready.
                *wake_on_exit = Some(cx.waker().clone());

                return Poll::Pending;
            }
        }

        // Get the actual result of the thread via the JoinHandle.
        // The handle.join() call is technically blocking, but we know that the
        // task has completed from the notification channel, so it isn't in practice.
        let res = self
            .handle
            .take()
            .ok_or_else(|| anyhow!("ThreadHandle was already polled to completion"))
            .and_then(|handle| match handle.join() {
                Ok(r) => r,
                Err(_) => Err(anyhow!("Failed to get thread join result")),
            });

        Poll::Ready(res)
    }
}

impl WatchedTasksBuilder {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            threads: Vec::new(),
        }
    }

    /// Spawn an async task that runs until the end of the program
    ///
    /// If any of the tasks spawned this way returns, the WatchedTasks
    /// Future will return the Result of said task.
    /// The WatchedTasks Future should be .awaited at the end of main() so
    /// that the program ends if any of the watched tasks ends.
    pub fn spawn_task<S, F>(&mut self, name: S, future: F) -> Result<()>
    where
        S: Into<String>,
        F: Future<Output = TaskResult> + Send + 'static,
    {
        let task = task::Builder::new().name(name.into()).spawn(future)?;

        self.tasks.push(task);

        Ok(())
    }

    /// Spawn a thread that runs until the end of the program
    ///
    /// If any of the threads spawned this way returns, the WatchedTasks
    /// Future will return the Result of said thread.
    /// The WatchedTasks Future should be .awaited at the end of main() so
    /// that the program ends if any of the watched threads ends.
    pub fn spawn_thread<S, F>(&mut self, name: S, function: F) -> Result<()>
    where
        S: Into<String>,
        F: FnOnce() -> TaskResult + Send + 'static,
    {
        let thread = ThreadHandle::new(name.into(), function)?;

        self.threads.push(thread);

        Ok(())
    }

    /// Complete the task and thread creation and enter the steady state of the program
    ///
    /// The returned WatchedTasks should be .awaited at the end of `main()` to end the
    /// program if any of the watched threads or tasks ends.
    pub fn watch(self) -> WatchedTasks {
        info!(
            "Spawned {} tasks and {} threads",
            self.tasks.len(),
            self.threads.len()
        );

        WatchedTasks {
            tasks: self.tasks,
            threads: self.threads,
        }
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

                // The first task to finish determines when all other should finish as well.
                return Poll::Ready(res);
            }
        }

        for thread in self.threads.iter_mut() {
            let name = thread.name().unwrap_or("<unknown>").to_owned();

            if let Poll::Ready(res) = Pin::new(thread).poll(cx) {
                info!("Thread {name} has completed");

                let res = res.with_context(|| format!("Failed in thread {name}"));

                // The first thread to finish determines when all other should finish as well.
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

    fn setup_tasks_and_threads() -> (
        WatchedTasks,
        Vec<Sender<TaskResult>>,
        Vec<Sender<TaskResult>>,
    ) {
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
                })
                .unwrap();

                tx
            })
            .collect();

        // Spawn ten tasks that each wait for a message on a channel and
        // complete if they receive it.
        let senders_threads: Vec<_> = (0..5)
            .map(|i| {
                let (tx, rx) = unbounded();

                wtb.spawn_thread(format!("thread-{i}"), move || {
                    println!("Hi from thread {i}!");
                    let res = block_on(rx.recv())?;
                    println!("Bye from thread {i}!");
                    res
                })
                .unwrap();

                tx
            })
            .collect();

        (wtb.watch(), senders_tasks, senders_threads)
    }

    #[test]
    fn tasks_end_execution() -> Result<()> {
        let (mut wt, senders_tasks, _senders_threads) = setup_tasks_and_threads();

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

    #[test]
    fn threads_end_execution() -> Result<()> {
        let (mut wt, _senders_tasks, senders_threads) = setup_tasks_and_threads();

        // At this point none of threads have completed yet.
        // Make sure wt reflects that.
        let wt_early_res = block_on(timeout(TIMEOUT, async { (&mut wt).await }));
        assert!(wt_early_res.is_err());

        // Make one of the threads complete.
        senders_threads[3].try_send(Ok(()))?;

        // Now wt should complete as well.
        let wt_late_res = block_on(timeout(TIMEOUT, async { (&mut wt).await }));
        assert!(matches!(wt_late_res, Ok(Ok(()))));

        Ok(())
    }
}

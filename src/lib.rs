#![feature(duration_constructors)]
use std::{future::Future, pin::Pin, sync::Arc};

use tokio::{sync::Mutex, task::JoinHandle};

#[derive(Clone)]
pub struct RateLimiter {
    limiter: Arc<Mutex<RateLimiterFut>>,
}

impl RateLimiter {
    fn new(
        max_attempts: usize,
        sleep_fn: impl Fn() -> Pin<Box<dyn Future<Output = ()> + Send + Sync>> + Send + 'static,
    ) -> Self {
        Self {
            limiter: Arc::new(Mutex::new(RateLimiterFut::build_new(
                max_attempts,
                sleep_fn,
            ))),
        }
    }

    pub async fn run_with<F: Future>(&self, task: F) -> F::Output {
        let mut limiter = self.limiter.lock().await;
        limiter.run_task(task).await
    }
}

struct RateLimiterFut {
    sleep_fut: JoinHandle<()>,
    sleep_fn: Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send + Sync>> + Send>,
    max_attempts: usize,
    current_attempts: usize,
}

impl RateLimiterFut {
    fn build_new(
        max_attempts: usize,
        sleep_fn: impl Fn() -> Pin<Box<dyn Future<Output = ()> + Send + Sync>> + Send + 'static,
    ) -> Self {
        Self {
            sleep_fut: tokio::spawn(sleep_fn()),
            max_attempts,
            current_attempts: 0,
            sleep_fn: Box::new(sleep_fn),
        }
    }

    /// resets the timer fut and returns the old one to be awaited on
    fn reset(&mut self) -> JoinHandle<()> {
        let new_sleep = tokio::spawn((self.sleep_fn)());
        let old_sleep = std::mem::replace(&mut self.sleep_fut, new_sleep);
        self.current_attempts = 0;
        old_sleep
    }

    async fn run_task<F: Future>(&mut self, task: F) -> F::Output {
        if self.current_attempts == self.max_attempts {
            self.block_task_until_timer(task).await
        } else {
            self.current_attempts += 1;
            task.await
        }
    }

    async fn block_task_until_timer<F: Future>(&mut self, task: F) -> F::Output {
        let _ = self.reset().await;
        self.current_attempts += 1;
        task.await
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RateLimiterBuilder {
    pub attempts_per_time_unit: u64,
    pub unit_of_time: TimeUnit,
}

impl RateLimiterBuilder {
    pub fn new(unit_of_time: TimeUnit) -> Self {
        Self {
            attempts_per_time_unit: 0,
            unit_of_time,
        }
    }

    pub fn with_attempts(mut self, attempts_per_time_unit: u64) -> Self {
        self.attempts_per_time_unit = attempts_per_time_unit;
        self
    }

    pub fn build(self) -> RateLimiter {
        let time = self.unit_of_time.as_duration();
        let sleep_fn = move || {
            Box::pin(tokio::time::sleep(time)) as Pin<Box<dyn Future<Output = ()> + Send + Sync>>
        };

        RateLimiter::new(self.attempts_per_time_unit as usize, sleep_fn)
    }
}

#[derive(Debug, Clone, Copy)]
pub enum TimeUnit {
    Millisecond(u64),
    Second(u64),
    Minute(u64),
    Hour(u64),
    Day(u64),
}

impl TimeUnit {
    fn as_duration(self) -> std::time::Duration {
        match self {
            TimeUnit::Millisecond(time) => std::time::Duration::from_millis(time),
            TimeUnit::Second(time) => std::time::Duration::from_secs(time),
            TimeUnit::Minute(time) => std::time::Duration::from_mins(time),
            TimeUnit::Hour(time) => std::time::Duration::from_hours(time),
            TimeUnit::Day(time) => std::time::Duration::from_days(time),
        }
    }

    pub fn millis(millis: u64) -> Self {
        assert!(millis >= 1);
        Self::Millisecond(millis)
    }

    pub fn seconds(secs: u64) -> Self {
        assert!(secs >= 1);
        Self::Second(secs)
    }

    pub fn minutes(mins: u64) -> Self {
        assert!(mins >= 1);
        Self::Minute(mins)
    }

    pub fn hours(hours: u64) -> Self {
        assert!(hours >= 1);
        Self::Hour(hours)
    }

    pub fn days(days: u64) -> Self {
        assert!(days >= 1);
        Self::Day(days)
    }
}

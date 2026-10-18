//! # Manual Future Implementation
//!
//! In this exercise, you will manually implement the `Future` trait for custom types to understand the core mechanism of asynchronous runtime.
//!
//! ## Concepts
//! - `std::future::Future` trait
//! - `Poll::Ready` and `Poll::Pending`
//! - The role of `Waker`: notifying the runtime to poll again

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Countdown Future: decrements count by 1 each time it's polled,
/// returns `"liftoff!"` when count reaches 0.
pub struct CountDown {
    pub count: u32,
}

impl CountDown {
    pub fn new(count: u32) -> Self {
        Self { count }
    }
}

impl Future for CountDown {
    type Output = &'static str;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.count == 0 {
            Poll::Ready("liftoff!")
        } else {
            this.count -= 1;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

/// Yield-only-once Future: first poll returns Pending, second returns Ready(()).
/// This is the minimal example of an asynchronous state machine.
pub struct YieldOnce {
    yielded: bool,
}

impl YieldOnce {
    pub fn new() -> Self {
        Self { yielded: false }
    }
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        if this.yielded {
            Poll::Ready(())
        } else {
            this.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::task::{Wake, Waker};

    #[test]
    fn test_poll_states_and_wakes() {
        struct WakeCounter(AtomicUsize);

        impl Wake for WakeCounter {
            fn wake(self: Arc<Self>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }

        let counter = Arc::new(WakeCounter(AtomicUsize::new(0)));
        let waker = Waker::from(counter.clone());
        let mut cx = Context::from_waker(&waker);
        let mut countdown = CountDown::new(2);
        assert_eq!(Pin::new(&mut countdown).poll(&mut cx), Poll::Pending);
        assert_eq!(countdown.count, 1);
        assert_eq!(Pin::new(&mut countdown).poll(&mut cx), Poll::Pending);
        assert_eq!(countdown.count, 0);
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);
        assert_eq!(
            Pin::new(&mut countdown).poll(&mut cx),
            Poll::Ready("liftoff!")
        );
        assert_eq!(counter.0.load(Ordering::SeqCst), 2);

        let mut once = YieldOnce::new();
        assert_eq!(Pin::new(&mut once).poll(&mut cx), Poll::Pending);
        assert_eq!(counter.0.load(Ordering::SeqCst), 3);
        assert_eq!(Pin::new(&mut once).poll(&mut cx), Poll::Ready(()));
        assert_eq!(counter.0.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn test_countdown_zero() {
        let result = CountDown::new(0).await;
        assert_eq!(result, "liftoff!");
    }

    #[tokio::test]
    async fn test_countdown_three() {
        let result = CountDown::new(3).await;
        assert_eq!(result, "liftoff!");
    }

    #[tokio::test]
    async fn test_yield_once() {
        YieldOnce::new().await;
    }

    #[tokio::test]
    async fn test_countdown_large() {
        let result = CountDown::new(100).await;
        assert_eq!(result, "liftoff!");
    }
}

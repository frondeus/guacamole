#![allow(dead_code)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use futures::task::{AtomicWaker, Context, Poll};
use futures::Future;
use std::pin::Pin;

#[derive(Debug)]
struct Inner {
    waker: AtomicWaker,
    ready: AtomicBool
}

pub struct Reservation(Arc<Inner>);

#[derive(Clone, Debug)]
pub struct ReservationReader(Arc<Inner>);

impl Future for ReservationReader {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0.ready.load(Ordering::Relaxed) {
            return Poll::Ready(());
        }

        self.0.waker.register(cx.waker());

        if self.0.ready.load(Ordering::Relaxed) {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Reservation {
    pub async fn new() -> (Self, ReservationReader) {
        let inner = Arc::new(Inner {
            waker: AtomicWaker::new(),
            ready: AtomicBool::new(false),
        });

        (Self(inner.clone()), ReservationReader(inner))
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.0.ready.store(true, Ordering::Relaxed);
        self.0.waker.wake();
    }
}

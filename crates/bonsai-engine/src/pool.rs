use std::any::Any;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread::{Scope, ScopedJoinHandle};

/// Generated code nests deeper than a default thread stack lets the recursive walkers go. The
/// reservation is address space rather than memory, and every shipped target is 64-bit.
pub const STACK_SIZE: usize = 256 << 20;

/// Results come back in item order whatever order the workers finished in. Each worker builds
/// its own `state`, because parsing takes `&mut` and a parser cannot be shared.
pub(crate) fn map<T, S, R>(
    items: &[T],
    workers: NonZeroUsize,
    state: impl Fn() -> S + Sync,
    step: impl Fn(&mut S, &T) -> R + Sync,
) -> Vec<R>
where
    T: Sync,
    R: Send,
{
    if items.is_empty() {
        return Vec::new();
    }

    let cursor = AtomicUsize::new(0);
    let run = || {
        let mut own = state();
        Cursor::new(items, &cursor)
            .map(|(index, item)| (index, step(&mut own, item)))
            .collect::<Vec<_>>()
    };

    let (parts, failure) = std::thread::scope(|scope| {
        let handles = spawn(scope, workers, &run);
        // Not being allowed a thread is a reason to be slow, not to fail the run.
        if handles.is_empty() {
            return (vec![run()], None);
        }
        join(handles)
    });

    if let Some(payload) = failure {
        std::panic::resume_unwind(payload);
    }
    order(parts, items.len())
}

/// A worker that cannot be started is dropped rather than reported: the rest still cover every
/// job, because they draw from one cursor rather than a fixed share.
fn spawn<'scope, F, V>(
    scope: &'scope Scope<'scope, '_>,
    workers: NonZeroUsize,
    run: &'scope F,
) -> Vec<ScopedJoinHandle<'scope, V>>
where
    F: Fn() -> V + Sync,
    V: Send + 'scope,
{
    (0..workers.get())
        .filter_map(|index| {
            std::thread::Builder::new()
                .name(format!("bonsai-scan-{index}"))
                .stack_size(STACK_SIZE)
                .spawn_scoped(scope, run)
                .ok()
        })
        .collect()
}

/// Joins every worker before re-raising, so none is still running when a panic propagates, and
/// takes the first failure by worker order so the message does not depend on who lost the race.
fn join<V>(handles: Vec<ScopedJoinHandle<'_, V>>) -> (Vec<V>, Option<Box<dyn Any + Send>>) {
    let mut parts = Vec::with_capacity(handles.len());
    let mut failure: Option<Box<dyn Any + Send>> = None;

    for handle in handles {
        match handle.join() {
            Ok(part) => parts.push(part),
            Err(payload) => failure = failure.or(Some(payload)),
        }
    }

    (parts, failure)
}

fn order<R>(parts: Vec<Vec<(usize, R)>>, len: usize) -> Vec<R> {
    let mut slots: Vec<Option<R>> = (0..len).map(|_| None).collect();
    for (index, value) in parts.into_iter().flatten() {
        slots[index] = Some(value);
    }

    // `flatten` would quietly shorten the report if a slot were ever missed.
    let ordered: Vec<R> = slots.into_iter().flatten().collect();
    assert_eq!(ordered.len(), len, "every job is scored exactly once");
    ordered
}

/// One index at a time: a fixed split would leave one worker holding a 2 MB bundle while the
/// rest sat idle, because file sizes in a repository differ by orders of magnitude.
struct Cursor<'a, T> {
    items: &'a [T],
    next: &'a AtomicUsize,
}

impl<'a, T> Cursor<'a, T> {
    fn new(items: &'a [T], next: &'a AtomicUsize) -> Self {
        Self { items, next }
    }
}

impl<'a, T> Iterator for Cursor<'a, T> {
    type Item = (usize, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        // Relaxed suffices: the counter is the only shared mutable state, and what it indexes
        // was published before the scope opened.
        let index = self.next.fetch_add(1, Ordering::Relaxed);
        self.items.get(index).map(|item| (index, item))
    }
}

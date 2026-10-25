//! A scoped parallel map over a slice, answers in input order. Enough for
//! the engine's independent runs without a thread-pool dependency.

use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// How many threads a batch should use on this device.
pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

pub fn map<T: Sync, R: Send>(items: &[T], threads: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let threads = threads.clamp(1, items.len().max(1));
    if threads == 1 {
        return items.iter().map(f).collect();
    }
    let next = AtomicUsize::new(0);
    let out: Mutex<Vec<Option<R>>> = Mutex::new((0..items.len()).map(|_| None).collect());
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(item) = items.get(i) else { break };
                    let r = f(item);
                    out.lock().unwrap_or_else(|e| e.into_inner())[i] = Some(r);
                }
            });
        }
    });
    out.into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .into_iter()
        .map(|r| r.expect("every item ran"))
        .collect()
}

#[cfg(test)]
mod tests {
    #[test]
    fn keeps_order() {
        let items: Vec<u32> = (0..100).collect();
        assert_eq!(
            super::map(&items, 8, |x| x * 2),
            items.iter().map(|x| x * 2).collect::<Vec<_>>()
        );
        assert!(super::map(&Vec::<u32>::new(), 8, |x| *x).is_empty());
    }
}

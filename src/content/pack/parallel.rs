use crate::minecraft::{Downloader, Result};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

// Keep workers busy when individual files have different download/verification times.
// Results retain manifest order; an error stops workers from taking more queued files.
pub(super) fn parallel_map<T: Sync, R: Send>(
    items: &[T],
    downloader: &Downloader,
    work: impl Fn(&T) -> Result<R> + Sync,
) -> Result<Vec<R>> {
    downloader.check_cancelled()?;
    let next = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    let results = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..4.min(items.len()))
            .map(|_| {
                scope.spawn(|| {
                    let mut results = Vec::new();
                    while !failed.load(Ordering::Relaxed) {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = items.get(index) else {
                            break;
                        };
                        match downloader.check_cancelled().and_then(|()| work(item)) {
                            Ok(value) => results.push((index, value)),
                            Err(error) => {
                                failed.store(true, Ordering::Relaxed);
                                return Err(error);
                            }
                        }
                    }
                    Ok(results)
                })
            })
            .collect();
        workers
            .into_iter()
            .map(|worker| {
                worker
                    .join()
                    .map_err(|_| "整合包处理线程异常。".to_owned())?
            })
            .collect::<Vec<Result<Vec<_>>>>()
    });
    downloader.check_cancelled()?;
    let mut ordered = Vec::with_capacity(items.len());
    for result in results {
        ordered.extend(result?);
    }
    ordered.sort_unstable_by_key(|(index, _)| *index);
    Ok(ordered.into_iter().map(|(_, value)| value).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        sync::{Arc, Mutex, mpsc},
        time::Duration,
    };

    fn downloader(cancel: Arc<AtomicBool>) -> Downloader {
        Downloader::new(Default::default(), cancel, Arc::new(|_| {})).unwrap()
    }

    #[test]
    fn workers_take_more_files_before_a_slow_file_finishes_and_preserve_order() {
        let d = downloader(Arc::new(AtomicBool::new(false)));
        let (sender, receiver) = mpsc::channel();
        let receiver = Mutex::new(receiver);
        let results = parallel_map(&[0, 1, 2, 3, 4, 5], &d, |&item| {
            if item == 0 {
                receiver
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(5))
                    .map_err(|_| "空闲线程没有继续处理后续文件。".to_owned())?;
            } else if item == 4 {
                sender.send(()).unwrap();
            }
            Ok(item * 2)
        })
        .unwrap();
        assert_eq!(results, [0, 2, 4, 6, 8, 10]);
    }

    #[test]
    fn errors_and_cancellation_are_propagated() {
        let cancel = Arc::new(AtomicBool::new(false));
        let d = downloader(cancel.clone());
        assert_eq!(
            parallel_map(&[0], &d, |_| Err::<(), _>("failed".into())).unwrap_err(),
            "failed"
        );
        cancel.store(true, Ordering::Relaxed);
        assert!(parallel_map::<_, ()>(&[0], &d, |_| panic!("cancelled work ran")).is_err());
    }
}

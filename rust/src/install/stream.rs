//! Bounded rendezvous adapter: the writer remains on its owning thread while
//! worker callbacks wait for acknowledgement. No unsafe Send/Sync writer casts.
use super::Error;
use std::{io, sync::mpsc};
pub fn stream<E: Send, R: Send, F>(
    run: F,
    mut emit: impl FnMut(E) -> io::Result<()>,
) -> io::Result<R>
where
    F: FnOnce(&(dyn Fn(E) -> Result<(), Error> + Sync)) -> R + Send,
{
    std::thread::scope(|scope| {
        let (tx, rx) = mpsc::sync_channel::<(E, mpsc::SyncSender<Result<(), Error>>)>(0);
        let worker = scope.spawn(move || {
            let sink = move |event| {
                let (ack, wait) = mpsc::sync_channel(0);
                tx.send((event, ack))
                    .map_err(|_| Error::named("WriteFailed"))?;
                wait.recv().map_err(|_| Error::named("WriteFailed"))?
            };
            run(&sink)
        });
        let mut failed = None;
        for (event, ack) in rx {
            let result = if failed.is_some() {
                Err(Error::named("WriteFailed"))
            } else {
                match emit(event) {
                    Ok(()) => Ok(()),
                    Err(e) => {
                        failed = Some(e);
                        Err(Error::named("WriteFailed"))
                    }
                }
            };
            let _ = ack.send(result);
        }
        let result = worker
            .join()
            .map_err(|_| io::Error::other("event worker failed"))?;
        if let Some(e) = failed {
            Err(e)
        } else {
            Ok(result)
        }
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    #[test]
    fn concurrent_failed_writer_acknowledges_all_inflight_workers() {
        let finished = AtomicUsize::new(0);
        let mut writes = 0;
        let result = stream(
            |sink| {
                std::thread::scope(|scope| {
                    for _ in 0..8 {
                        let finished = &finished;
                        scope.spawn(move || {
                            for _ in 0..4 {
                                assert_eq!(sink(1).unwrap_err().name(), "WriteFailed");
                            }
                            finished.fetch_add(1, Ordering::SeqCst);
                        });
                    }
                });
            },
            |_| {
                writes += 1;
                Err(io::Error::new(io::ErrorKind::BrokenPipe, "fixture"))
            },
        );
        assert!(result.is_err());
        assert_eq!(writes, 1);
        assert_eq!(finished.load(Ordering::SeqCst), 8);
    }
    #[test]
    fn writer_panic_drops_receiver_and_joins_worker() {
        let finished = AtomicUsize::new(0);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _ = stream(
                |sink| {
                    assert_eq!(sink(1).unwrap_err().name(), "WriteFailed");
                    finished.fetch_add(1, Ordering::SeqCst);
                },
                |_| panic!("fixture writer"),
            );
        }));
        assert!(result.is_err());
        assert_eq!(finished.load(Ordering::SeqCst), 1);
    }
    #[test]
    fn worker_panic_closes_channel_without_deadlock() {
        let result = stream::<u8, (), _>(|_| panic!("fixture worker"), |_| Ok(()));
        assert!(result.is_err());
    }
}

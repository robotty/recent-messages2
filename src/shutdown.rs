use std::cmp::Ordering;
use std::ops::Deref;
use tokio::sync::watch;

#[derive(Eq, PartialEq, Copy, Clone, Debug)]
pub struct ShutdownNotice {
    pub graceful: bool,
}

impl Ord for ShutdownNotice {
    fn cmp(&self, other: &Self) -> Ordering {
        // this intentionally backwards to make it so
        // ShutdownNotice { graceful: true } < ShutdownNotice { graceful: false }
        other.graceful.cmp(&self.graceful)
    }
}

impl PartialOrd for ShutdownNotice {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl ShutdownNotice {
    fn ultimate() -> ShutdownNotice {
        ShutdownNotice { graceful: false }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunState {
    Run,
    Shutdown(ShutdownNotice),
}

impl Ord for RunState {
    fn cmp(&self, other: &Self) -> Ordering {
        match (self, other) {
            (RunState::Run, RunState::Run) => Ordering::Equal,
            (RunState::Run, RunState::Shutdown(_)) => Ordering::Less,
            (RunState::Shutdown(_), RunState::Run) => Ordering::Greater,
            (RunState::Shutdown(self_notice), RunState::Shutdown(other_notice)) => {
                self_notice.cmp(other_notice)
            }
        }
    }
}

impl PartialOrd for RunState {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

pub struct ShutdownNoticeReceiver {
    inner: watch::Receiver<RunState>,
    last_returned_value: Option<ShutdownNotice>,
}

impl ShutdownNoticeReceiver {
    pub async fn next_shutdown_notice(&mut self) -> ShutdownNotice {
        if !self.may_have_more_notices() {
            futures::future::pending::<()>().await;
            unreachable!();
        }

        loop {
            match self.inner.changed().await {
                Ok(()) => match *self.inner.borrow_and_update() {
                    // RunState::Run is only received once on the first poll
                    // of the `inner` (watch::Receiver<RunState>)
                    RunState::Run => continue,
                    RunState::Shutdown(shutdown_notice) => {
                        tracing::debug!(
                            "ShutdownNoticeReceiver: received notice {:?}",
                            shutdown_notice
                        );
                        self.last_returned_value = Some(shutdown_notice);
                        return shutdown_notice;
                    }
                },
                Err(_) => {
                    // sender has been dropped
                    // this means we send the ultimate shutdown notice
                    self.last_returned_value = Some(ShutdownNotice::ultimate());
                    return ShutdownNotice::ultimate();
                }
            }
        }
    }

    pub fn may_have_more_notices(&self) -> bool {
        self.last_returned_value < Some(ShutdownNotice::ultimate())
    }
}

impl Clone for ShutdownNoticeReceiver {
    fn clone(&self) -> ShutdownNoticeReceiver {
        ShutdownNoticeReceiver {
            inner: self.inner.clone(),
            last_returned_value: None,
        }
    }
}

pub struct ShutdownNoticeSender {
    inner: watch::Sender<RunState>,
}

impl ShutdownNoticeSender {
    fn current_value(&self) -> RunState {
        self.inner.borrow().deref().clone()
    }

    pub fn initiate_shutdown(&self, graceful: bool) {
        let new_run_state = RunState::Shutdown(ShutdownNotice { graceful });
        // only send if the new state is more severe than before
        if self.current_value() < new_run_state {
            tracing::debug!(
                "ShutdownNoticeSender: updating state to {:?}",
                new_run_state
            );
            self.inner.send(new_run_state).ok(); // .ok(): Ignore if all receivers have been dropped
        }
    }

    pub fn next_shutdown_severity(&self) {
        match self.current_value() {
            RunState::Run => {
                self.initiate_shutdown(true);
            }
            RunState::Shutdown(ShutdownNotice { graceful: true }) => {
                self.initiate_shutdown(false);
            }
            RunState::Shutdown(ShutdownNotice { graceful: false }) => {}
        }
    }

    pub fn is_in_shutdown_mode(&self) -> bool {
        match self.current_value() {
            RunState::Run => false,
            RunState::Shutdown(_) => true,
        }
    }
}

pub fn new_pair() -> (ShutdownNoticeSender, ShutdownNoticeReceiver) {
    let (tx, rx) = watch::channel(RunState::Run);

    (
        ShutdownNoticeSender { inner: tx },
        ShutdownNoticeReceiver {
            inner: rx,
            last_returned_value: None,
        },
    )
}

#[cfg(unix)]
pub mod unix {
    use tokio::signal::unix::{signal, Signal, SignalKind};

    pub struct UnixShutdownSignal {
        sigint: Signal,
        sigterm: Signal,
    }

    impl UnixShutdownSignal {
        pub fn new() -> UnixShutdownSignal {
            UnixShutdownSignal {
                sigint: signal(SignalKind::interrupt()).expect("Failed to listen to SIGINT"),
                sigterm: signal(SignalKind::terminate()).expect("Failed to listen to SIGTERM"),
            }
        }

        pub async fn next_signal(&mut self) {
            tokio::select! {
                _ = self.sigint.recv() => {},
                _ = self.sigterm.recv() => {}
            }
        }
    }
}

#[cfg(not(unix))]
pub mod universal {
    pub struct UniversalShutdownSignal;

    impl UniversalShutdownSignal {
        pub fn new() -> UniversalShutdownSignal {
            UniversalShutdownSignal
        }

        pub async fn next_signal(&mut self) {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen to Ctrl-C event");
        }
    }
}

#[cfg(unix)]
pub type ShutdownSignal = unix::UnixShutdownSignal;
#[cfg(not(unix))]
pub type ShutdownSignal = universal::UniversalShutdownSignal;

#[cfg(test)]
mod test {
    use crate::shutdown::{RunState, ShutdownNotice};
    use std::cmp::Ordering;

    #[test]
    fn test_runstate_comparison() {
        assert_eq!(RunState::Run.cmp(&RunState::Run), Ordering::Equal);
        assert_eq!(
            RunState::Run.cmp(&RunState::Shutdown(ShutdownNotice { graceful: true })),
            Ordering::Less
        );
        assert_eq!(
            RunState::Run.cmp(&RunState::Shutdown(ShutdownNotice { graceful: false })),
            Ordering::Less
        );

        assert_eq!(
            RunState::Shutdown(ShutdownNotice { graceful: true }).cmp(&RunState::Run),
            Ordering::Greater
        );
        assert_eq!(
            RunState::Shutdown(ShutdownNotice { graceful: true })
                .cmp(&RunState::Shutdown(ShutdownNotice { graceful: true })),
            Ordering::Equal
        );
        assert_eq!(
            RunState::Shutdown(ShutdownNotice { graceful: true })
                .cmp(&RunState::Shutdown(ShutdownNotice { graceful: false })),
            Ordering::Less
        );

        assert_eq!(
            RunState::Shutdown(ShutdownNotice { graceful: false }).cmp(&RunState::Run),
            Ordering::Greater
        );
        assert_eq!(
            RunState::Shutdown(ShutdownNotice { graceful: false })
                .cmp(&RunState::Shutdown(ShutdownNotice { graceful: true })),
            Ordering::Greater
        );
        assert_eq!(
            RunState::Shutdown(ShutdownNotice { graceful: false })
                .cmp(&RunState::Shutdown(ShutdownNotice { graceful: false })),
            Ordering::Equal
        );
    }

    #[test]
    fn test_shutdown_notice_ordering() {
        assert_eq!(
            ShutdownNotice { graceful: false }.cmp(&ShutdownNotice { graceful: false }),
            Ordering::Equal
        );
        assert_eq!(
            ShutdownNotice { graceful: false }.cmp(&ShutdownNotice { graceful: true }),
            Ordering::Greater
        );

        assert_eq!(
            ShutdownNotice { graceful: true }.cmp(&ShutdownNotice { graceful: false }),
            Ordering::Less
        );
        assert_eq!(
            ShutdownNotice { graceful: true }.cmp(&ShutdownNotice { graceful: true }),
            Ordering::Equal
        );
    }
}

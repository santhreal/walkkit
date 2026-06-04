//! Iterator adapter over walk results (streaming channel or sorted buffer).

use crate::WalkItem;
use crossbeam_channel::Receiver;

/// Backing storage for a [`WalkItemIter`].
pub(crate) enum WalkItemIterInner {
    /// Live stream from worker threads.
    Streaming(Receiver<WalkItem>),
    /// Pre-collected items (e.g. sorted mode).
    Buffered(std::vec::IntoIter<WalkItem>),
}

/// Iterator over walk results: discovered files and traversal errors.
pub struct WalkItemIter {
    pub(crate) inner: WalkItemIterInner,
}

impl Iterator for WalkItemIter {
    type Item = WalkItem;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            WalkItemIterInner::Streaming(rx) => rx.recv().ok(),
            WalkItemIterInner::Buffered(it) => it.next(),
        }
    }
}

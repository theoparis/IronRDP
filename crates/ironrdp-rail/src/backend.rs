//! This module provides infrastructure for implementing an OS-specific RemoteApp backend.

use ironrdp_core::AsAny;

use crate::pdu::ExecResult;

/// OS-specific RemoteApp backend interface.
///
/// Implementors are responsible for whatever local action is needed once a remote program
/// launch has been acknowledged by the server (e.g. tracking the resulting remote window,
/// or simply logging/displaying the result for a headless test client).
pub trait RailBackend: AsAny + core::fmt::Debug + Send {
    /// Called by [`crate::Rail`] once the server has replied to the handshake, indicating that
    /// the channel is ready to accept [`crate::Rail::launch`] requests.
    fn on_ready(&mut self) {}

    /// Called by [`crate::Rail`] when the server responds to a launch request.
    fn on_execute_result(&mut self, exe_or_file: &str, result: ExecResult, raw_result: u32) {
        let _ = (exe_or_file, result, raw_result);
    }
}

/// A [`RailBackend`] that does nothing beyond what [`crate::Rail`] itself requires.
///
/// Useful for callers that only need the RAIL handshake and execute request/response
/// plumbing (e.g. a test client), without any further OS integration.
#[derive(Debug, Default)]
pub struct NoopRailBackend;

impl AsAny for NoopRailBackend {
    #[inline]
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

impl RailBackend for NoopRailBackend {}

#![cfg_attr(doc, doc = include_str!("../README.md"))]
#![doc(html_logo_url = "https://cdnweb.devolutions.net/images/projects/devolutions/logos/devolutions-icon-shadow.svg")]

pub mod backend;
pub mod pdu;

use ironrdp_core::{AsAny, decode};
use ironrdp_pdu::gcc::ChannelName;
use ironrdp_pdu::{PduResult, decode_err};
use ironrdp_svc::{
    ChannelFlags, CompressionCondition, SvcClientProcessor, SvcMessage, SvcProcessor, SvcProcessorMessages,
};
use tracing::{debug, trace, warn};

use backend::RailBackend;
use pdu::{ClientExecute, ClientStatus, ClientStatusFlags, ExecFlags, Handshake, RailPdu};

/// PDUs for sending to the server on the `rail` channel.
pub type RailSvcMessages = SvcProcessorMessages<Rail>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RailState {
    /// Waiting for the server's initial Handshake (Ex) PDU.
    AwaitingHandshake,
    /// Handshake completed, channel ready to accept launch requests.
    Ready,
}

/// A pending remote program launch request, queued when [`Rail::launch`] is called before the
/// handshake has completed.
#[derive(Debug, Clone)]
struct PendingLaunch {
    flags: ExecFlags,
    exe_or_file: String,
    working_dir: String,
    arguments: String,
}

/// RAIL (RemoteApp) static virtual channel client-side endpoint implementation.
///
/// Drives the [MS-RDPERP] handshake and exposes [`Rail::launch`] to request that the server
/// start a remote program.
///
/// [MS-RDPERP]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/
#[derive(Debug)]
pub struct Rail {
    backend: Box<dyn RailBackend>,
    state: RailState,
    client_status_flags: ClientStatusFlags,
    pending_launch: Option<PendingLaunch>,
}

impl SvcClientProcessor for Rail {}

impl AsAny for Rail {
    #[inline]
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    #[inline]
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

impl Rail {
    const CHANNEL_NAME: ChannelName = ChannelName::from_static(b"rail\0\0\0\0");

    /// Creates a new RAIL channel processor.
    ///
    /// `client_status_flags` are sent to the server in the Client Information PDU once the
    /// handshake completes; [`ClientStatusFlags::empty()`] is a safe default.
    pub fn new(backend: Box<dyn RailBackend>, client_status_flags: ClientStatusFlags) -> Self {
        Self {
            backend,
            state: RailState::AwaitingHandshake,
            client_status_flags,
            pending_launch: None,
        }
    }

    pub fn downcast_backend<T: RailBackend>(&self) -> Option<&T> {
        self.backend.as_any().downcast_ref::<T>()
    }

    pub fn downcast_backend_mut<T: RailBackend>(&mut self) -> Option<&mut T> {
        self.backend.as_any_mut().downcast_mut::<T>()
    }

    /// Requests that the server start (or activate) a remote program.
    ///
    /// If the RAIL handshake has not completed yet, the request is queued and sent as soon as
    /// it does; only the most recently queued request is kept. Returns the PDUs to send on the
    /// channel, which may be empty if the request had to be queued.
    pub fn launch(&mut self, exe_or_file: &str, working_dir: &str, arguments: &str) -> PduResult<RailSvcMessages> {
        let mut flags = ExecFlags::empty();
        if !working_dir.is_empty() {
            flags |= ExecFlags::EXPAND_WORKING_DIRECTORY;
        }
        if !arguments.is_empty() {
            flags |= ExecFlags::EXPAND_ARGUMENTS;
        }

        let request = PendingLaunch {
            flags,
            exe_or_file: exe_or_file.to_owned(),
            working_dir: working_dir.to_owned(),
            arguments: arguments.to_owned(),
        };

        match self.state {
            RailState::Ready => Ok(vec![into_rail_message(Self::build_exec(&request))].into()),
            RailState::AwaitingHandshake => {
                debug!("Queuing RemoteApp launch request until RAIL handshake completes");
                self.pending_launch = Some(request);
                Ok(Vec::new().into())
            }
        }
    }

    fn build_exec(request: &PendingLaunch) -> RailPdu {
        RailPdu::ClientExecute(ClientExecute {
            flags: request.flags,
            exe_or_file: request.exe_or_file.clone(),
            working_dir: request.working_dir.clone(),
            arguments: request.arguments.clone(),
        })
    }

    fn handle_handshake(&mut self, handshake: Handshake) -> PduResult<Vec<SvcMessage>> {
        let mut messages = vec![into_rail_message(RailPdu::Handshake(Handshake::reply(
            handshake.build_number(),
        )))];

        messages.push(into_rail_message(RailPdu::ClientStatus(ClientStatus {
            flags: self.client_status_flags,
        })));

        if self.state == RailState::AwaitingHandshake {
            self.state = RailState::Ready;
            self.backend.on_ready();
        }

        if let Some(request) = self.pending_launch.take() {
            messages.push(into_rail_message(Self::build_exec(&request)));
        }

        Ok(messages)
    }
}

impl SvcProcessor for Rail {
    fn channel_name(&self) -> ChannelName {
        Self::CHANNEL_NAME
    }

    fn compression_condition(&self) -> CompressionCondition {
        CompressionCondition::Never
    }

    fn process(&mut self, payload: &[u8]) -> PduResult<Vec<SvcMessage>> {
        let pdu: RailPdu = decode(payload).map_err(|e| decode_err!(e))?;

        match pdu {
            RailPdu::Handshake(handshake) => self.handle_handshake(handshake),
            RailPdu::ServerExecuteResult(result) => {
                self.backend
                    .on_execute_result(&result.exe_or_file, result.result, result.raw_result);
                Ok(Vec::new())
            }
            RailPdu::ClientStatus(_) | RailPdu::ClientExecute(_) => {
                warn!(
                    message = pdu.message_name(),
                    "Received client-originated RAIL PDU from server, ignoring"
                );
                Ok(Vec::new())
            }
            RailPdu::Unknown { order_type } => {
                trace!(order_type, "Unhandled RAIL order type");
                Ok(Vec::new())
            }
        }
    }
}

fn into_rail_message(pdu: RailPdu) -> SvcMessage {
    SvcMessage::from(pdu).with_flags(ChannelFlags::empty())
}

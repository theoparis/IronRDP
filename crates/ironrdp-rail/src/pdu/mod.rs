//! This module implements RDP RemoteApp channel PDUs encode/decode logic as defined in
//! [MS-RDPERP]: Remote Desktop Protocol: Remote Programs Virtual Channel Extension.
//!
//! [MS-RDPERP]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/

use bitflags::bitflags;
use ironrdp_core::{
    Decode, DecodeResult, Encode, EncodeResult, ReadCursor, WriteCursor, ensure_fixed_part_size, invalid_field_err,
};
use ironrdp_pdu::utils::{CharacterSet, read_string_from_cursor, write_string_to_cursor};
use ironrdp_svc::SvcEncode;

const ORDER_TYPE_EXEC: u16 = 0x0001;
const ORDER_TYPE_EXEC_RESULT: u16 = 0x0004;
const ORDER_TYPE_CLIENTSTATUS: u16 = 0x0002;
const ORDER_TYPE_HANDSHAKE: u16 = 0x0003;
const ORDER_TYPE_HANDSHAKE_EX: u16 = 0x0012;

/// [2.2.1.1] `TS_RAIL_PDU_HEADER`
///
/// [2.2.1.1]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/45592c6a-1fd1-4433-8934-6e4f7db0dfef
struct RailPduHeader {
    order_type: u16,
    order_length: u16,
}

impl RailPduHeader {
    const NAME: &'static str = "TS_RAIL_PDU_HEADER";
    const FIXED_PART_SIZE: usize = 2 /* orderType */ + 2 /* orderLength */;
    const SIZE: usize = Self::FIXED_PART_SIZE;

    fn new(order_type: u16, body_size: usize) -> EncodeResult<Self> {
        let order_length = ironrdp_core::cast_length!("orderLength", Self::SIZE + body_size)?;

        Ok(Self {
            order_type,
            order_length,
        })
    }
}

impl Encode for RailPduHeader {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        ensure_fixed_part_size!(in: dst);

        dst.write_u16(self.order_type);
        dst.write_u16(self.order_length);

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        Self::FIXED_PART_SIZE
    }
}

impl<'de> Decode<'de> for RailPduHeader {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        ensure_fixed_part_size!(in: src);

        let order_type = src.read_u16();
        let order_length = src.read_u16();

        Ok(Self {
            order_type,
            order_length,
        })
    }
}

/// A non null-terminated UTF-16LE string, prefixed on the wire by its length in bytes.
///
/// Used pervasively throughout [MS-RDPERP] for variable-length string fields
/// (e.g. `ExeOrFile`, `WorkingDir`, `Arguments`).
fn read_rail_string(src: &mut ReadCursor<'_>, byte_len: usize, field: &'static str) -> DecodeResult<String> {
    let slice = src.read_slice(byte_len);
    let mut cursor = ReadCursor::new(slice);
    read_string_from_cursor(&mut cursor, CharacterSet::Unicode, false)
        .map_err(|_| invalid_field_err!(field, "failed to decode UTF-16 string"))
}

fn rail_string_byte_len(value: &str) -> usize {
    value.encode_utf16().count() * 2
}

fn write_rail_string(dst: &mut WriteCursor<'_>, value: &str) -> EncodeResult<()> {
    write_string_to_cursor(dst, value, CharacterSet::Unicode, false)
}

bitflags! {
    /// Flags of the [`ClientStatus`] PDU (`TS_RAIL_CLIENTSTATUS_*`).
    ///
    /// See [MS-RDPERP] 2.2.2.2.1.
    ///
    /// [MS-RDPERP]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/45592c6a-1fd1-4433-8934-6e4f7db0dfef
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub struct ClientStatusFlags: u32 {
        /// TS_RAIL_CLIENTSTATUS_ALLOWLOCALMOVESIZE
        const ALLOW_LOCAL_MOVE_SIZE = 0x0000_0001;
        /// TS_RAIL_CLIENTSTATUS_AUTORECONNECT
        const AUTORECONNECT = 0x0000_0002;
        /// TS_RAIL_CLIENTSTATUS_ZORDER_SYNC
        const ZORDER_SYNC = 0x0000_0004;
        /// TS_RAIL_CLIENTSTATUS_WINDOW_RESIZE_MARGIN_SUPPORTED
        const WINDOW_RESIZE_MARGIN_SUPPORTED = 0x0000_0010;
        /// TS_RAIL_CLIENTSTATUS_APPBAR_REMOTING_SUPPORTED
        const APPBAR_REMOTING_SUPPORTED = 0x0000_0020;
        /// TS_RAIL_CLIENTSTATUS_POWER_DISPLAY_REQUEST_SUPPORTED
        const POWER_DISPLAY_REQUEST_SUPPORTED = 0x0000_0040;
        /// TS_RAIL_CLIENTSTATUS_GET_APPID_RESPONSE_EX_SUPPORTED
        const GET_APPID_RESPONSE_EX_SUPPORTED = 0x0000_0080;
        /// TS_RAIL_CLIENTSTATUS_BIDIRECTIONAL_CLOAK_SUPPORTED
        const BIDIRECTIONAL_CLOAK_SUPPORTED = 0x0000_0100;
    }
}

bitflags! {
    /// Flags of the [`ClientExecute`] PDU (`TS_RAIL_EXEC_FLAG_*`).
    ///
    /// See [MS-RDPERP] 2.2.2.3.1.
    ///
    /// [MS-RDPERP]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/e0ecca9e-712a-4713-aec6-15fe1baa5cc0
    #[derive(Debug, PartialEq, Eq, Clone, Copy)]
    pub struct ExecFlags: u16 {
        /// TS_RAIL_EXEC_FLAG_EXPAND_ARGUMENTS
        const EXPAND_ARGUMENTS = 0x0001;
        /// TS_RAIL_EXEC_FLAG_TRANSLATE_FILES
        const TRANSLATE_FILES = 0x0002;
        /// TS_RAIL_EXEC_FLAG_EXPAND_WORKINGDIRECTORY
        const EXPAND_WORKING_DIRECTORY = 0x0004;
        /// TS_RAIL_EXEC_FLAG_FILE
        const FILE = 0x0008;
        /// TS_RAIL_EXEC_FLAG_APP_USER_MODEL_ID
        const APP_USER_MODEL_ID = 0x0010;
    }
}

/// Result code of the [`ServerExecuteResult`] PDU (`TS_RAIL_EXEC_*`).
///
/// See [MS-RDPERP] 2.2.2.3.2.
///
/// [MS-RDPERP]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/c8a5db64-c4b3-4a95-9c60-ba39c3062bfa
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecResult {
    /// RAIL_EXEC_S_OK
    Ok,
    /// RAIL_EXEC_E_HOOK_NOT_LOADED
    HookNotLoaded,
    /// RAIL_EXEC_E_DECODE_FAILED
    DecodeFailed,
    /// RAIL_EXEC_E_NOT_IN_ALLOWLIST
    NotInAllowList,
    /// RAIL_EXEC_E_FILE_NOT_FOUND
    FileNotFound,
    /// RAIL_EXEC_E_FAIL
    Fail,
    /// RAIL_EXEC_E_SESSION_LOCKED
    SessionLocked,
    /// Unknown/reserved result code, preserved as received.
    Other(u16),
}

impl ExecResult {
    fn from_u16(value: u16) -> Self {
        match value {
            0 => Self::Ok,
            1 => Self::HookNotLoaded,
            2 => Self::DecodeFailed,
            3 => Self::NotInAllowList,
            5 => Self::FileNotFound,
            6 => Self::Fail,
            7 => Self::SessionLocked,
            other => Self::Other(other),
        }
    }

    fn to_u16(self) -> u16 {
        match self {
            Self::Ok => 0,
            Self::HookNotLoaded => 1,
            Self::DecodeFailed => 2,
            Self::NotInAllowList => 3,
            Self::FileNotFound => 5,
            Self::Fail => 6,
            Self::SessionLocked => 7,
            Self::Other(value) => value,
        }
    }

    /// Returns `true` if the remote program execution request succeeded.
    pub fn is_ok(self) -> bool {
        matches!(self, Self::Ok)
    }
}

/// [2.2.2.2.2] / [2.2.2.2.3] Handshake PDU (`TS_RAIL_ORDER_HANDSHAKE` / `TS_RAIL_ORDER_HANDSHAKE_EX`)
///
/// Sent by the server to initiate the RAIL handshake, and echoed back (as a plain
/// [`Handshake`], never [`Handshake::Ex`]) by the client per [MS-RDPERP] 3.2.5.1.
///
/// [2.2.2.2.2]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/954c48d3-8f78-4238-a67e-3c193c6a2e94
/// [2.2.2.2.3]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/b788be97-6cdb-4d61-a94d-c74d4c17e0a3
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Handshake {
    /// `TS_RAIL_ORDER_HANDSHAKE`
    Basic { build_number: u32 },
    /// `TS_RAIL_ORDER_HANDSHAKE_EX`
    Ex {
        build_number: u32,
        rail_handshake_flags: u32,
    },
}

impl Handshake {
    const NAME: &'static str = "TS_RAIL_ORDER_HANDSHAKE";
    const BASIC_SIZE: usize = 4 /* buildNumber */;
    const EX_SIZE: usize = 4 /* buildNumber */ + 4 /* railHandshakeFlags */;

    pub fn build_number(&self) -> u32 {
        match self {
            Self::Basic { build_number } | Self::Ex { build_number, .. } => *build_number,
        }
    }

    /// Builds the plain (non-Ex) reply the client must send back to the server.
    pub fn reply(build_number: u32) -> Self {
        Self::Basic { build_number }
    }
}

impl Encode for Handshake {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        match self {
            Self::Basic { build_number } => {
                let header = RailPduHeader::new(ORDER_TYPE_HANDSHAKE, Self::BASIC_SIZE)?;
                header.encode(dst)?;
                dst.write_u32(*build_number);
            }
            Self::Ex {
                build_number,
                rail_handshake_flags,
            } => {
                let header = RailPduHeader::new(ORDER_TYPE_HANDSHAKE_EX, Self::EX_SIZE)?;
                header.encode(dst)?;
                dst.write_u32(*build_number);
                dst.write_u32(*rail_handshake_flags);
            }
        }

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        RailPduHeader::SIZE
            + match self {
                Self::Basic { .. } => Self::BASIC_SIZE,
                Self::Ex { .. } => Self::EX_SIZE,
            }
    }
}

impl<'de> Decode<'de> for Handshake {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        let header = RailPduHeader::decode(src)?;

        match header.order_type {
            ORDER_TYPE_HANDSHAKE => {
                ironrdp_core::ensure_size!(in: src, size: Self::BASIC_SIZE);
                let build_number = src.read_u32();
                Ok(Self::Basic { build_number })
            }
            ORDER_TYPE_HANDSHAKE_EX => {
                ironrdp_core::ensure_size!(in: src, size: Self::EX_SIZE);
                let build_number = src.read_u32();
                let rail_handshake_flags = src.read_u32();
                Ok(Self::Ex {
                    build_number,
                    rail_handshake_flags,
                })
            }
            _ => Err(invalid_field_err!("orderType", "unexpected RAIL handshake order type")),
        }
    }
}

/// [2.2.2.2.1] `TS_RAIL_ORDER_CLIENTSTATUS`
///
/// [2.2.2.2.1]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/53c045e6-3999-4c30-855a-3fc86d0d827e
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClientStatus {
    pub flags: ClientStatusFlags,
}

impl ClientStatus {
    const NAME: &'static str = "TS_RAIL_ORDER_CLIENTSTATUS";
    const INNER_SIZE: usize = 4 /* Flags */;
}

impl Encode for ClientStatus {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        let header = RailPduHeader::new(ORDER_TYPE_CLIENTSTATUS, Self::INNER_SIZE)?;
        header.encode(dst)?;
        dst.write_u32(self.flags.bits());
        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        RailPduHeader::SIZE + Self::INNER_SIZE
    }
}

impl<'de> Decode<'de> for ClientStatus {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        let _header = RailPduHeader::decode(src)?;
        ironrdp_core::ensure_size!(in: src, size: Self::INNER_SIZE);
        let flags = ClientStatusFlags::from_bits_retain(src.read_u32());
        Ok(Self { flags })
    }
}

/// [2.2.2.3.1] `TS_RAIL_ORDER_EXEC`
///
/// Requests that the server start (or activate, if already running) a remote program.
///
/// [2.2.2.3.1]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/e0ecca9e-712a-4713-aec6-15fe1baa5cc0
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientExecute {
    pub flags: ExecFlags,
    pub exe_or_file: String,
    pub working_dir: String,
    pub arguments: String,
}

impl ClientExecute {
    const NAME: &'static str = "TS_RAIL_ORDER_EXEC";
    const FIXED_PART_SIZE: usize = 2 /* Flags */ + 2 /* ExeOrFileLength */ + 2 /* WorkingDirLength */ + 2 /* ArgumentsLength */;

    fn inner_size(&self) -> usize {
        Self::FIXED_PART_SIZE
            + rail_string_byte_len(&self.exe_or_file)
            + rail_string_byte_len(&self.working_dir)
            + rail_string_byte_len(&self.arguments)
    }
}

impl Encode for ClientExecute {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        let header = RailPduHeader::new(ORDER_TYPE_EXEC, self.inner_size())?;
        header.encode(dst)?;

        ensure_fixed_part_size!(in: dst);
        dst.write_u16(self.flags.bits());
        dst.write_u16(ironrdp_core::cast_length!(
            "ExeOrFileLength",
            rail_string_byte_len(&self.exe_or_file)
        )?);
        dst.write_u16(ironrdp_core::cast_length!(
            "WorkingDirLength",
            rail_string_byte_len(&self.working_dir)
        )?);
        dst.write_u16(ironrdp_core::cast_length!(
            "ArgumentsLength",
            rail_string_byte_len(&self.arguments)
        )?);

        write_rail_string(dst, &self.exe_or_file)?;
        write_rail_string(dst, &self.working_dir)?;
        write_rail_string(dst, &self.arguments)?;

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        RailPduHeader::SIZE + self.inner_size()
    }
}

impl<'de> Decode<'de> for ClientExecute {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        let _header = RailPduHeader::decode(src)?;

        ensure_fixed_part_size!(in: src);
        let flags = ExecFlags::from_bits_retain(src.read_u16());
        let exe_or_file_len = usize::from(src.read_u16());
        let working_dir_len = usize::from(src.read_u16());
        let arguments_len = usize::from(src.read_u16());

        let exe_or_file = read_rail_string(src, exe_or_file_len, "ExeOrFile")?;
        let working_dir = read_rail_string(src, working_dir_len, "WorkingDir")?;
        let arguments = read_rail_string(src, arguments_len, "Arguments")?;

        Ok(Self {
            flags,
            exe_or_file,
            working_dir,
            arguments,
        })
    }
}

/// [2.2.2.3.2] `TS_RAIL_ORDER_EXEC_RESULT`
///
/// Sent by the server in response to a [`ClientExecute`] request.
///
/// [2.2.2.3.2]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/c8a5db64-c4b3-4a95-9c60-ba39c3062bfa
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerExecuteResult {
    pub flags: ExecFlags,
    pub result: ExecResult,
    pub raw_result: u32,
    pub exe_or_file: String,
}

impl ServerExecuteResult {
    const NAME: &'static str = "TS_RAIL_ORDER_EXEC_RESULT";
    // Flags(2) + ExecResult(2) + RawResult(4) + Padding(2) + ExeOrFileLength(2)
    const FIXED_PART_SIZE: usize = 2 + 2 + 4 + 2 + 2;

    fn inner_size(&self) -> usize {
        Self::FIXED_PART_SIZE + rail_string_byte_len(&self.exe_or_file)
    }
}

impl Encode for ServerExecuteResult {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        let header = RailPduHeader::new(ORDER_TYPE_EXEC_RESULT, self.inner_size())?;
        header.encode(dst)?;

        ensure_fixed_part_size!(in: dst);
        dst.write_u16(self.flags.bits());
        dst.write_u16(self.result.to_u16());
        dst.write_u32(self.raw_result);
        dst.write_u16(0); // Padding
        dst.write_u16(ironrdp_core::cast_length!(
            "ExeOrFileLength",
            rail_string_byte_len(&self.exe_or_file)
        )?);

        write_rail_string(dst, &self.exe_or_file)?;

        Ok(())
    }

    fn name(&self) -> &'static str {
        Self::NAME
    }

    fn size(&self) -> usize {
        RailPduHeader::SIZE + self.inner_size()
    }
}

impl<'de> Decode<'de> for ServerExecuteResult {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        let _header = RailPduHeader::decode(src)?;

        ensure_fixed_part_size!(in: src);
        let flags = ExecFlags::from_bits_retain(src.read_u16());
        let result = ExecResult::from_u16(src.read_u16());
        let raw_result = src.read_u32();
        let _padding = src.read_u16();
        let exe_or_file_len = usize::from(src.read_u16());

        let exe_or_file = read_rail_string(src, exe_or_file_len, "ExeOrFile")?;

        Ok(Self {
            flags,
            result,
            raw_result,
            exe_or_file,
        })
    }
}

/// Any PDU exchanged on the `rail` static virtual channel that this crate understands.
///
/// Unrecognized order types (e.g. window order PDUs, not yet implemented by this crate) are
/// preserved as [`RailPdu::Unknown`] so that callers can at least observe channel activity
/// without the channel processor failing outright.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RailPdu {
    Handshake(Handshake),
    ClientStatus(ClientStatus),
    ClientExecute(ClientExecute),
    ServerExecuteResult(ServerExecuteResult),
    /// An order type this crate does not (yet) decode further.
    Unknown {
        order_type: u16,
    },
}

impl RailPdu {
    pub fn message_name(&self) -> &'static str {
        match self {
            Self::Handshake(Handshake::Basic { .. }) => "TS_RAIL_ORDER_HANDSHAKE",
            Self::Handshake(Handshake::Ex { .. }) => "TS_RAIL_ORDER_HANDSHAKE_EX",
            Self::ClientStatus(_) => "TS_RAIL_ORDER_CLIENTSTATUS",
            Self::ClientExecute(_) => "TS_RAIL_ORDER_EXEC",
            Self::ServerExecuteResult(_) => "TS_RAIL_ORDER_EXEC_RESULT",
            Self::Unknown { .. } => "TS_RAIL_ORDER_UNKNOWN",
        }
    }
}

impl<'de> Decode<'de> for RailPdu {
    fn decode(src: &mut ReadCursor<'de>) -> DecodeResult<Self> {
        // Peek the order type without consuming the header, so specific PDU decoders can
        // re-read it as part of their own `RailPduHeader::decode` call.
        let mut peek = ReadCursor::new(src.remaining());
        ironrdp_core::ensure_size!(in: peek, size: RailPduHeader::FIXED_PART_SIZE);
        let order_type = peek.read_u16();

        match order_type {
            ORDER_TYPE_HANDSHAKE | ORDER_TYPE_HANDSHAKE_EX => Ok(Self::Handshake(Handshake::decode(src)?)),
            ORDER_TYPE_CLIENTSTATUS => Ok(Self::ClientStatus(ClientStatus::decode(src)?)),
            ORDER_TYPE_EXEC => Ok(Self::ClientExecute(ClientExecute::decode(src)?)),
            ORDER_TYPE_EXEC_RESULT => Ok(Self::ServerExecuteResult(ServerExecuteResult::decode(src)?)),
            other => {
                let header = RailPduHeader::decode(src)?;
                let remaining_len = usize::from(header.order_length).saturating_sub(RailPduHeader::SIZE);
                let _skipped = src.read_slice(remaining_len.min(src.len()));
                Ok(Self::Unknown { order_type: other })
            }
        }
    }
}

impl Encode for RailPdu {
    fn encode(&self, dst: &mut WriteCursor<'_>) -> EncodeResult<()> {
        match self {
            Self::Handshake(pdu) => pdu.encode(dst),
            Self::ClientStatus(pdu) => pdu.encode(dst),
            Self::ClientExecute(pdu) => pdu.encode(dst),
            Self::ServerExecuteResult(pdu) => pdu.encode(dst),
            Self::Unknown { .. } => Err(invalid_field_err!("orderType", "cannot encode an unknown RAIL PDU")),
        }
    }

    fn name(&self) -> &'static str {
        self.message_name()
    }

    fn size(&self) -> usize {
        match self {
            Self::Handshake(pdu) => pdu.size(),
            Self::ClientStatus(pdu) => pdu.size(),
            Self::ClientExecute(pdu) => pdu.size(),
            Self::ServerExecuteResult(pdu) => pdu.size(),
            Self::Unknown { .. } => 0,
        }
    }
}

impl SvcEncode for RailPdu {}

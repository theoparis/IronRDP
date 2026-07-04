# ironrdp-rail

RemoteApp (RAIL) static virtual channel implemented as described in
[MS-RDPERP]: Remote Desktop Protocol: Remote Programs Virtual Channel Extension.

This crate implements the wire protocol and client-side channel state machine required to
launch and track a single remote application window (the equivalent of FreeRDP's `/app` flag),
i.e. the handshake, client status, client system parameters, and remote program execution PDUs.

Rendering/placement of the resulting remote window (moving, resizing, z-ordering, icons, ...) is
out of scope for this initial version and is left to a future extension of the PDU set and to a
native, OS-integration crate (analogous to `ironrdp-cliprdr-native`).

[MS-RDPERP]: https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdperp/

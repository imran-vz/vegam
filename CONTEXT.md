# Vegam

Vegam is a desktop-first file transfer product context. It describes the product language for sending files directly between user-owned computers.

## Language

**Device**:
A computer running Vegam.
_Avoid_: Client, node, phone

**Display Name**:
A cosmetic, user-editable name for a Device, initially generated as a random two-word name.
_Avoid_: Identity, username, account

**Peer**:
A Device participating in a Transfer with another Device.
_Avoid_: Contact, friend

**Peer Discovery**:
A way for Vegam to surface available Peers without manually sharing a Transfer Ticket.
_Avoid_: Device list, nearby devices

**Sender**:
The Peer that makes a file available for transfer.
_Avoid_: Host, uploader

**Receiver**:
The Peer that obtains a file from a Sender.
_Avoid_: Client, downloader

**Transfer**:
The movement of one file from a Sender to a Receiver.
_Avoid_: Sync, share, session

**Resumable Transfer**:
A Transfer that can continue after an interruption without starting again from the beginning.
_Avoid_: Retry, restart

**Partial Download**:
Incomplete Receiver-side Transfer data preserved so a Resumable Transfer can continue later.
_Avoid_: Temporary file, cache

**Cancellation**:
A deliberate action that ends a Transfer instead of preserving it for resume.
_Avoid_: Pause, interruption

**Content Identity**:
A stable identity for the contents of a file used to decide whether two file instances are the same Transfer payload.
_Avoid_: Filename, path, modified time

**Transfer Ticket**:
A bearer value shared by a Sender with a Receiver to start a Transfer.
_Avoid_: Link, code, token

**Desktop Release**:
The production milestone for Windows, Linux, and macOS.
_Avoid_: Cross-platform release, mobile release

**Mobile Release**:
A future production milestone for phones and tablets after the Desktop Release.
_Avoid_: Android release, side project

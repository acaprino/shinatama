# C01 — private local data location, Windows

Date: 2026-11-02. Windows 11 x64 build 26200. This is a verified Windows
implementation slice, not certification of every S2/S9/S11/D15/E4 scenario.

## Delivered behavior

The shared data root is `%LOCALAPPDATA%\shinatama`, with the exact future database
path `shinatama.db`, or the explicit absolute `SHINATAMA_HOME` directory. C01
prepares the root and exposes its diagnostic path; SQLite is introduced in C02.
Legacy B02 picker preferences remain in their canonical original directory until
the C02 import. Their location guards live as long as the owning services.

Shape and drive-type checks precede filesystem reads. Location resolution itself
creates nothing. The policy resolves existing aliases plus missing descendants,
rejects recognized project/synchronized/network roots and non-ACL filesystems,
and rechecks under retained no-delete directory handles before creating private
directories. Later project selection cannot include the active data root.

Windows directories are owner-only at creation. Existing roots must belong to
the current user; repairs preserve existing child permissions and bytes. Handle
security checks reject reparse-point files and hard links, compare file identity,
and inspect the effective owner-only protected ACL. Existing configuration bytes
are preserved. No current-user credentials are read or copied.

Storage discovery, legacy reads and permission work run on a blocking worker,
once per process, outside window setup. Storage errors leave the custom window
controls and terminal-list command available and prevent new launches through the
unavailable service. Existing process output does not depend on storage readiness.

## Executed checks

`cargo test -p shinatama-state --lib --test location -- --test-threads=1`:
**13 passed**. Includes no-write resolution/rejection, current-user ACLs, retained
bytes and existing child ACLs, junction bypass, hard links, UNC/device paths,
drive case/verbatim prefixes, project boundaries and selection after startup.

`cargo test -p shinatama-projects`: **11 passed**, including the read-only catalog
preflight, hard-link rejection before reading, malformed source preservation and
the existing concurrent picker checks.

The read-only `check_location` example was run against missing synthetic names on
real G:, H: and I: volumes. All three were refused because their filesystems do
not support the required persistent per-user ACL. C: and D: were accepted by
resolution only. No probe directory or database was created on these volumes.

The real embedded-app storage smoke passed: exact diagnostic path, conflicting
project/launch rejection, no directory created for a rejected project-contained
home, immediate UNC refusal, usable maximize/restore and close controls in both
error cases. The project-history and terminal smoke checks are recorded in the
checkpoint after execution. Their data homes now use dedicated
system-temporary directories outside the project, rather than the rejected
repository evidence directory.

## Qualification still open

Recognized roots include registered Windows sync providers, OneDrive account
roots/environment, bounded Dropbox root metadata and Google Drive mount policies
or volume properties. A custom NTFS Google Drive **mirrored** directory without
registered provider metadata is not proven detectable. SMB/NFS/offline mapped
shares, real OneDrive/Dropbox roots, 8.3 paths and every agent's project-identity
encoding still require their provider-specific S2/S9/E4 fixtures. No full gate is
closed by the successful local/default-root checks.

## Commit boundary

TL1.1 and C01 share the desktop command registration and Cargo manifest. Automatic
approval rejected both temporarily substituting those files and staging derived
partial versions, because either could omit uncommitted storage work. The safe
boundary is one atomic commit of their verified, current file contents; all source
changes remain intact. C02/C03 files are excluded until their own verification.
This recorded exception preserves the plan's individual evidence and checkpoints.

# 005-community-registry / 007 — File Picker and Clipboard Permissions

**Source:** [Part 5 § 4 Permission catalog](../../ModPlayer-Software-Specification.md#4-permission-catalog) (`files.read`, `files.write`, `clipboard`, PL-4.1, PL-4.4), [INT-6 Operating system services](../../ModPlayer-Software-Specification.md#int-6-operating-system-services) (INT-6.6), [FR-7.2 Permissions](../../ModPlayer-Software-Specification.md#fr-72-permissions) (FR-7.2.5 file access in usage log), [DM-13 UsageLogEntry](../../ModPlayer-Software-Specification.md#dm-13-usagelogentry), [NFR § 4 Security](../../ModPlayer-Software-Specification.md#4-security) (NFR-4.3)

**Prerequisites:** Assumes permission management and the usage log from 005-community-registry/002-permission-management-and-usage-log.

## Prompt

> Let a plugin import a setlist or chord file the user chooses, export its own notes to a place the user picks, or copy a marker list to the clipboard — without ever seeing a file path or touching a file the user did not point at.
>
> With `files.read` a plugin asks the host to open a file; the host shows the platform file picker, and if the user picks a file the plugin receives an opaque handle it can read through the API. With `files.write` the plugin asks for a save location; the host shows the picker and hands back an opaque handle it can write through. Plugins never receive paths, cannot enumerate directories, and cannot reopen a handle after the session ends. Both permissions are Medium risk with the explanations "Open files you choose" and "Save files where you choose". Every file read and write is recorded in the plugin's usage log with time, a label for the handle, byte volume, and outcome, never content. No file permission grants access to decoded audio, the cache, or credentials.
>
> With `clipboard`, a plugin can read or write the system clipboard only in response to a user gesture on one of its own widgets; reads outside a gesture return `invalid_state`.
>
> Acceptance: when a plugin with `files.read` requests a file and the user cancels the picker, the plugin receives `not_found` and nothing is logged as a read. When the user picks `setlist.txt`, the plugin reads its contents through the handle and the usage log shows the label and byte count. When a plugin with `files.write` attempts to write to a handle from a previous session, the call returns `invalid_state`. When a plugin tries to write the clipboard from a timer rather than a button press, it receives `invalid_state`.

## Scope boundary

Does not cover network access, MIDI permissions, or library permissions, which live in their own slices.

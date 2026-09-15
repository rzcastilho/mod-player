# SPDX-License-Identifier: MIT OR Apache-2.0
# Welcome / disclosure / privacy-notice text. Hash-pinned via
# DISCLOSURE_EN_US_SHA256 (crates/modplayer-account/src/disclosure.rs,
# `disclosure_text_is_pinned_to_bundle_version`, SC-007). Any change to this
# file must bump DISCLOSURE_BUNDLE_VERSION if the change is substantive
# (re-shows the welcome screen to every user); a pure typo fix only needs
# the hash updated (FR-004).

welcome-title = Welcome to ModPlayer
welcome-description = ModPlayer is a desktop application that connects to your Spotify Premium account so you can control playback on this device.
disclosure-unofficial = ModPlayer is an independent, unofficial application. It is not affiliated with, endorsed by, or sponsored by Spotify.
disclosure-premium-required = Playback requires an active Spotify Premium subscription. You can sign in with a Free account, but ModPlayer cannot play audio without Premium.
disclosure-terms-apply = Using ModPlayer with your Spotify account is subject to Spotify's own terms of service.
disclosure-terms-link = Spotify's terms of service
disclosure-privacy-link = Read the privacy notice
welcome-acknowledge = I understand, continue
welcome-decline = Decline
decline-explanation = You must accept the disclosure above to use ModPlayer. Nothing has been saved and no account has been contacted.
decline-quit = Quit

privacy-title = Privacy notice
privacy-body =
    ModPlayer stores your Spotify sign-in credential only in this device's operating-system credential store — Keychain on macOS, Credential Manager on Windows, or Secret Service on Linux. The credential never appears in a plain file, a log, or a notification.

    Your account identifier, display name, and subscription tier are stored locally in this device's ModPlayer configuration folder, so the app can show your account details without contacting Spotify every time it starts.

    ModPlayer does not collect analytics or telemetry, and does not share your credential or account details with any third party.

    Signing out, or revoking ModPlayer's access from your Spotify account settings, removes the stored credential and account details from this device. Your acceptance of this disclosure is unaffected by signing out.
privacy-back = Back

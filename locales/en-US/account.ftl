# SPDX-License-Identifier: MIT OR Apache-2.0
# Sign-in, tier, account settings, about, and notification keys.

## Sign-in step (contracts/ui-surface.md "Sign-in step")

signin-title = Sign in
signin-explanation = Sign in with your Spotify account to use ModPlayer.
signin-note-cancelled = Sign-in was cancelled.
signin-note-timed-out = Sign-in timed out waiting for your browser.
signin-note-service-error = Something went wrong while signing in.
signin-note-previous-unfinished = Your previous sign-in didn't finish.
signin-note-revoked = ModPlayer's access to your account was revoked.
signin-start = Sign in
signin-retry = Retry

signin-store-unavailable = ModPlayer couldn't reach your { $store }.
signin-store-remedy-keychain = Unlock your Mac's Keychain, then retry.
signin-store-remedy-credential-manager = Unlock Windows Credential Manager, then retry.
signin-store-remedy-secret-service = Unlock your Secret Service keyring (GNOME Keyring or KWallet), then retry.
store-name-keychain = Keychain
store-name-credential-manager = Credential Manager
store-name-secret-service = Secret Service keyring

signin-waiting = Waiting for your browser…
signin-waiting-resumed = Resuming your previous sign-in attempt.
signin-open-again = Open the browser again
signin-cancel = Cancel

signin-checking = Checking your account

## Tier result (contracts/ui-surface.md "Sign-in step")

tier-free-title = Playback requires Premium
tier-free-explanation = ModPlayer can control playback on this device, but Spotify Premium is required to actually play audio. You can continue with a Free account and upgrade later.
tier-open-upgrade = Open upgrade page
tier-continue = Continue

tier-unknown-title = Couldn't verify your subscription
tier-unknown-explanation = ModPlayer couldn't confirm your subscription tier right now. You can retry, or continue and check again later from Settings.
tier-retry = Retry

tier-premium = Premium
tier-free = Free
tier-unknown = Unknown

## Settings > Account (contracts/ui-surface.md "Settings > Account")

account-display-name = Signed in as { $name }
account-tier = Subscription tier
account-last-validated = Last checked { $when }
account-never-validated = Never checked online
account-signed-out = You are not signed in.
account-sign-in = Sign in

## Sign-out confirmation (contracts/ui-surface.md "Settings > Account")

signout-confirm-title = Sign out?
signout-confirm-intro = This deletes the following from this device:
signout-category-credential = Your sign-in credential
signout-category-account-details = Account details
signout-confirm = Sign out
signout-cancel = Cancel

## Settings > About (contracts/ui-surface.md "Settings > About")

# Deliberately service-neutral, unlike `welcome-description` (contracts/
# ui-surface.md's trademark rule guards this exact key — FR-006): the
# fuller, service-naming description lives in the read-only disclosure
# view one tap away (`about-disclosure`), which reuses `welcome-
# description` itself and is not trademark-guarded (body text may say
# "Spotify" descriptively).
about-product = ModPlayer is a desktop application that connects to your streaming account so you can control playback on this device.
about-version = Version { $version }
about-privacy = Privacy notice
about-disclosure = Disclosure

## Notifications (contracts/account-session.md "Events", contracts/ui-surface.md "Notifications")

signin-again = Your sign-in needs attention. Please sign in again.
session-expired = Your sign-in has expired. Sign in again to resume playback.
session-revoked = ModPlayer's access to your account was revoked. Sign in again to continue.
store-unreadable = ModPlayer couldn't read your saved sign-in from your { $store }.
signed-out = Signed out. Deleted: { $categories }
signout-incomplete = Couldn't fully clear: { $category }

# In-app updates

## Intent

Once someone has installed DesktopAssistant from the `Setup.exe`, a new release published on
GitHub should reach them from inside the app, with no manual download and reinstall. Updates
must be impossible to fake, and the app must not contact anything unless the user lets it.

Builds on the installer (`2026-11-22-installer-design.md`): the per-user NSIS setup, the
`tauri.bundle.conf.json` installer settings, and the tag-driven `release.yml` draft releases.

## Goals

- Tray item **Check for updates**. It checks GitHub now. If there's a newer published release, it asks "DesktopAssistant X.Y.Z is available (you have A.B.C). Install it now?" On **Yes**, it downloads, closes the app, upgrades in place and restarts.
- Tray tick **Check automatically**, **off by default** (the user's choice). When on, the app checks at startup if the last check was over 24 hours ago, and once every 24 hours while running. It asks the same question before installing.
- Only updates signed with the project's update key are installed.

## Non-goals

- Silent or forced updates. It always asks.
- Release channels (beta or nightly), delta updates, or rolling back to an older version.
- Updating copies built from source. These have no `latest.json` match to work with, so the menu says so instead of failing.

## Anti-goals

- Contacting GitHub while **Check automatically** is off and the user hasn't clicked **Check for updates**.
- Installing anything not signed with the project key, or any draft release.
- An update that breaks Connect. It must upgrade in the same folder, so the hook path stays valid.

## Approaches considered

- **A. Tauri's official updater plugin (chosen by the user).** It handles signature checks, the download and running the NSIS setup in its quick mode. Signing uses a free key pair, not a paid certificate.
- **B. Notify only.** Check GitHub's latest release and open the download page. Simpler, but the user still downloads and runs the setup by hand.

## Design

1. **Plugin.** Add `tauri-plugin-updater` (v2). In `tauri.conf.json`, `plugins.updater` gets the public key and one endpoint, `https://github.com/jbstinson/DesktopAssistant/releases/latest/download/latest.json`. GitHub's `latest` URL resolves only to the newest **published**, non-prerelease release, so drafts are never offered. Windows install mode is `passive`: a small progress window and no questions, the right fit for an upgrade the user already said yes to.
2. **Signed release artifacts.** In `tauri.bundle.conf.json`, set `createUpdaterArtifacts: true`, so the installer build also writes `<setup>.exe.sig`. `release.yml` passes the repository secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` to the build, then writes `latest.json` (version, notes, pub_date, and `platforms.windows-x86_64` with the signature text and the release asset URL of the setup). It uploads `latest.json` and the `.sig` to the draft release next to the setup. If the secrets are missing, the release job fails with a clear message rather than publishing an unsigned update.
3. **Signing key (a one-time step for the owner).** The owner runs `npx tauri signer generate -w %USERPROFILE%\.tauri\desktopassistant.key` and chooses a password. They give the **public** key to be committed in `tauri.conf.json`, and add the private key and password as the two GitHub repository secrets themselves. The README warns them to back up the private key: if it's lost, existing installs can't verify any future update and would need a manual reinstall.
4. **Tray.** Two new items, under the existing items and above **Open rules file**:
   - **Check for updates**: checks now, and says "You have the latest version (X.Y.Z)" or names the error if the check fails.
   - **Check automatically**: a tick, off by default.

   Questions and results use the existing native message-box helpers (`ask`, `show_error`, plus an info variant). Only one check can run at a time.
5. **Saved choice.** `updates.json` in the data folder holds `{ "auto": bool, "last_check": <unix seconds or null> }`. It's read at startup and written on change and after each check. A missing or broken file means auto is off and the app never checks.
6. **Installing.** On **Yes**, the app hands any held permission requests back to Claude Code, exactly as quitting does. It removes `endpoint.json`, then lets the plugin download, verify and run the setup, which closes the app and restarts it after the upgrade. If the download or signature check fails, the app keeps running and shows the error.
7. **README.** Explain updates and the **Check automatically** option, and change "Nothing leaves your machine" to say the only network request is the optional update check to GitHub, which sends no session data. In "Making a release", add the one-time signing key setup and the backup warning.

## Left to the builder

- The exact plugin API for the version that resolves (check, download and install, then restart), and running the check off the main thread.
- Pure, unit-tested pieces:
  - the "is a check due" rule (auto on, and `last_check` missing or ≥ 24 h ago)
  - reading and writing `updates.json`, including a missing or broken file
  - building `latest.json` from the version, the signature and the asset URL
- Local verification: build two versions with a throwaway key pair (never the real one), serve `latest.json` and the setup from a local server, point a test build's endpoint at it, and confirm the old version upgrades to the new one in a scratch install folder. Also confirm that a tampered `.sig` is rejected.

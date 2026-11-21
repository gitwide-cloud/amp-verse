# AMP Verse Beta 1

This is the Beta release channel. The desktop numeric base version remains
1.0.22; Android is 0.3.2-beta.1, version code 37. Server builds report Beta 1.

Use `install-tested-desktop.ps1` and the verified matching build manifest to
update the single existing AMP Verse installation in place. Follow
`tested-desktop-install.md`; do not create a new install folder for each build.
The existing launcher, music, playlists, settings, signing identities and
internal `V1Preview` profile paths stay compatible. `restore-tested-desktop.ps1`
provides rollback from the installer receipt. The archive omits the old separate
Preview installer; legacy scripts remain in the source repository only.

Unsigned Android APKs require local signing with the existing private key before
installation. Do not uninstall or clear app data to bypass a signer mismatch.
Server artifacts are staged for a controlled update; release branding alone
does not justify restarting a server that is playing music.

Audio Preview Mix, clip previews and podcast previews are playback features and
retain their names. Beta artifacts are manually reviewed updates; stable
automatic-update trust rules remain in effect.

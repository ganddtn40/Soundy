# Privacy

Soundy is a local desktop application. It does not run a Soundy-operated account service, does
not include analytics, crash reporting, advertising or an installation counter, and does not
send telemetry to the project.

This build has no server of its own. The only network requests it makes are the ones a feature
you use needs, and they go to the service that feature belongs to.

## What is stored on your device

Soundy keeps its data in the platform's usual application directories under `soundy`:

- Windows: `%APPDATA%\soundy` and `%LOCALAPPDATA%\soundy`
- macOS: `~/Library/Application Support/soundy` and `~/Library/Caches/soundy`
- Linux: `$XDG_CONFIG_HOME/soundy` and `$XDG_CACHE_HOME/soundy`

That data includes settings, playback state, provider credentials, local-library information,
cover and image caches and the played-track cache. Diagnostic logs are written locally under the
same folders. They are not uploaded anywhere by Soundy.

Played tracks kept on disk are encrypted with a key held in the operating system's keychain, or
in a key file only the user can read when no keychain is available. Spotify's own files stay
encrypted under Spotify's key.

## Network requests

When you use an online feature, Soundy talks to the service behind it:

- **Music providers** - Spotify, Apple Music, YouTube Music, Deezer, Tidal, Subsonic, Jellyfin
  and the other providers the app offers receive the requests needed to search, browse, play
  and, when you ask, download for offline use. Their own privacy policies apply.
- **Lyrics and artwork** - lyric catalogues and public cover-art catalogues receive the artist,
  title and album needed to look a track up, when those features are enabled.
- **Sessions** - if you start or join a listening session, the session server receives
  the session code and the playback information the feature needs to synchronize. The default
  community server is operated by the upstream Sonora project, not by Soundy.
- **Discord** - rich presence is off by default. When enabled and Discord is running, Soundy
  sends the track, artist, album, a cover link and the start time so Discord can show them on
  your profile. The settings can hide the details or turn the presence off.
- **Scrobbling** - off until an account is linked. A linked Last.fm, Libre.fm, ListenBrainz or
  Maloja server receives the tracks you play and when you played them.
- **Links** - opening a provider short link makes Soundy fetch it from its host to find the
  destination, the same way a browser would.

Spotify Connect is enabled by default for a signed-in Spotify account. Soundy then appears as a
device in Spotify's apps and sends Spotify a device name and a device identifier. The default
name includes the computer's hostname, as in `Soundy (my-laptop)`; it can be changed or Spotify
Connect can be turned off in Settings.

Provider credentials are stored locally. Soundy does not send them to the project, because there
is no project server to send them to.

## Third-party services

The services named above are operated by other companies and projects. Their privacy policies,
not this document, govern what they collect. Soundy does not control them and cannot delete data
held by them; use the service's own account controls for that.

## Changes

Material changes to this policy will be published in this repository. Questions about the code
can be raised at <https://github.com/ganddtn40/soundy/issues>.

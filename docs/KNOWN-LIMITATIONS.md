# Known limitations

These are the limits of the current build, including the ones that cannot be fixed without
changing what a service permits. Nothing here is hidden behind a success message in the app.

## Spotify

### A public link publishes at most 100 rows

Spotify's public playlist page, the widget at `open.spotify.com/embed/playlist/<id>`, carries the
playlist name, owner, cover and up to 100 rows. It names no reliable total and carries no album
names, ISRCs, per-track artwork or description. A public link therefore imports anonymously, but
a playlist longer than 100 tracks may be cut short; the result says so under **Public page
limit**, and the complete track list or an exported file is the way to import the rest.

A private, collaborative, deleted or restricted playlist may publish no track list at all.
Soundy reports that and never bypasses it. When a Spotify playback session is already signed in,
Soundy first asks the official Web API, which can read private or collaborative playlists the
account is allowed to see; it does not scrape an authenticated session.

### The live Spotify paths are not in automated tests

The public-page reader is tested against stored payloads and was exercised against real public
playlists. The Web API read, the browser consent and the token refresh still need a real Spotify
account, so they are not exercised in this repository's automated tests. The file and pasted-list
paths remain fully usable without an account.

### Statuses are a snapshot

An imported row's status is decided at import time. If the Spotify session later ends, or the
active provider changes, a row labelled **Full playback** may no longer play. Soundy shows the
playback error then; it does not silently re-import or revalidate every row on every play.

## Catalogues and matching

### Jamendo is optional

Jamendo needs a developer `client_id`. Without `SOUNDY_JAMENDO_CLIENT_ID`, it is skipped
entirely; Audius remains available without an account. A configured but invalid key is reported
as an unavailable catalogue in the log, and the row stays unlinked.

### A title-only row cannot be matched

To avoid substituting an unrelated recording, a provider or catalogue match requires a credited
artist that agrees with the source and a length within five seconds (or an ISRC match). A CSV or
pasted row with no artist is kept, but it is normally **Unavailable** unless the listener's own
library already has a matching file.

### Remixes, live takes and covers stay unmatched

That is intentional. A different version is a different recording, even when its title is close.
The row keeps its own metadata and an **Unavailable** label rather than playing the wrong song.

### Local path equality is literal

An M3U path is matched against library track paths as written. Windows paths with different
letter case, a changed drive letter or a moved folder fall back to title/artist/album/length
matching. For unusual tags, an external catalogue link or an **Unavailable** row is the result.

### A matched local row displays the library file's tags

The imported row keeps the source title, artist, album and duration in the database, and the
import report counts the source rows. In the playlist detail, a row matched to one of your own
files is drawn with that file's tags, the same as any other local track. A filename-derived title
such as `test-tone` can therefore appear where the source list said something else. This is a
display choice, not a lost row; the source metadata remains stored.

### Public catalogue links are not playback

**External playback** means a permitted public page describes the same recording. Soundy opens
that page in the browser; it does not stream it inside the player.

### M3U stream URLs are links, not radio

An `http://` or `https://` entry in an M3U is kept as an external row with its URL. Soundy does
not implement an arbitrary internet-radio player, so the URL opens externally.

## Import behaviour

- A list file is capped at 8 MiB.
- Imported playlists are read-only. Renaming is possible through the local playlist actions, but
  rows cannot be edited, added or reordered in place.
- Re-importing the same file or pasted list replaces that playlist's rows after the new list is
  saved successfully. Re-importing a different file with the same name creates a different
  playlist.
- Duplicate rows are preserved by position and counted in the result; they are not collapsed.
- The playlist description is stored and shown in the import result, but the playlist detail
  screen does not currently display it. The playlist name, owner, cover, row metadata, source
  link and import date are kept.
- Provider and catalogue searches are issued for rows that still need them, four at a time. An
  import of a very large list therefore still needs a network round trip per distinct query; the
  bounded cache only avoids repeats.
- Cancelling is checked between requests and before the database write. A request already in
  flight is allowed to finish or time out first.

## Features Monochrome has that Soundy does not

The full list is in [MONOCHROME-PARITY.md](./MONOCHROME-PARITY.md). The user-visible ones worth
stating here:

- No Info or Related tab in the right panel; it carries Lyrics, Queue and, in a Session,
  Members.
- No command palette, no local crossfade setting, no playback speed, no sleep timer, no mono /
  crossfeed / stereo-width controls and no binaural or spatial DSP. Spotify's own crossfade
  preference is the only crossfade path.
- No waveform seekbar and no per-row quality badge.
- Downloads stay in Soundy's encrypted offline cache. There is no "export to my music folder"
  pipeline, filename template or CUE/NFO/M3U sidecar writer.
- No Monochrome account, cloud sync, community playlist service, listening-party protocol,
  AOTY pages or unreleased tracker. None of those services exist for this build to call.
- No podcast catalogue. The wired providers do not expose one, so the sidebar has no Podcasts row
  rather than a row that opens an empty page.

## Artwork and catalogue limits

- Deezer's public API can throttle or refuse a client. When it does, it is disabled for the rest of
  that import and iTunes is used; if both refuse, rows keep their placeholder and the retry button
  remains available for rows that can display art.
- Artwork found by title/artist/duration is the provider's release art, not necessarily the exact
  pressing named in the export. The match rules prevent a different recording, not a different
  edition of the same recording.
- Album grouping means one cover can be reused for several rows. That is intentional for a
  playlist collage; it is not a claim that every row came from that album.
- A generated collage is rebuilt when a retry finds more art, unless the listener chose a cover.
  There is no separate "regenerate collage" button.

## Application and packaging

### Output device switching is not in the fullscreen player

The fullscreen player carries the transport, seek, volume, shuffle/repeat, lyrics and queue. The
output device list lives in **Settings → Sound**; it is not duplicated in the fullscreen corner,
which has room for four icon buttons at 936×648. Picking a device is one settings visit away and
does not stop playback. This is a deliberate layout choice, not a hidden control.

### Idle working set misses the 100 MB target

The shipped binary settles at **104.0 MB working set with an 87.8 MB private commit** on a fresh
profile at 936×648, measured over 60 seconds with no account, local folders or imports. With the
maintainer's populated profile (signed-in YouTube Music, two imported playlists covering 276 rows)
it settles at **119.9 MB / 100.9 MB**, and streaming measures 124.5 MB / 107.6 MB. The 100 MB
target is met by private commit on a fresh profile only; resident memory is 4 MB over there and
higher once a library and provider session are live. The gap is the GPUI/`wgpu` window, font and
image atlases plus the in-memory library data, not a leak: both profiles were flat across the
sample. See `PERFORMANCE.md` for the method and raw numbers.

### New labels fall back to English outside en-US

The unified build adds 23 interface strings: the Discover, Favorites and Downloads destinations,
the Home empty state, the Discover page, and the importer's enrichment, artwork and retry labels.
Only `en-US` carries translations for them in this release. The i18n layer falls back to English
per key, so no screen shows a missing-key marker or a blank label, but those labels stay English
in the other 17 locales. Translating them is a strings-only change for a later patch.

### The binary is unsigned

There is no code-signing certificate for this release. Windows SmartScreen may warn on first run.

### Only Windows is packaged here

The workspace keeps the Linux and macOS build paths, but Soundy ships only a Windows x64
installer and portable ZIP. Flatpak and Nix files are source recipes, not published artefacts.

### UI automation is not part of the test suite

The automated tests cover the parsers, matching rules, report counts, storage migration, provider
HTTP behaviour and state logic. The rendered dialog is verified by launching the release build
and exercising it by hand, not by a pixel or accessibility-tree test. A future GPUI test harness
could close that gap.

### Spotify playback still follows Spotify's rules

A Spotify row that says **Full playback** is still subject to Spotify's subscription and device
rules at play time. Soundy does not bypass them.

# Known limitations

These are the limits of the current build, including the ones that cannot be fixed without
changing what a service permits. Nothing here is hidden behind a success message in the app.

## Spotify

### A Spotify link cannot be imported without a Spotify session

Spotify does not expose a playlist's tracks to an unauthenticated client. Its public `oEmbed`
answer contains the title and a thumbnail only; the Web API requires OAuth; scraping the web
player is prohibited by Spotify's terms.

When no session exists, Soundy returns **“Spotify does not publish this playlist's tracks
without an authorized Spotify session”** and offers a file or pasted-list fallback. It does not
scrape, fake a successful read or create an empty playlist. This is a deliberate product
decision, not a bug.

### The optional Spotify path needs an existing session

The authorized path is used only when Spotify is signed in and selected as the active provider.
It asks for no separate password, client secret or developer app. The path is not exercised in
this repository's automated tests because no Spotify account is available; its HTTP behaviour is
tested against a local server instead.

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

## Application and packaging

### Idle memory misses the 100 MB target

Measured idle RSS is 111.7 MB on Windows 11 x64, with a 100.0 MB private working set. The CPU,
startup and playback targets are met. See `PERFORMANCE.md` for the method and the raw numbers.
The profile held five imported playlists and a signed-in YouTube Music session, so the figure is
not a fresh-profile minimum. The gap is the GPUI/`wgpu` baseline, not the importer.

### The binary is unsigned

There is no code-signing certificate for this fork. Windows SmartScreen may warn on first run.

### Only Windows is packaged here

The workspace keeps the Linux and macOS build paths, but this fork ships only a Windows x64
installer and portable ZIP. Flatpak and Nix files are source recipes, not published artefacts.

### UI automation is not part of the test suite

The automated tests cover the parsers, matching rules, report counts, storage migration, provider
HTTP behaviour and state logic. The rendered dialog is verified by launching the release build
and exercising it by hand, not by a pixel or accessibility-tree test. A future GPUI test harness
could close that gap.

### Spotify playback still follows Spotify's rules

A Spotify row that says **Full playback** is still subject to Spotify's subscription and device
rules at play time. Soundy does not bypass them.

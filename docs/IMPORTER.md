# Playlist import

Soundy imports a playlist from three sources. A public Spotify playlist, a file and a pasted
list all import without an account, a developer key or an external converter.

| Source | What it accepts | Account needed |
| --- | --- | --- |
| Spotify link | A public playlist URL, `spotify:playlist:` URI or 22-character playlist ID | No |
| File | M3U, M3U8, CSV, TXT | No |
| Pasted list | Text on the clipboard, one track per line | No |

The dialog is opened from **+ -> Import playlist** on the Playlists screen, the playlist context
menu, or the import button in the playlist editor. Opening it makes no request. Reading starts
when **Import** is pressed.

## How a Spotify link is read

A 22-character playlist ID is accepted on its own. `open.spotify.com/embed/playlist/<id>` is the
widget Spotify serves to any browser with no
account. Soundy requests that public page and reads the playlist data it carries:

- The playlist's name, owner and cover.
- Every row the page publishes: title, credited artists, duration, Spotify track id and URL,
  whether Spotify considers the track playable, and whether only a preview is offered.

Soundy does not sign in for a public link, does not read cookies, does not call a private
endpoint and does not copy audio. A playlist Spotify does not publish is reported as unavailable
instead of being guessed at from its title or cover.

### The public page limit

The public widget publishes at most **100 rows** and names no reliable total. It also carries no
album names, ISRCs, per-track artwork or description. When the page comes back with 100 rows,
the import result says so under **Public page limit**: every row Spotify published is kept, but
the playlist may have more. For a longer playlist, paste its complete track list as a pasted
list. Nothing is invented or silently dropped.

A playlist with 91 rows, such as the public playlist used for the 1.0.1 test, imports in full.

### Private, collaborative and deleted playlists

The public page publishes only what Spotify makes public. A private or collaborative playlist
may therefore report no track list; a deleted playlist reports not found. Soundy never bypasses
that, never scrapes an authenticated session and never asks for a password.

When a Spotify playback session is already signed in, Soundy first asks the official Web API,
which can read private or collaborative playlists the account is allowed to see. If the API
cannot read the link, the public page is tried as a fallback; if both fail, the dialog explains
why and offers the file and pasted-list paths. No session is required for a public link.

## Accepted file formats

### M3U / M3U8

- `#EXTINF:<seconds>,<Artist> - <Title>` sets the duration and credit.
- Relative paths are resolved against the folder that holds the list.
- `file://` URLs are accepted, including percent-encoded paths and Windows drive letters.
- Absolute local paths are accepted.
- An `http://` or `https://` entry is kept as an **External playback** row with that URL as its
  source link. It is not streamed by Soundy.
- Rows with no readable title are counted as skipped, not invented.

### CSV

- Reads Spotify, Exportify, Soundiiz and TuneMyMusic exports.
- Comma, semicolon and tab delimiters are detected; a UTF-8 BOM is ignored.
- Recognised columns include `Track Name`/`Title`, `Artist Name(s)`/`Artist`, `Album Name`,
  `Track Duration (ms)`/`Duration`/`Length`/`Time`, `Album Image URL`/cover columns, `ISRC`,
  `Track URI`/`Track ID`, `Track URL`, `Added At` and release year.
- The header may be in any order or case. With no header, the first cells are read as title,
  artist, album and duration.
- Quoted fields, commas inside quotes and line breaks inside quotes are understood.
- Durations may be seconds (`225`), `m:ss`, `h:mm:ss` or milliseconds when the column says so.
- A row without a title is skipped and reported in the result.

### Artwork, metadata and generated covers

After matching, Soundy fills in what the source did not carry:

- **Deezer** is asked first, then **iTunes Search** as a fallback. A hit is accepted only when the
  title, the primary artist and the duration agree (or the album, when the list gave no length).
  Remixes, live takes and covers are not substituted.
- Rows are grouped by album, and covers already known are reused, so a 500-row list does not make
  500 identical requests.
- A blocked, throttled or refused provider is disabled for the rest of that import instead of
  being retried row by row.
- Album art is cached under `%LOCALAPPDATA%\soundy\artwork`. The playlist's own cover is a 2x2
  collage of distinct track art (3, 2 or 1 tile when there are fewer); it is cached under
  `%LOCALAPPDATA%\soundy\playlist-covers` and marked as generated in the state database. A cover
  the listener chose is never overwritten, and a playlist cover the source provides is kept.
- **Retry missing artwork** re-runs enrichment for the rows of the saved playlist. It is shown only
  while a row that can actually display art still lacks it; external and unavailable rows never
  trigger it.
- A row whose length the source never gave shows a dash. It is never drawn as `0:00`.

### Pasted list / TXT

One track per non-empty line. `#` starts a comment. Two forms are understood:

```
Artist - Title
Title | Artist | Album | Duration
```

A line with no separator is read as a title only. A title-only row is preserved, but it cannot
be matched to a provider safely because there is no credited artist to verify; it stays
**Unavailable** unless the listener's library has a file the path names.

## What is preserved

- Playlist name, description, owner and permitted artwork (when the source provides them).
- Every row's title, artist, album, duration, cover, ISRC and source URL.
- Original order and duplicate rows.
- The source link and the import date.
- Rows that cannot be played, with their own status and, when a permitted public catalogue knows
  the recording, a link to that catalogue's page.

A row matched to one of your own files is drawn with that file's tags in the playlist detail, the
same as any other local track. The source title, artist, album and duration stay in the imported
row; the display choice can make a filename-derived title visible. See
[KNOWN-LIMITATIONS.md](./KNOWN-LIMITATIONS.md).

Imported playlists are read-only. Re-importing the same file or pasted list updates the same
local playlist instead of adding a duplicate copy; the old rows are replaced only after the new
list has been written successfully.

## How a row is matched

1. **Your own files first.** An M3U path is matched exactly. Otherwise title, primary artist,
   album and length are compared, then ISRC when the lengths disagree and both sides carry it.
2. **The active provider.** Only a playable result with the same title, primary artist and length
   is accepted.
3. **Permitted public catalogues.** Audius is queried without an account. Jamendo is queried only
   when `SOUNDY_JAMENDO_CLIENT_ID` is set. A catalogue answer labels the row **External playback**
   and supplies a link; it does not make the row playable inside Soundy.
4. **Nothing else.** A remix, live take, cover, sped-up version or different artist is never
   substituted. The row stays **Unavailable**.

An anonymous Spotify row that matches nothing still carries the track's Spotify URL, so it stays
in the playlist as **External playback** and its badge opens the track in Spotify. A row Spotify
marks as preview-only is labelled **Preview only**. If Spotify itself is the active provider and
a playback session is signed in, a playable row can be matched and labelled **Full playback**;
the public import never pretends a Spotify row can stream without that session.

### When several recordings match

A provider search can return more than one playable recording that agrees on title, artist and
length, such as the album version and a deluxe edition. Instead of silently keeping the first,
the import pauses on a review screen:

- Each ambiguous row lists up to six candidates, with the album (or the recording's title when the
  provider reports no album) and its length; the first is preselected. Ignoring the screen
  therefore still writes a playable playlist.
- Choosing another candidate updates the pending playlist immediately. Nothing is written until
  **Add to Soundy** is pressed.
- **Back** discards the review; **Close** discards it too. Nothing is saved by either.
- Duplicate ids from one search are collapsed, so the same recording is never offered twice.
- Review rows are capped at 500; beyond that the first match is kept and the screen says so.

The status labels are:

| Label | Meaning |
| --- | --- |
| Full playback | The active provider can play the matched recording |
| Full playback (local) | One of your own files is the matched recording |
| Preview only | The provider offers a clip |
| External playback | Only a permitted catalogue page or the Spotify track page is known; the badge opens it |
| Unavailable | No verified recording was found |

## Work and limits

- Reading, parsing, matching and the database write run off the UI thread.
- Provider and catalogue lookups run four at a time. Cached search answers are bounded at 2048
  distinct queries per import.
- The importer does not poll while idle. A progress timer runs only while an import is active.
- A list file is capped at 8 MiB. A public Spotify page publishes at most 100 rows.
- Cancelling before the write leaves the library untouched.
- A public Spotify page is requested once, with three attempts for a transient failure, a
  `Retry-After` honoured and a 20-second request timeout. A longer wait is reported instead of
  freezing the dialog.
- No browser, WebView or embedded engine is started for import.

## Privacy

The importer sends only what the selected source needs. A file or pasted list is not uploaded.
A public Spotify link sends the playlist id to Spotify's public page; no account token, cookie or
authenticated session is used for it. Matching sends a search query of title and artist to the
active provider and to Audius (and Jamendo if configured). Artwork enrichment sends the row's
title, artist and album to Deezer and, when Deezer has no confident hit, to iTunes. No audio is
uploaded. No telemetry, analytics or install reporting is added by the importer.

# Playlist import

Soundy imports a playlist from three sources. None of them requires a Spotify account, Spotify
Premium, OAuth, a password, an API key or a paid service.

| Source | What it accepts | Account needed |
| --- | --- | --- |
| Spotify link | A playlist URL or `spotify:playlist:` URI | Only for this source, and only if a Spotify session already exists |
| File | M3U, M3U8, CSV, TXT | No |
| Pasted list | Text on the clipboard, one track per line | No |

The dialog is opened from **+ → Import playlist…** on the Playlists screen, the playlist context
menu, or the import button in the playlist editor. Opening it makes no request. Reading starts
when **Import** is pressed.

## The honest limit of a Spotify link

Spotify does not publish playlist tracklists to unauthenticated clients:

- The public `oEmbed` endpoint returns the playlist title and thumbnail only, not its tracks.
- The Web API's playlist endpoint requires an authorized access token.
- Spotify's terms prohibit scraping the web player or otherwise working around authentication.

Soundy therefore does one of two things:

1. **If a Spotify session is already authorized** (the listener signed in to Spotify in Soundy),
   the link is read through the Spotify Web API using that same session. This is the optional
   path, and it is only used when Spotify is the active provider.
2. **If there is no Spotify session**, the link is parsed locally and the import stops with
   `TracklistUnavailable`. The dialog explains why and offers two buttons: choose an exported
   file, or paste the track list. Nothing is scraped, no request is faked, and no empty playlist
   is created.

The same rule applies to private playlists: without the owner's authorized session Soundy
reports `Forbidden` and does not try to bypass it.

### How to export a Spotify playlist

- Use Spotify's own export or a third-party exporter that produces **CSV** with `Title`, `Artist`,
  `Album` and `Duration` columns.
- Or export **M3U/M3U8** from a player that has the same tracks.
- Or copy the track list and use **Pasted list**. `Artist - Title` per line is enough.

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

- Columns: `Title`, `Artist`, `Album`, `Duration`.
- The header may be in any order or case. `Track title`, `Artists`, `Length` and `Time` are also
  recognised.
- With no header, the first four cells are read in that order.
- Quoted fields, commas inside quotes and line breaks inside quotes are understood.
- `Duration` may be seconds (`225`) or `m:ss` / `h:mm:ss`.
- A row without a title is skipped and reported in the result.

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

The status labels are:

| Label | Meaning |
| --- | --- |
| Full playback | The active provider can play the matched recording |
| Full playback (local) | One of your own files is the matched recording |
| Preview only | The provider offers a clip |
| External playback | Only a permitted catalogue page is known; the badge opens it |
| Unavailable | No verified recording was found |

## Work and limits

- Reading, parsing, matching and the database write run off the UI thread.
- Provider and catalogue lookups run four at a time. Cached search answers are bounded at 2048
  distinct queries per import.
- The importer does not poll while idle. A progress timer runs only while an import is active.
- A list file is capped at 8 MiB.
- Cancelling before the write leaves the library untouched.
- Spotify's rows can use the optional Web API path only when Spotify is already signed in; the
  path never asks for a password or client secret and never reads audio.

## Privacy

The importer sends only what the selected source needs. A file or pasted list is not uploaded.
A Spotify link is sent to Spotify only through an existing session. Matching sends a search
query of title and artist to the active provider and to Audius (and Jamendo if configured).
No telemetry, analytics or install reporting is added by the importer.

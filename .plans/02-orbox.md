# Plan 02: Online Radio Box API Client

## Goal

An HTTP client that resolves `orbox:<country>/<alias>` station URIs into playable
stream URLs and fetches now-playing metadata. Also handles metadata parsing with
the artist/name fallback logic.

## Endpoints

All from the Online Radio Box API (documented in `API.md`):

### Stream URL Resolution

```
GET https://onlineradiobox.com/json/{country}/{alias}/widget/
→ { streamURL: string, streamType: int, isGeoBlocked: bool, isRestricted: bool }
```

Called every time a station is activated (no caching, per user requirement).

### Now Playing

```
GET http://scraper.onlineradiobox.com/{country}.{alias}
→ { title, iArtist, iName, iImg, ... }
```

Polled on a timer (configurable, default 30s). Called with query parameters when
possible and appropriate.

## Station URI Resolver (`orbox.rs`)

Given a `StationUri::Orbox { country, alias }`, resolve to a stream URL:

1. GET the widget endpoint
2. Parse response
3. Return `streamURL` (the direct stream URL to hand to GStreamer)
4. Handle errors: API unavailable, station not found, geo-blocked — log and
   propagate as `anyhow::Error` so the caller can present an error to the user

Use `reqwest` for HTTP (it's already in the tokio ecosystem and handles
redirects, TLS, etc).

## Metadata Poller (`metadata.rs`)

Given an active `StationUri`, periodically fetch now-playing data:

1. GET the scraper endpoint
2. Parse response JSON
3. Extract metadata fields
4. Emit metadata as a struct/enum to be consumed by MPRIS and Last.fm handlers

### Metadata Parsing Logic

The scraper response may contain `iArtist` and `iName` fields. Parsing priority:

1. **Primary**: If both `iArtist` and `iName` are non-empty strings, use them
   directly as artist and title.
2. **Fallback**: If either is missing or empty, parse the `title` field instead:
   - Split on the first `" - "` (space-dash-space) separator
   - Left side = artist, right side = title
   - If no `" - "` found, treat the entire string as title with empty artist

This covers cases where the scraper returns a raw title string like
`"Aitch - Rain (feat. Tay Keith)"` but doesn't populate the structured fields.

### Metadata Output Type

Define a struct that downstream consumers (MPRIS, Last.fm) use:

- `track_id`: stable identifier derived from station alias + timestamp
- `artist`: String (may be empty if unresolvable)
- `title`: String
- `album`: Option<String> (not provided by ORBox, always None for now)
- `art_url`: String from `iImg` field
- `duration_secs`: Option<u64> (not provided by ORBox, leave None)

## Integration Point

The daemon main loop spawns a metadata poll task when a station is active and
cancels it on station change or stop. The task sends updates on a
`tokio::sync::watch` channel (single-producer, single-consumer, always keeps
latest value) so MPRIS and Last.fm subscribers can independently observe
metadata changes.

## Error Handling

- Transient HTTP errors: log at warn level, retry on next poll interval
- Persistent errors (e.g. station removed from API): log at error level, emit
  empty metadata so MPRIS clears the now-playing display
- Geo-blocked stations: treat as a permanent error for that station

## Verification

- Given a valid `orbox:uk/bbcradio1` URI, the resolver returns a non-empty
  stream URL, and the metadata poller returns parsed artist/title
- Given a station that returns only `title` (no `iArtist`/`iName`), the fallback
  parser correctly splits on `" - "`
- Given an invalid station alias, the resolver returns an error (not a panic)
- Network errors are handled gracefully with retry on next poll

## Dependencies on Other Plans

- **01-core**: Uses `StationUri` type
- **03-mpris**: Consumes the metadata output type
- **05-lastfm**: Consumes the metadata output type

# Plan 05: Last.fm Scrobbling

## Goal

Implement Last.fm "Now Playing" and scrobbling for tracks played through the
daemon, with persistent authentication.

## Authentication Flow

Use Last.fm's **Mobile Auth** method (not Desktop Auth — it's simpler and
doesn't require a browser callback loop):

1. On first run, collect `api_key`, `api_secret`, `username`, and `password`
   from the config's `[lastfm]` section.
2. Call `auth.getMobileSession` with these parameters. The password must be
   passed as an MD5 hash (Last.fm's mobile auth convention).
3. The response includes a session key (`sk`). Persist this key to
   `$XDG_DATA_HOME/radio-devil/lastfm_session.toml`.
4. On subsequent runs, load the session key from the data file instead of
   re-authenticating. If the saved key is invalid (API returns auth error),
   re-authenticate using the stored credentials.

### API Signature

All authenticated Last.fm API calls require an `api_sig` parameter:

1. Collect all parameters (including `method`, `api_key`, `sk`, etc.) into a
   sorted list by key name
2. Concatenate as `key1value1key2value2...`
3. Append the `api_secret`
4. Compute MD5 of the resulting string
5. Include `api_sig` in the request

This signature logic should be a shared utility used by both `updateNowPlaying`
and `scrobble`.

## Now Playing (`track.updateNowPlaying`)

Sent whenever track metadata changes (via the metadata watch channel from plan
02).

### Parameters

| Param | Value |
|---|---|
| `artist` | `metadata.artist` |
| `track` | `metadata.title` |
| `album` | `metadata.album` (optional) |
| `duration` | `metadata.duration_secs` (optional) |
| `api_key` | From config |
| `sk` | From session |
| `api_sig` | Computed signature |

### Timing

Send this as soon as the metadata changes. If the same track repeats (rare for
radio but possible), still send a new now-playing update — the timestamp in the
request is implicit.

### Edge Cases

- Empty artist or title: skip the update entirely (don't send "Unknown - Unknown")
- HTTP failure: log at warn level, retry on next metadata change (don't queue)

## Scrobbling (`track.scrobble`)

A track should be scrobbled to the user's history after it has been "played" for
a sufficient duration. Since we don't know the actual track duration from the
radio metadata, use a simpler heuristic:

**Scrobble the previous track when the metadata changes to a new track, provided
it was the active track for at least 30 seconds.**

### State Machine

The scrobbler maintains state about the current track:

```
[Nothing playing]
     │
     ▼ metadata arrives
[Tracking: artist=A, title=T, start_time=S]
     │
     ├─ metadata changes (same track): ignore
     │
     └─ metadata changes (new track):
           if (now - start_time >= 30s): SCROBBLE artist=A, title=T
           Transition to [Tracking: new track]
```

### Scrobble Parameters

| Param | Value |
|---|---|
| `artist[0]` | Previous track's artist |
| `track[0]` | Previous track's title |
| `timestamp[0]` | Unix timestamp of when the previous track _started_ (not ended — Last.fm convention for timestamped scrobbles is the start time) |
| `api_key` | From config |
| `sk` | From session |
| `api_sig` | Computed signature |

### Edge Cases

- If the daemon stops, scrobble the currently-playing track (unless it's been
  less than 30s).
- If metadata polling fails and we get no updates for an extended period, don't
  scrobble stale data.
- If the same track appears again immediately (station replay), treat it as a
  new tracking session.

## Module Structure (`lastfm.rs`)

Three public functions exposed to the rest of the daemon:

- `authenticate(config) → session` — handles first-auth + session caching
- `update_now_playing(session, metadata)` — sends now-playing update
- `scrobble(session, metadata)` — submits a scrobble

The scrobbling state machine lives in the daemon's main event loop or a spawned
task, consuming the metadata watch channel and calling these functions.

## Session Persistence

```toml
# $XDG_DATA_HOME/radio-devil/lastfm_session.toml
sk = "abc123..."
username = "example_user"
```

Loaded on startup. If missing, run the auth flow. If auth fails (invalid key),
retry auth with stored credentials (if password is in config) or log a warning
and disable scrobbling.

## Verification

- First run with `[lastfm]` credentials: session file is created, now-playing
  appears on the user's Last.fm profile
- Subsequent runs: no re-auth, session file is reused
- Track change after 30+ seconds: scrobble appears in listening history
- Track change under 30s: no scrobble sent
- Missing credentials in config: daemon starts without errors, scrobbling
  silently disabled (log at debug level)
- Network failure during scrobble: logged, daemon continues, no retry needed

## Dependencies on Other Plans

- **01-core**: Config `[lastfm]` section, data directory path
- **02-orbox**: Metadata output type (consumed from watch channel)

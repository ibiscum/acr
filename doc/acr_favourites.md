# acr_favourites

Manage favourite songs via the AudioControl HTTP API.

The Rust binary name is `audiocontrol_favourites`.
Some docs may refer to it as `acr_favourites`.

## Synopsis

```bash
audiocontrol_favourites [OPTIONS] <COMMAND>
```

## Global Options

- `--url URL`: AudioControl API base URL (default: `http://localhost:1080`)
- `-v, --verbose`: show request/response details
- `-q, --quiet`: suppress normal output (errors still shown)

## Commands

### check

```bash
audiocontrol_favourites check --artist <ARTIST> --title <TITLE>
```

Checks whether a track is currently marked as favourite.

Short flags:

- `-a, --artist`
- `-t, --title`

### add

```bash
audiocontrol_favourites add --artist <ARTIST> --title <TITLE>
```

Adds a track to favourites.

Short flags:

- `-a, --artist`
- `-t, --title`

### remove

```bash
audiocontrol_favourites remove --artist <ARTIST> --title <TITLE>
```

Removes a track from favourites.

Short flags:

- `-a, --artist`
- `-t, --title`

### providers

```bash
audiocontrol_favourites providers
```

Lists favourite providers and their enabled/disabled status.

## API Response Handling

The tool accepts both response styles:

- wrapped success: `{ "Ok": { ... } }`
- direct object: `{ ... }`

And it handles wrapped errors:

- `{ "Err": { "error": "..." } }` -> reported as API error
- `{ "Err": { ... } }` -> reported as generic API error

If response JSON is not an object shape the tool expects, the command fails.

## Output

Typical status output uses checkmarks/crosses:

```text
✓ 'Hey Jude' by 'The Beatles' is marked as favourite
✗ 'Song' by 'Artist' is not marked as favourite
```

Provider listing example:

```text
Favourite Providers: 2 enabled out of 2 total

	Last.fm (lastfm): ✓ Enabled
	User settings (settingsdb): ✓ Enabled
```

With `--verbose`, additional request URLs, raw responses, and per-provider counts are printed.

## Examples

Check a track:

```bash
audiocontrol_favourites check --artist "The Beatles" --title "Hey Jude"
```

Add a track:

```bash
audiocontrol_favourites add -a "Pink Floyd" -t "Wish You Were Here"
```

Remove a track:

```bash
audiocontrol_favourites remove --artist "Queen" --title "Bohemian Rhapsody"
```

List providers using a remote instance:

```bash
audiocontrol_favourites --url http://192.168.1.100:1080 providers
```

Verbose debugging:

```bash
audiocontrol_favourites -v add --artist "Artist" --title "Song"
```

## Exit Behavior

Returns non-zero on failure, including:

- network/HTTP errors
- invalid JSON/response format
- API-level error responses

## Related

- `debian/man/audiocontrol_favourites.1`
- `src/tools/acr_favourites.rs`

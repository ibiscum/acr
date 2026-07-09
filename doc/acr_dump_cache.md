# acr_dump_cache

Cache inspection and maintenance tool for AudioControl attribute cache.

The Rust binary name is `audiocontrol_dump_cache`.
Some packaging/docs also refer to it as `acr_dump_cache`.

## Synopsis

```bash
audiocontrol_dump_cache [--cache-dir DIR] <COMMAND> [OPTIONS]
```

Commands:

- `list`: list cache keys or detailed entries
- `clean`: delete cache entries using filters
- `stats`: show cache size/count summary

Global option:

- `-c, --cache-dir DIR`: use a custom cache directory (expects `attributes.db` there)

## List Command

```bash
audiocontrol_dump_cache list [OPTIONS]
```

Options:

- `-p, --prefix PREFIX`: filter keys by prefix
- `-d, --detailed`: show key, size, created, updated columns
- `-l, --limit N`: limit returned rows
- `--artistmbid`: shortcut for artist MusicBrainz prefix
- `--imagemeta`: shortcut for image metadata prefix
- `--artistsplit`: shortcut for artist split prefix
- `--artistnotfound`: shortcut for MusicBrainz negative cache prefix

Notes:

- You can use either `--prefix` or one shortcut flag, not both.
- Only one shortcut flag may be passed at a time.

## Clean Command

```bash
audiocontrol_dump_cache clean [OPTIONS]
```

Options:

- `-p, --prefix PREFIX`: delete keys matching prefix
- `--all`: delete all cache entries
- `--older-than-days DAYS`: delete entries with `created_at` older than `DAYS`
- `--dry-run`: print what would be deleted, do not delete
- `--artistmbid`: shortcut for artist MusicBrainz prefix
- `--imagemeta`: shortcut for image metadata prefix
- `--artistsplit`: shortcut for artist split prefix
- `--artistnotfound`: shortcut for MusicBrainz negative cache prefix

Validation rules:

- Must provide one of: `--all`, `--prefix`, `--older-than-days`, or a shortcut.
- `--all` and `--prefix` cannot be combined.
- Shortcut conflict rules are the same as `list`.

Dry-run behavior:

- With `--all`: prints total entries that would be removed.
- With `--prefix`/shortcut: prints count and up to 10 sample keys.
- With `--older-than-days`: evaluates age threshold and prints count and up to 10 sample keys.

## Stats Command

```bash
audiocontrol_dump_cache stats [--by-prefix]
```

Options:

- `-b, --by-prefix`: include grouped stats table by extracted prefix

Output includes:

- total entries
- total size (human readable)
- oldest and newest entry timestamps

## Prefix Shortcuts

The tool resolves shortcuts to internal cache prefixes:

- `--artistmbid` -> artist MusicBrainz cache prefix
- `--imagemeta` -> image metadata cache prefix
- `--artistsplit` -> artist splitter cache prefix
- `--artistnotfound` -> MusicBrainz not-found cache prefix

## Examples

List first 20 keys:

```bash
audiocontrol_dump_cache list --limit 20
```

List image metadata entries with timestamps:

```bash
audiocontrol_dump_cache list --imagemeta --detailed
```

Dry-run cleanup for entries older than 30 days:

```bash
audiocontrol_dump_cache clean --older-than-days 30 --dry-run
```

Delete only MusicBrainz not-found cache entries:

```bash
audiocontrol_dump_cache clean --artistnotfound
```

Show cache statistics grouped by prefix:

```bash
audiocontrol_dump_cache stats --by-prefix
```

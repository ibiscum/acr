# acr_dump_store

Inspect entries in the AudioControl security store JSON file.

The Rust binary name is `audiocontrol_dump_store`.
Some docs and packaging refer to it as `acr_dump_store`.

## Synopsis

```bash
audiocontrol_dump_store [--store-path PATH] [--key KEY]
```

## Options

- `-s, --store-path PATH`: path to `security_store.json`
- `-k, --key KEY`: encryption key to decrypt values

If `--store-path` is not provided, the tool uses:

```text
secrets/security_store.json
```

## Behavior

### Without `--key`

The tool reads the JSON file directly and prints key names with masked values:

```text
my_key: ***
another_key: ***
```

This mode requires the JSON to contain an object at `values`.

### With `--key`

The tool attempts to initialize `SecurityStore` and decrypt values.

- On success: prints decrypted key/value pairs
- If initialization fails: prints an error and falls back to raw masked mode

## Exit and Error Semantics

The tool returns a non-zero exit when:

- the store file does not exist
- the JSON cannot be parsed
- raw mode finds missing/invalid `values` map
- decrypt mode cannot enumerate keys after initialization

In decrypt mode, individual key decryption failures are logged and skipped; the command continues for remaining keys.

## Expected Store Shape

Minimum accepted shape for raw mode:

```json
{
	"values": {
		"api_key": "...encrypted...",
		"token": "...encrypted..."
	}
}
```

An empty map is valid:

```json
{
	"values": {}
}
```

If `values` is missing or not an object, the command fails with an error.

## Examples

Dump keys with masked values from default location:

```bash
audiocontrol_dump_store
```

Dump keys with masked values from a custom file:

```bash
audiocontrol_dump_store --store-path /var/lib/audiocontrol/security_store.json
```

Attempt full decryption with a key:

```bash
audiocontrol_dump_store --key "$SECRETS_ENCRYPTION_KEY"
```

Use custom path and decryption key:

```bash
audiocontrol_dump_store --store-path ./secrets/security_store.json --key "$SECRETS_ENCRYPTION_KEY"
```

## Related

- `secrets.txt.sample` for encryption key setup
- `src/helpers/security_store.rs` for storage/encryption implementation

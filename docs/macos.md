# Running the prebuilt binary on macOS

The release binaries aren't signed with an Apple Developer ID or notarized. macOS blocks
an unsigned program only when its file carries the *quarantine* flag, which browsers and
some download tools add. What you need to do depends on how you downloaded it.

## Downloaded with `curl`: nothing to do

`curl` doesn't add the quarantine flag, so the binary runs as is:

```sh
v=0.1.0                                  # the release you want
t=aarch64-apple-darwin                   # Apple Silicon; x86_64-apple-darwin for Intel
base=https://github.com/UbadahJ/bmk/releases/download/v$v
curl -fLO "$base/bmk-$v-$t.tar.gz"
curl -fLO "$base/SHA256SUMS"
shasum -a 256 -c SHA256SUMS --ignore-missing
tar -xzf "bmk-$v-$t.tar.gz"
"./bmk-$v-$t/install.sh"
```

## Downloaded with a browser: remove the quarantine flag

If you saved the archive from the GitHub releases page, macOS says *"bmk" can't be opened
because Apple cannot check it for malicious software* (or *Apple could not verify "bmk"*)
the first time it runs. Use one of these:

**Terminal (recommended).** Remove the flag from the archive before extracting it, or
from the extracted folder:

```sh
xattr -d com.apple.quarantine bmk-0.1.0-aarch64-apple-darwin.tar.gz
# or, after extracting:
xattr -dr com.apple.quarantine bmk-0.1.0-aarch64-apple-darwin
```

**System Settings.** Run `bmk` once so macOS blocks it, then open **System Settings >
Privacy & Security**, scroll to **Security**, and click **Open Anyway** next to the message
about `bmk`. Confirm with your password. You only need to do this once per binary.
(Control-clicking and choosing **Open** no longer bypasses the check on macOS 15 and later,
and doesn't apply to command-line tools anyway.)

## Check that it worked

```sh
xattr bmk          # should not list com.apple.quarantine
bmk -V             # bmk 0.1.0 (generic)
```

## Why not sign it?

Signing and notarizing need a paid Apple Developer account. Apple Silicon binaries are
still *ad-hoc signed* by the linker, which macOS requires to run them at all; that
signature just doesn't identify a developer. If you'd rather not trust a prebuilt binary,
build from source with `./install.sh`. You also get a native build that way.

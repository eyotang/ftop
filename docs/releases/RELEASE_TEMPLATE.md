# ftop VERSION

GPUI Kit native system monitor for macOS, Windows, and Ubuntu.

## Downloads and updates

Verify the accompanying SHA-256 checksum before installing.

- **macOS**: Copy `ftop.app` to `/Applications`. This build is unsigned and not notarized. If macOS blocks launch after downloading, run:

  ```bash
  xattr -dr com.apple.quarantine /Applications/ftop.app
  ```

  If the installed app is owned by another user and the command reports permission denied, use `sudo xattr -dr com.apple.quarantine /Applications/ftop.app`.
- **Windows**: Portable builds detect updates and open the matching Release page. Replace the executable manually.
- **Ubuntu**: Install or replace the native binary manually; a graphical session and Vulkan-capable driver are required.

The application checks `latest.json` after 20 seconds, then every 24 hours. Development builds do not check automatically. Unsigned distributions never replace themselves. A signed installer update path is not yet shipped.

## Changes

- TODO: describe verified changes.

## Validation

- TODO: list actual native platform results and remaining limitations.

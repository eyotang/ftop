# Application icons

The owl PNG and macOS AppIcon.icns come from Nongfsq/ftop (MIT), whose copyright and license are retained in the root LICENSE and native/macos/README.md. icon.ico wraps the PNG as a Windows icon resource without changing its pixels. app.rc embeds that icon into the Windows executable.

macOS packages declare CFBundleIconFile and include AppIcon.icns. Ubuntu installs icon.png under the hicolor theme and associates the desktop entry with the ftop application ID. Verify Dock/taskbar appearance on each native OS after packaging; compilation alone does not verify desktop shell icon caching.

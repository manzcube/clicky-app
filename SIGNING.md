# Code signing (optional)

Without signing, people downloading clicky see a scary warning the first time.
It still runs — right-click → Open on macOS, "More info" → "Run anyway" on
Windows. If you're only building this for yourself, skip this file entirely.

## macOS

You need an Apple Developer account ($99/year). Then:

1. Create a **Developer ID Application** certificate in the Apple Developer
   portal and download it.
2. Export it from Keychain Access as a `.p12` with a password.
3. Base64 it: `base64 -i cert.p12 | pbcopy`
4. Create an app-specific password at appleid.apple.com.

Add these as GitHub repository secrets (Settings → Secrets → Actions):

| Secret | Value |
|---|---|
| `APPLE_CERTIFICATE` | the base64 string from step 3 |
| `APPLE_CERTIFICATE_PASSWORD` | the .p12 password |
| `APPLE_SIGNING_IDENTITY` | `Developer ID Application: Your Name (TEAMID)` |
| `APPLE_ID` | your Apple ID email |
| `APPLE_PASSWORD` | the app-specific password |
| `APPLE_TEAM_ID` | your 10-character team ID |

The release workflow already reads all of these. Add them and the next tag
produces a signed, notarised build.

### Accessibility permission and signing

One thing that will confuse you: macOS ties the Accessibility permission to the
app's signature. Every time you rebuild an *unsigned* clicky, macOS sees a
different app and you have to remove and re-add it in System Settings. Signing
with a stable identity makes the permission stick.

## Windows

Needs a code signing certificate from a CA (~$200/year), or ship unsigned and
accept the SmartScreen warning. Set `WINDOWS_CERTIFICATE` and
`WINDOWS_CERTIFICATE_PASSWORD` as secrets and add `signCommand` to the bundle
config if you get one.

# Tauri Build and Packaging Policy

This document defines a general build and packaging policy for Tauri macOS projects.

The main goal is to keep ordinary development builds inside the safer default environment, and only grant broader permissions when they are truly needed for release packaging.

Replace the placeholders in this document for each project:

- `<APP_NAME>`: the product name, for example `MacTTC`
- `<APP_BUNDLE>`: the app bundle name, for example `MacTTC.app`
- `<VERSION>`: the app version, for example `0.9.5`
- `<ARCH>`: the build architecture, for example `aarch64`
- `<DMG_NAME>`: the final DMG filename, for example `MacTTC_0.9.5_aarch64.dmg`

## Development Builds

During normal development and testing, build only the macOS `.app` bundle.

Do not build a DMG during ordinary development.

Recommended checks:

```sh
npm run build
```

```sh
cd src-tauri
cargo test
```

Build only the app bundle:

```sh
npm run tauri:build -- --bundles app
```

Expected app output:

```text
src-tauri/target/release/bundle/macos/<APP_BUNDLE>
```

## Packaging Test

When a packaging test is requested, do not use unrestricted `npm run tauri:build`.

Use this flow instead:

1. Build only the `.app` bundle.
2. Review the exact manual DMG commands.
3. Package the generated `.app` with `hdiutil create`.
4. Verify the DMG with `hdiutil verify`.

Build only the app bundle:

```sh
npm run tauri:build -- --bundles app
```

Create a DMG manually:

```sh
rm -rf /private/tmp/<app-name>-dmg-root
mkdir -p /private/tmp/<app-name>-dmg-root
cp -R src-tauri/target/release/bundle/macos/<APP_BUNDLE> /private/tmp/<app-name>-dmg-root/
ln -s /Applications /private/tmp/<app-name>-dmg-root/Applications
hdiutil create \
  -volname <APP_NAME> \
  -srcfolder /private/tmp/<app-name>-dmg-root \
  -ov \
  -format UDZO \
  src-tauri/target/release/bundle/dmg/<DMG_NAME>
```

Verify the DMG:

```sh
hdiutil verify src-tauri/target/release/bundle/dmg/<DMG_NAME>
```

The manual DMG should contain:

```text
<APP_BUNDLE>
Applications
```

## Formal Release

When a formal release is requested, unrestricted `npm run tauri:build` may be used only after reviewing the files that control build behavior.

Review these files before granting unrestricted build permission:

- `package.json`
- `package-lock.json`
- `src-tauri/tauri.conf.json`
- `src-tauri/target/release/bundle/dmg/bundle_dmg.sh`

Confirm the following:

- `package.json` scripts only run expected build commands.
- `package-lock.json` has no unexpected dependency changes.
- `src-tauri/tauri.conf.json` has expected app identity, version, window, icon, and bundle settings.
- `bundle_dmg.sh` only performs expected DMG creation, Finder layout, signing, notarization, or disk image cleanup steps.
- No reviewed script contains unexpected network access, destructive file operations, credential access, or unrelated filesystem changes.

After review, run the formal release build with unrestricted permissions:

```sh
npm run tauri:build
```

Expected release outputs:

```text
src-tauri/target/release/bundle/macos/<APP_BUNDLE>
src-tauri/target/release/bundle/dmg/<DMG_NAME>
```

Verify the final DMG:

```sh
hdiutil verify src-tauri/target/release/bundle/dmg/<DMG_NAME>
```

## Why This Policy Exists

Tauri's built-in DMG packaging uses a generated `bundle_dmg.sh` script. That script calls macOS tools such as:

- `hdiutil`
- `osascript`
- `SetFile`

These tools may require broader macOS filesystem and disk image permissions than ordinary app compilation.

In a restricted execution environment, the generated DMG script can fail when it indirectly calls `hdiutil create`, even when a direct reviewed `hdiutil create` command succeeds. For packaging tests, manually running a small reviewed `hdiutil create` command keeps the permission surface smaller.

## Version Notes

When updating the app version, keep version references in sync across the project.

Common files to check:

- `package.json`
- `package-lock.json`
- `src-tauri/Cargo.toml`
- `src-tauri/tauri.conf.json`
- `README.md`
- Release or packaging documents, if they mention versioned filenames

If a project uses signing or notarization, also confirm that release instructions and CI/CD settings use the same version and artifact name.

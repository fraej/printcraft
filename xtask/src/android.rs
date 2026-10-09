//! `cargo xtask android [--release] [--install] [--emulator]`: build the Android app
//! (apps/printcraft-android) into an APK with cargo-apk, and optionally install and start it on
//! a device or emulator. `--emulator` builds for x86_64 (the emulator's system images) instead
//! of 64-bit ARM (phones).
//!
//! Needs: `cargo install cargo-apk`, `rustup target add aarch64-linux-android` (and
//! `x86_64-linux-android` for `--emulator`), the Android SDK
//! (platform 34+, build-tools) and the NDK. The SDK is found through `ANDROID_HOME` (or the
//! usual install folder), the NDK through `ANDROID_NDK_ROOT` (or the newest in `<sdk>/ndk`).

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, anyhow, bail};

use crate::gates::{root, target_dir};

const PACKAGE: &str = "ai.storyteller.printcraft";
/// Launcher icons: (density folder, the app icon size used for it).
const ICONS: &[(&str, u32)] = &[("mipmap-mdpi", 48), ("mipmap-xhdpi", 128), ("mipmap-xxxhdpi", 256)];

pub fn run(args: &[String]) -> anyhow::Result<()> {
    let mut release = false;
    let mut install = false;
    let mut emulator = false;
    for a in args {
        match a.as_str() {
            "--release" => release = true,
            "--install" => install = true,
            "--emulator" => emulator = true,
            other => bail!("unknown option {other} (use --release, --install, --emulator)"),
        }
    }
    let sdk = sdk_dir()?;
    let ndk = ndk_dir(&sdk)?;
    println!("Android SDK: {}\nAndroid NDK: {}", sdk.display(), ndk.display());
    write_resources()?;
    // cargo-apk packs every library it finds here: drop the other architecture's.
    let staged = target_dir().join(if release { "release" } else { "debug" }).join("apk").join("lib");
    if staged.is_dir() {
        std::fs::remove_dir_all(&staged).with_context(|| format!("clearing {}", staged.display()))?;
    }

    let mut c = Command::new("cargo");
    c.current_dir(root()).args(["apk", "build", "-p", "printcraft-android"]);
    if release {
        c.arg("--release");
    }
    // The emulator's system images are x86_64; phones are 64-bit ARM (the manifest's default).
    if emulator {
        c.args(["--target", "x86_64-linux-android"]);
    }
    // A release APK needs a key. Without one (CARGO_APK_RELEASE_KEYSTORE), sign with the debug
    // key Android tooling keeps in ~/.android: fine for your own devices, not for publishing.
    if release && std::env::var_os("CARGO_APK_RELEASE_KEYSTORE").is_none() {
        let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).map(PathBuf::from);
        if let Some(key) = home.map(|h| h.join(".android").join("debug.keystore")).filter(|k| k.is_file()) {
            println!("Signing the release APK with the debug key {} (set CARGO_APK_RELEASE_KEYSTORE to publish)", key.display());
            c.env("CARGO_APK_RELEASE_KEYSTORE", key).env("CARGO_APK_RELEASE_KEYSTORE_PASSWORD", "android");
        }
    }
    c.env("ANDROID_HOME", &sdk).env("ANDROID_SDK_ROOT", &sdk).env("ANDROID_NDK_ROOT", &ndk).env("ANDROID_NDK_HOME", &ndk);
    let status = c.status().context("running cargo apk (install it with `cargo install cargo-apk`)")?;
    if !status.success() {
        bail!("cargo apk build failed");
    }
    let apk = target_dir().join(if release { "release" } else { "debug" }).join("apk").join("printcraft.apk");
    if !apk.is_file() {
        bail!("cargo apk finished but {} is missing", apk.display());
    }
    println!("APK: {}", apk.display());
    if install {
        let adb = sdk.join("platform-tools").join(format!("adb{}", std::env::consts::EXE_SUFFIX));
        let ok = Command::new(&adb).arg("install").arg("-r").arg(&apk).status().context("running adb")?.success();
        if !ok {
            bail!("adb install failed (is a device connected and USB debugging on? `adb devices`)");
        }
        let started = Command::new(&adb)
            .args(["shell", "am", "start", "-n", &format!("{PACKAGE}/android.app.NativeActivity")])
            .status()
            .context("running adb")?
            .success();
        if !started {
            bail!("installed, but adb couldn't start the app");
        }
    }
    Ok(())
}

/// The SDK: `ANDROID_HOME`, `ANDROID_SDK_ROOT`, or where Android Studio puts it.
fn sdk_dir() -> anyhow::Result<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    let guess = if cfg!(windows) {
        env("LOCALAPPDATA").map(|d| d.join("Android").join("Sdk"))
    } else if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Android/sdk"))
    } else {
        env("HOME").map(|h| h.join("Android/Sdk"))
    };
    env("ANDROID_HOME")
        .or_else(|| env("ANDROID_SDK_ROOT"))
        .or(guess)
        .filter(|d| d.join("platforms").is_dir())
        .ok_or_else(|| anyhow!("no Android SDK found: set ANDROID_HOME (it needs platforms/ and build-tools/)"))
}

/// The NDK: `ANDROID_NDK_ROOT`, `ANDROID_NDK_HOME`, or the newest under `<sdk>/ndk`.
fn ndk_dir(sdk: &Path) -> anyhow::Result<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if let Some(d) = env("ANDROID_NDK_ROOT").or_else(|| env("ANDROID_NDK_HOME")) {
        return Ok(d);
    }
    let mut versions: Vec<PathBuf> = std::fs::read_dir(sdk.join("ndk"))
        .map(|rd| rd.flatten().map(|e| e.path()).filter(|p| p.join("source.properties").is_file()).collect())
        .unwrap_or_default();
    versions.sort_by_key(|p| version_key(p));
    versions.pop().ok_or_else(|| anyhow!("no NDK found: install one with the SDK manager or set ANDROID_NDK_ROOT"))
}

/// `30.0.14904198` → [30, 0, 14904198] (folders that aren't versions sort first).
fn version_key(p: &Path) -> Vec<u64> {
    p.file_name().map(|n| n.to_string_lossy().split('.').map(|s| s.parse().unwrap_or(0)).collect()).unwrap_or_default()
}

/// The window theme: the system's light or dark look, no title bar. Android 15 would draw an
/// app that targets it under the status and navigation bars; `PrintCraft` keeps them apart.
const THEME: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<resources>
    <style name="PrintCraft" parent="@android:style/Theme.DeviceDefault.DayNight">
        <item name="android:windowActionBar">false</item>
        <item name="android:windowNoTitle">true</item>
    </style>
</resources>
"#;
const THEME_V35: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<resources>
    <style name="PrintCraft" parent="@android:style/Theme.DeviceDefault.DayNight">
        <item name="android:windowActionBar">false</item>
        <item name="android:windowNoTitle">true</item>
        <item name="android:windowOptOutEdgeToEdgeEnforcement">true</item>
    </style>
</resources>
"#;

/// Write the APK's resources: the theme, and the app icon (assets/app-icon, already attributed).
fn write_resources() -> anyhow::Result<()> {
    let res = root().join("apps/printcraft-android/res");
    for (folder, xml) in [("values", THEME), ("values-v35", THEME_V35)] {
        std::fs::create_dir_all(res.join(folder))?;
        std::fs::write(res.join(folder).join("styles.xml"), xml)?;
    }
    for (folder, size) in ICONS {
        let src = root().join(format!("assets/app-icon/hicolor/{size}x{size}/apps/{PACKAGE}.png"));
        let dir = res.join(folder);
        std::fs::create_dir_all(&dir)?;
        std::fs::copy(&src, dir.join("ic_launcher.png")).with_context(|| format!("copying {}", src.display()))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ndk_versions_sort_numerically() {
        let mut v = [PathBuf::from("ndk/9.1.0"), PathBuf::from("ndk/30.0.14904198"), PathBuf::from("ndk/27.2.1")];
        v.sort_by_key(|p| version_key(p));
        assert_eq!(v.last(), Some(&PathBuf::from("ndk/30.0.14904198")));
    }

    #[test]
    fn launcher_icons_exist() {
        for (_, size) in ICONS {
            assert!(root().join(format!("assets/app-icon/hicolor/{size}x{size}/apps/{PACKAGE}.png")).is_file());
        }
    }
}

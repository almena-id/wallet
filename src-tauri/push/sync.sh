#!/bin/sh
# Puts what push needs into the native projects under src-tauri/gen, which
# `tauri android|ios init` regenerates without it: the notification's words,
# the APNs entitlement, and Firebase in the Android build — and, while it is
# patching the native projects, the iOS privacy manifest (PrivacyInfo.xcprivacy)
# and the Android backup rules that leave the vault out (backup/). Safe to run again;
# run from src-tauri/ (the Taskfile's `push:sync` does).
set -eu

android=gen/android
if [ -d "$android/app/src/main/res" ]; then
  cp push/android/values/almena_push.xml "$android/app/src/main/res/values/"

  # The vault's record stays out of the system's backups (backup/): sealed
  # under this phone's Keystore, it opens nowhere else.
  mkdir -p "$android/app/src/main/res/xml"
  cp backup/data_extraction_rules.xml backup/backup_rules.xml "$android/app/src/main/res/xml/"
  manifest="$android/app/src/main/AndroidManifest.xml"
  if ! grep -q dataExtractionRules "$manifest"; then
    sed -i.bak 's|^\(        android:icon="@mipmap/ic_launcher"\)$|        android:dataExtractionRules="@xml/data_extraction_rules"\
        android:fullBackupContent="@xml/backup_rules"\
\1|' "$manifest"
    rm -f "$manifest.bak"
  fi
  mkdir -p "$android/app/src/main/res/values-es"
  cp push/android/values-es/almena_push.xml "$android/app/src/main/res/values-es/"

  # The status bar icon: the launcher's monochrome layer, white on nothing.
  manifest="$android/app/src/main/AndroidManifest.xml"
  if ! grep -q default_notification_icon "$manifest"; then
    sed -i.bak 's|^\(        <provider\)$|        <meta-data\
            android:name="com.google.firebase.messaging.default_notification_icon"\
            android:resource="@mipmap/ic_launcher_monochrome" />\
\
\1|' "$manifest"
    rm -f "$manifest.bak"
  fi

  # Firebase and the push plugin are built with Kotlin 2.1, which the 1.9
  # compiler the template pins cannot read.
  root="$android/build.gradle.kts"
  sed -i.bak 's|kotlin-gradle-plugin:1\.9\.[0-9]*|kotlin-gradle-plugin:2.1.21|' "$root"
  rm -f "$root.bak"

  # Firebase reads its project from google-services.json, which is the
  # publisher's and not in the repository. Without it the app builds and
  # simply gets no token.
  root="$android/build.gradle.kts"
  if ! grep -q google-services "$root"; then
    sed -i.bak 's|^\(        classpath("org.jetbrains.kotlin:kotlin-gradle-plugin:.*")\)$|\1\
        classpath("com.google.gms:google-services:4.4.2")|' "$root"
    rm -f "$root.bak"
  fi
  app="$android/app/build.gradle.kts"
  if ! grep -q google-services "$app"; then
    cat >> "$app" <<'GRADLE'

// Push: Firebase, only when the project's google-services.json is here.
if (file("google-services.json").exists()) {
    apply(plugin = "com.google.gms.google-services")
}
GRADLE
  fi
  echo "Push synced into $android"
fi

apple=gen/apple
if [ -d "$apple/wallet_iOS" ]; then
  cp push/ios/wallet_iOS.entitlements "$apple/wallet_iOS/"
  cp PrivacyInfo.xcprivacy "$apple/wallet_iOS/"
  for lang in en es; do
    mkdir -p "$apple/wallet_iOS/$lang.lproj"
    cp "push/ios/$lang.lproj/Localizable.strings" "$apple/wallet_iOS/$lang.lproj/"
  done
  python3 push/xcode.py "$apple/wallet.xcodeproj/project.pbxproj"
  echo "Push synced into $apple"
fi

# XRVis

Mixed reality visualizations for the SSL, using [bevy](https://bevy.org)

## SSLGame

SSLGame is the shared backend crate for receiving game state from hosts in the local network, encoding it into the ECS,
and rendering it in 3d. It is **not** a full standalone ssl stack, but rather acts as a thin client that relies on
external hosts for simulation and robot interactions.

## Desktop

A traditional "2d" desktop frontend for sslgame. Debugging in VR is annoying, so this application exists to make
development less painful. It is currently used to test new features before building full vr interactions for them, but
it might be expanded to provide visualization overlays for the public livestreams in the future.

## VR

The main "production" frontend, focussed on intuitive hand-tracked interactions and passthrough rendering. It primarily
targets standalone meta quest headsets, but it should work with any headset supporting openxr, even without passthrough.

### Android

The `xrvis-vr/android` folder contains a gradle project that compiles xrvis-vr for the correct target architecture and
android platform version using `cargo-ndk`, collects the assets, and packages everything into an apk with all the
required metadata for running on meta quest headsets.

Build instructions:

- Make sure you have rust installed with both your native and the aarch64-linux-android (
  `rustup target add aarch64-linux-android`) targets. The native one is still used to execute build scripts.
- Java is required to run the android tooling. But gradle is not always forward-compatible, so new versions might
  require you to update the gradle wrapper version using `gradle wrapper --gradle-version <version>`, as long as it is
  still supported by the android gradle plugin (AGP). Android studio bundles its own compatible jdk.
- You also need the official android SDK, including the NDK
    - The easiest way to get it is to install the Android Studio IDE and
      manually [add](https://developer.android.com/studio/projects/install-ndk#default-version) the NDK feature in its
      SDK manager.
    - If you don't need the full IDE, you can also use
      the [command line tools](https://developer.android.com/tools/sdkmanager). AGP's ndk autodownload is tied to its
      makefile/cmake integration, so you need to manually install
      the [latest LTS version](https://developer.android.com/ndk/downloads#lts-downloads) with this command:
      `sdkmanager --install "ndk;<version>"`.
- The gradle buildscript needs [cargo-ndk](https://github.com/bbqsrc/cargo-ndk) to correctly build and link the rust
  code for android. You can either install it using `cargo install --locked cargo-ndk`, or use a system package if your
  distro has one available and you don't want to compile it yourself.
    - cargo-ndk will try to find the android ndk in its default location for android studio (usually
      \~/Android/Sdk/ndk), but this detection can be overwritten using the `ANDROID_HOME` (\~/Android/Sdk) or
      `ANDROID_NDK_HOME` (\~/Android/Sdk/ndk) environment variables.
- HorizonOS only accepts release builds when they are signed, so release builds require bringing your own keystore.jks
  and keystore.properties. The .jks can
  be [generated](https://developer.android.com/studio/publish/app-signing#generate-key) using android studio, and the
  .properties contains the password and key name to keep
  them [out of the buildscript](https://developer.android.com/studio/publish/app-signing#secure-shared-keystore).
- To build the apk you can either use the android studio tooling, or run gradle manually using `./gradlew buildDebug`
  and `./gradlew buildRelease`. The apk will be located in `build/outputs/apk/` and can be installed using
  `adb install <apk-path-here>`. You can also build and install at once using `./gradlew install<Buildtype>`.
- On windows or mac, the [meta quest developer hub](https://developers.meta.com/horizon/documentation/unity/ts-mqdh)
  app could also be useful for managing installed apps, switching to wireless adb, and performance analysis.

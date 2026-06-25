import java.io.FileInputStream
import java.util.Properties

plugins {
    id("com.android.application") version "9.2.1"
}

// Compile and link the rust project using cargo-ndk.
// Each project variant gets its own build task because they can have different build targets.
androidComponents.onVariants { variant ->
    val variantName = variant.name.replaceFirstChar { it.uppercase() }

    val outputDir = objects.directoryProperty()
    outputDir.set(layout.buildDirectory.dir("generated/rust/$variantName/jniLibs"))

    val cargoTask = tasks.register<Exec>("cargoNdkBuild$variantName") {
        // Always declare as outdated to force a rebuild. Gradle can't reliably check for changes
        // in the rust project, so this is better handled by cargo's internal caching
        outputs.upToDateWhen { false }

        outputs.dir(outputDir)

        // Set workdir to the cargo project (assuming the android project is a subdir of the cargo project)
        workingDir = layout.projectDirectory.asFile.parentFile

        val platform = variant.targetSdk
        // This could be overridden by the flavor, but correctly handling multi-abi builds is quite complicated
        val abi = android.defaultConfig.ndk.abiFilters.firstOrNull() ?: "arm64-v8a"
        val isRelease = !variant.debuggable

        commandLine("cargo", "ndk", "build", "--link-libcxx-shared")
        args("--platform", platform.apiLevel)
        args("--target", abi)
        if (isRelease) args("--release")
        args("--output-dir", outputDir.get().asFile.absolutePath)
    }

    variant.sources.jniLibs?.addGeneratedSourceDirectory(cargoTask) { outputDir }
}

// Load signing properties from an uncommitted properties file
val keystoreFile = rootProject.file("keystore.jks")
val keystorePropertiesFile = rootProject.file("keystore.properties")
val keystoreProperties = Properties()
keystoreProperties.load(FileInputStream(keystorePropertiesFile))

android {
    namespace = "de.erforce.xrvis_vr"
    compileSdk {
        version = release(37)
    }

    defaultConfig {
        applicationId = "de.erforce.xrvis_vr"
        minSdk = 32
        targetSdk = 34
        versionCode = 1
        versionName = "1.0"

        ndk {
            abiFilters.add("arm64-v8a")
        }
    }

    signingConfigs {
        create("optional") {
            if (keystoreFile.exists()) {
                storeFile = keystoreFile
                storePassword = keystoreProperties["storePassword"] as String
                keyAlias = keystoreProperties["keyAlias"] as String
                keyPassword = keystoreProperties["keyPassword"] as String
            }
        }
        create("required") {
            storeFile = keystoreFile
            storePassword = keystoreProperties["storePassword"] as String
            keyAlias = keystoreProperties["keyAlias"] as String
            keyPassword = keystoreProperties["keyPassword"] as String
        }
    }

    buildTypes {
        debug {
            isDebuggable = true
            isJniDebuggable = true
            // Also sign debug builds where possible to make switching between debug and release builds easier
            signingConfig = signingConfigs.getByName("optional")
        }
        release {
            isDebuggable = false
            isJniDebuggable = false
            signingConfig = signingConfigs.getByName("required")
        }
    }

    // Set the java version. The default of java 8 is causing deprecation warnings
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_11
        targetCompatibility = JavaVersion.VERSION_11
    }

    // Include the bevy asset dir
    sourceSets.getByName("main") {
        assets.directories.add("../../assets")
    }
}

dependencies {
    // Only needed for GameActivity
    // Has to be matched to the android-activity crate (android-activity 0.6.1 -> games-activity 4.x)
    //implementation("androidx.games:games-activity:4.4.2")
    // Only used for the @style/Theme.AppCompat.NoActionBar theme in the manifest.
    //implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("org.khronos.openxr:openxr_loader_for_android:1.1.60")
}

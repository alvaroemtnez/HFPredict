plugins {
    alias(libs.plugins.android.application)
    alias(libs.plugins.kotlin.compose)
}

// One version for the whole project: [workspace.package] in Cargo.toml.
val appVersion: String = Regex("""(?m)^version\s*=\s*"([^"]+)"""")
    .find(rootDir.resolve("../Cargo.toml").readText())!!.groupValues[1]
// 1.2.3 -> 10203: increases with every release. A beta (1.2.3-beta.1) has its
// release's code: the scheme has no room below a release, and Android installs
// an equal code as an update, so beta.1 -> beta.2 -> 1.2.3 each install over the last.
val appVersionCode: Int = appVersion.substringBefore('-').split(".").map { it.toInt() }
    .let { (a, b, c) -> a * 10000 + b * 100 + c }

android {
    namespace = "io.alvaroedo.hfpredict"
    compileSdk = 37
    // build.sh installs the SDK packages named here and by compileSdk.
    buildToolsVersion = "37.0.0"
    ndkVersion = "30.0.16248370"

    defaultConfig {
        applicationId = "io.alvaroedo.HFPredict"
        minSdk = 26
        targetSdk = 37
        versionCode = appVersionCode
        versionName = appVersion
        // The manifest's ".Name" components live in the namespace, not in the
        // application id (different case, and ".debug" on debug builds).
        buildConfigField("String", "NAMESPACE", "\"$namespace\"")
    }

    buildTypes {
        debug {
            applicationIdSuffix = ".debug"
            versionNameSuffix = "-debug"
        }
        release {
            isMinifyEnabled = true
            isShrinkResources = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
        }
    }

    splits {
        abi {
            // One APK per ABI (smaller downloads, what Obtainium picks from)
            // and one universal APK.
            // Off for the app bundle (Accrescent), which bundletool splits itself.
            isEnable = !project.hasProperty("hfp.bundle")
            reset()
            include("arm64-v8a", "armeabi-v7a", "x86_64")
            isUniversalApk = true
        }
    }

    // build.sh builds the Rust core and generates its Kotlin bindings here.
    sourceSets {
        getByName("main") {
            kotlin.directories += "build/generated/uniffi/kotlin"
            jniLibs.directories += "build/generated/uniffi/jniLibs"
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    buildFeatures {
        compose = true
        buildConfig = true
    }

    androidResources {
        localeFilters += listOf("en", "es", "ca", "de", "ja", "ar")
        generateLocaleConfig = true
    }

    // Every language in every build: the language can be changed in the
    // app, with no Play Store to download it from.
    bundle {
        language { enableSplit = false }
    }

    // No Google-only signing block in the APK (F-Droid and reproducibility).
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }

    packaging {
        // The Rust library arrives stripped.
        jniLibs.keepDebugSymbols += "**/*.so"
        // The Rust core is built for arm64-v8a, armeabi-v7a and x86_64 only
        // (build.sh ALL_ABIS). JNA and AndroidX also ship libraries for
        // other ABIs (x86, and JNA the obsolete armeabi, mips and mips64);
        // left in, the universal APK and the app bundle's x86 split would
        // install on such a device and crash without the core. Without
        // them, such a device is simply not offered the app.
        jniLibs.excludes += listOf("lib/x86/**", "lib/armeabi/**", "lib/mips/**", "lib/mips64/**")
    }

    lint {
        abortOnError = true
        checkReleaseBuilds = true
        warningsAsErrors = true
        informational += setOf("GradleDependency", "AndroidGradlePluginVersion", "NewerVersionAvailable")
        // The generated UniFFI bindings are not ours to lint.
        disable += setOf("SyntheticAccessor")
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

dependencies {
    implementation(libs.androidx.core.ktx)
    implementation(libs.androidx.core.splashscreen)
    implementation(libs.androidx.activity.compose)
    implementation(libs.androidx.lifecycle.viewmodel.compose)
    implementation(libs.androidx.lifecycle.runtime.compose)
    implementation(libs.kotlinx.coroutines.android)
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.compose.ui)
    implementation(libs.androidx.compose.ui.tooling.preview)
    implementation(libs.androidx.compose.material3)
    implementation(libs.androidx.compose.material3.window.size)
    implementation(libs.jna) { artifact { type = "aar" } }
    debugImplementation(libs.androidx.compose.ui.tooling)

    testImplementation(libs.junit)
}

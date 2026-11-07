plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

val coreFfiLibraryDirectory = providers.gradleProperty("idroidCoreFfiLibDir").orNull
val idroidNdkPath = providers.gradleProperty("idroidNdkPath").orNull

dependencyLocking {
    lockAllConfigurations()
}

android {
    namespace = "com.idroid.verticalslice"
    compileSdk = 35
    ndkVersion = "28.2.13676358"
    if (idroidNdkPath != null) ndkPath = idroidNdkPath

    defaultConfig {
        applicationId = "com.idroid.verticalslice"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0-experiment"
        ndk { abiFilters += listOf("arm64-v8a") }
    }

    buildTypes {
        release {
            isMinifyEnabled = false
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions { jvmTarget = "17" }

    if (coreFfiLibraryDirectory != null) {
        externalNativeBuild {
            cmake { path = file("src/main/cpp/CMakeLists.txt") }
        }
    }

    defaultConfig {
        if (coreFfiLibraryDirectory != null) {
            externalNativeBuild {
                cmake { arguments("-DIDROID_CORE_FFI_LIB_DIR=$coreFfiLibraryDirectory") }
            }
        }
    }
}

dependencies {
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    implementation("com.google.mlkit:barcode-scanning:17.3.0")
    implementation("io.github.webrtc-sdk:android:125.6422.07")
}

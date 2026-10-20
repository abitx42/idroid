plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

dependencyLocking {
    lockAllConfigurations()
}

android {
    namespace = "com.idroid.verticalslice"
    compileSdk = 35

    defaultConfig {
        applicationId = "com.idroid.verticalslice"
        minSdk = 29
        targetSdk = 35
        versionCode = 1
        versionName = "0.1.0-experiment"
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
}

dependencies {
    implementation("io.github.webrtc-sdk:android:125.6422.07")
}

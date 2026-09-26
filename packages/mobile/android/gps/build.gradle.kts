// dx strips plugin versions and uses its own AGP/Kotlin; they're kept for standalone IDE use.
plugins {
    id("com.android.library") version "8.7.0"
    kotlin("android") version "2.0.20"
}

android {
    namespace = "dev.traced.gps"
    compileSdk = 34

    defaultConfig {
        minSdk = 26
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions {
        jvmTarget = "17"
    }
}

dependencies {
    implementation("androidx.core:core-ktx:1.12.0")
    implementation("com.google.android.gms:play-services-location:21.3.0")
}

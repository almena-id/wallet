plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

android {
    namespace = "id.almena.call"
    compileSdk = 36

    defaultConfig {
        minSdk = 24
        consumerProguardFiles("consumer-rules.pro")
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_1_8
        targetCompatibility = JavaVersion.VERSION_1_8
    }
    kotlinOptions {
        jvmTarget = "1.8"
    }
}

dependencies {
    implementation(project(":tauri-android"))
    // The notifications plugin keeps the FCM token and shows ordinary pushes;
    // this plugin's messaging service passes it everything but a call.
    implementation(project(":tauri-plugin-notifications"))
    implementation("com.google.firebase:firebase-messaging:24.1.2")
    implementation("androidx.core:core-ktx:1.13.1")
}

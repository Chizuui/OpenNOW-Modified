plugins {
    id("com.android.test")
    id("androidx.baselineprofile") version "1.5.0-beta01"
}

android {
    namespace = "com.opencloudgaming.opennow.baselineprofile"
    compileSdk = 37

    defaultConfig {
        minSdk = 28
        targetSdk = 36
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    targetProjectPath = ":app"
}

kotlin {
    jvmToolchain(17)
}

// Runs on a Gradle Managed Device / connected device. Pick one with:
//   ./gradlew :app:generateBaselineProfile
baselineProfile {
    // Emit onto a real/managed device (not an unrooted target that can't compile).
    useConnectedDevices = true
}

dependencies {
    implementation("androidx.test.ext:junit:1.2.1")
    implementation("androidx.test.uiautomator:uiautomator:2.3.0")
    implementation("androidx.benchmark:benchmark-macro-junit4:1.5.0-beta01")
}

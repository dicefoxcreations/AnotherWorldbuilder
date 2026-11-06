import java.util.Properties
import org.gradle.api.artifacts.component.ModuleComponentIdentifier

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
    id("rust")
}

val tauriProperties = Properties().apply {
    val propFile = file("tauri.properties")
    if (propFile.exists()) {
        propFile.inputStream().use { load(it) }
    }
}

android {
    compileSdk = 36
    namespace = "com.dicefoxcreations.anotherworldbuilder"
    defaultConfig {
        manifestPlaceholders["usesCleartextTraffic"] = "false"
        applicationId = "com.dicefoxcreations.anotherworldbuilder"
        minSdk = 26
        targetSdk = 36
        versionCode = tauriProperties.getProperty("tauri.android.versionCode", "1").toInt()
        versionName = tauriProperties.getProperty("tauri.android.versionName", "1.0")
        // The server's device test (`tools/test/android-server.py`).
        testInstrumentationRunner = "com.dicefoxcreations.anotherworldbuilder.server.ServerInstrumentation"
    }
    buildTypes {
        getByName("debug") {
            manifestPlaceholders["usesCleartextTraffic"] = "true"
            isDebuggable = true
            isJniDebuggable = true
            isMinifyEnabled = false
            packaging {
                jniLibs.keepDebugSymbols.add("*/arm64-v8a/*.so")
                jniLibs.keepDebugSymbols.add("*/armeabi-v7a/*.so")
                jniLibs.keepDebugSymbols.add("*/x86/*.so")
                jniLibs.keepDebugSymbols.add("*/x86_64/*.so")
            }
        }
        getByName("release") {
            isMinifyEnabled = true
            // Compressed in the APK, unpacked when installed: a third of the download.
            packaging { jniLibs.useLegacyPackaging = true }
            proguardFiles(
                *fileTree(".") { include("**/*.pro") }
                    .plus(getDefaultProguardFile("proguard-android-optimize.txt"))
                    .toList().toTypedArray()
            )
        }
    }
    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }
    kotlinOptions {
        jvmTarget = "17"
    }
    buildFeatures {
        buildConfig = true
    }
    // The license texts the server's own screen shows (`tools/build/android.py`).
    sourceSets["main"].assets.srcDir("../../../../../../build/android/assets")
}

rust {
    rootDirRel = "../../../"
}

dependencies {
    implementation("androidx.webkit:webkit:1.14.0")
    implementation("androidx.appcompat:appcompat:1.7.1")
    implementation("androidx.activity:activity-ktx:1.10.1")
    implementation("com.google.android.material:material:1.12.0")
    implementation("androidx.lifecycle:lifecycle-process:2.10.0")
    testImplementation("junit:junit:4.13.2")
    androidTestImplementation("androidx.test.ext:junit:1.1.4")
    androidTestImplementation("androidx.test.espresso:espresso-core:3.5.0")
}

apply(from = "tauri.build.gradle.kts")

tasks.register("awRuntimeCoordinates") {
    doLast {
        val modules = configurations.getByName("universalReleaseRuntimeClasspath")
            .incoming.resolutionResult.allComponents.mapNotNull { it.id as? ModuleComponentIdentifier }
            .map { "${it.group}:${it.module}:${it.version}" }.distinct().sorted()
        val output = file("../../../../../../build/android/maven-runtime.txt")
        output.parentFile.mkdirs(); output.writeText(modules.joinToString("\n") + "\n")
    }
}

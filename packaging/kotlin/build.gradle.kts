// Gradle build for the UniFFI Kotlin bindings of nova-ta.
// `cargo xtask build -t kotlin` copies this file next to the generated sources in
// bindings/kotlin/ together with the native library under src/main/resources/<os-arch>/.
// The version is kept in sync with Cargo.toml by `cargo xtask check` / `cargo xtask bump`.

plugins {
    kotlin("jvm") version "2.1.20"
    `java-library`
    `maven-publish`
}

group = "io.github.rick29"
version = "0.1.2"

repositories {
    mavenCentral()
}

dependencies {
    api("net.java.dev.jna:jna:5.17.0")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-core:1.10.2")
}

kotlin {
    jvmToolchain(17)
}

publishing {
    publications {
        create<MavenPublication>("maven") {
            artifactId = "nova-ta"
            from(components["java"])
        }
    }
}

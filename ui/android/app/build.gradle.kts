import java.util.Properties
import java.security.KeyStore
import java.security.MessageDigest

plugins {
    id("com.android.application")
    // The Flutter Gradle Plugin must be applied after the Android and Kotlin Gradle plugins.
    id("dev.flutter.flutter-gradle-plugin")
}

// 本地从忽略提交的 key.properties 读取；CI 只在签名步骤注入环境变量。
val releaseKeyProperties = Properties().apply {
    val propertiesPath = providers.gradleProperty("androidSigningProperties").orNull
        ?: "key.properties"
    val propertiesFile = rootProject.file(propertiesPath)
    if (propertiesFile.isFile) propertiesFile.inputStream().use { load(it) }
}
fun releaseSigningValue(env: String, property: String): String? =
    System.getenv(env)?.takeIf { it.isNotBlank() }
        ?: releaseKeyProperties.getProperty(property)?.takeIf { it.isNotBlank() }

val releaseStorePath = releaseSigningValue("ANDROID_KEYSTORE_PATH", "storeFile")
val releaseStorePassword = releaseSigningValue("ANDROID_KEYSTORE_PASSWORD", "storePassword")
val releaseKeyAlias = releaseSigningValue("ANDROID_KEY_ALIAS", "keyAlias")
val releaseKeyPassword = releaseSigningValue("ANDROID_KEY_PASSWORD", "keyPassword")
val releaseSigningReady = listOf(
    releaseStorePath, releaseStorePassword, releaseKeyAlias, releaseKeyPassword,
).all { it != null }

val checkReleaseSigning = tasks.register("checkReleaseSigning") {
    doLast {
        check(releaseSigningReady) {
            "Release 签名未配置：请配置 ui/android/key.properties 或 ANDROID_KEYSTORE_* / ANDROID_KEY_* 环境变量。不得回退到临时 debug 签名。"
        }
        check(rootProject.file(releaseStorePath!!).isFile) {
            "Release 签名密钥文件不存在，请恢复既有密钥，不要重新生成证书。"
        }
        val certificate = try {
            KeyStore.getInstance(rootProject.file(releaseStorePath), releaseStorePassword!!.toCharArray())
                .getCertificate(releaseKeyAlias)
        } catch (_: Exception) {
            error("无法读取固定签名证书，请检查密钥文件、alias 和口令。")
        }
        check(certificate != null) { "固定签名 alias 不存在。" }
        val fingerprint = MessageDigest.getInstance("SHA-256").digest(certificate.encoded)
            .joinToString("") { "%02x".format(it.toInt() and 0xff) }
        check(fingerprint == rootProject.file("release-signing.sha256").readText().trim()) {
            "签名证书与 release-signing.sha256 不同，停止构建以免生成无法覆盖升级的 APK。"
        }
    }
}
tasks.matching { it.name == "preReleaseBuild" }.configureEach {
    dependsOn(checkReleaseSigning)
}

android {
    namespace = "dev.elsewhen.elsewhen_ui"
    compileSdk = flutter.compileSdkVersion
    ndkVersion = flutter.ndkVersion

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    defaultConfig {
        // TODO: Specify your own unique Application ID (https://developer.android.com/studio/build/application-id.html).
        applicationId = "dev.elsewhen.elsewhen_ui"
        // You can update the following values to match your application needs.
        // For more information, see: https://flutter.dev/to/review-gradle-config.
        minSdk = flutter.minSdkVersion
        targetSdk = flutter.targetSdkVersion
        // Uses the version code from pubspec.yaml. When using split APKs, 1000 * ABI_VERSION
        // is added automatically by Flutter. (https://developer.android.com/studio/build/configure-apk-splits#configure-APK-versions)
        // You can force using the value of versionCode by specifying the `-P force-version-code-ignoring-abi=true`
        // flag during build.
        versionCode = flutter.versionCode
        versionName = flutter.versionName

        // Rust 核心只编 arm64（见 scripts/build-android-rust.sh）
        ndk {
            abiFilters += listOf("arm64-v8a")
        }
    }

    // 预编译的 Rust cdylib（libelsewhen.so），由仓库根 scripts/build-android-rust.sh 产出。
    // Gradle 自动把 sourceSets main jniLibs.srcDir 下的 .so 打进 APK 的 lib/<abi>/。
    sourceSets {
        getByName("main") {
            jniLibs.srcDir("../../rustLibs")
        }
    }

    // 预编译 .so 没有 strip 任务，避免打包期 strip 报错。
    packagingOptions {
        jniLibs {
            useLegacyPackaging = false
        }
    }

    signingConfigs {
        if (releaseSigningReady) {
            create("release") {
                storeFile = rootProject.file(releaseStorePath!!)
                storePassword = releaseStorePassword
                keyAlias = releaseKeyAlias
                keyPassword = releaseKeyPassword
            }
        }
    }

    buildTypes {
        release {
            signingConfig = if (releaseSigningReady) signingConfigs.getByName("release") else null
        }
    }
}

kotlin {
    compilerOptions {
        jvmTarget = org.jetbrains.kotlin.gradle.dsl.JvmTarget.JVM_17
    }
}

flutter {
    source = "../.."
}

// FileProvider（应用内升级：content:// 共享下载好的 APK 给系统安装器）。
dependencies {
    implementation("androidx.core:core-ktx:1.13.1")
    // Android 12+ 启动页：windowSplashScreenIcon 等主题属性由该库提供
    implementation("androidx.core:core-splashscreen:1.0.1")
}

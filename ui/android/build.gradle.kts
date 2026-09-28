allprojects {
    repositories {
        google()
        mavenCentral()
    }
}

val newBuildDir: Directory =
    rootProject.layout.buildDirectory
        .dir("../../build")
        .get()
rootProject.layout.buildDirectory.value(newBuildDir)

subprojects {
    val newSubprojectBuildDir: Directory = newBuildDir.dir(project.name)
    project.layout.buildDirectory.value(newSubprojectBuildDir)
}
subprojects {
    project.evaluationDependsOn(":app")
}

// 某些插件（keyboard_height_plugin 等）仍以过低的 compileSdk（31）发布，
// 而 androidx.fragment/window 等依赖要求 34+。统一把所有子项目的 compileSdk
// 提升，避免 AAR metadata 检查失败。36 与 Flutter 3.47 默认 compileSdk 对齐。
// 注意不能用 afterEvaluate（部分子项目已被 evaluationDependsOn 提前评估）。
subprojects {
    plugins.withId("com.android.library") {
        val androidExt = extensions.getByName("android") as com.android.build.gradle.BaseExtension
        androidExt.compileSdkVersion(36)
    }
}

tasks.register<Delete>("clean") {
    delete(rootProject.layout.buildDirectory)
}

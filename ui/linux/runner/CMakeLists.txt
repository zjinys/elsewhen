cmake_minimum_required(VERSION 3.13)
project(runner LANGUAGES CXX)

# Embed the app icon PNG into the binary so the GTK window can set its
# taskbar/window icon at runtime in both dev (`flutter run`) and packaged
# builds, independent of the icon theme or a filesystem path.
set(APP_ICON_PNG "${CMAKE_CURRENT_SOURCE_DIR}/app_icon.png")
set(APP_ICON_EMBEDDED_SOURCE "${CMAKE_CURRENT_BINARY_DIR}/app_icon_embedded.cpp")
set(APP_ICON_GEN_SCRIPT "${CMAKE_CURRENT_SOURCE_DIR}/make_embedded_icon.cmake")
add_custom_command(
  OUTPUT "${APP_ICON_EMBEDDED_SOURCE}"
  COMMAND ${CMAKE_COMMAND} -P "${APP_ICON_GEN_SCRIPT}"
  DEPENDS "${APP_ICON_PNG}" "${APP_ICON_GEN_SCRIPT}"
  COMMENT "Generating embedded app icon source"
  VERBATIM
)

# Define the application target. To change its name, change BINARY_NAME in the
# top-level CMakeLists.txt, not the value here, or `flutter run` will no longer
# work.
#
# Any new source files that you add to the application should be added here.
add_executable(${BINARY_NAME}
  "main.cc"
  "my_application.cc"
  "${APP_ICON_EMBEDDED_SOURCE}"
  "${FLUTTER_MANAGED_DIR}/generated_plugin_registrant.cc"
)

# Apply the standard set of build settings. This can be removed for applications
# that need different build settings.
apply_standard_settings(${BINARY_NAME})

# Add preprocessor definitions for the application ID.
add_definitions(-DAPPLICATION_ID="${APPLICATION_ID}")

# Add dependency libraries. Add any application-specific dependencies here.
target_link_libraries(${BINARY_NAME} PRIVATE flutter)
target_link_libraries(${BINARY_NAME} PRIVATE PkgConfig::GTK)

target_include_directories(${BINARY_NAME} PRIVATE "${CMAKE_SOURCE_DIR}")

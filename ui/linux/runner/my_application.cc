#include "my_application.h"

#include <cstddef>
#include <flutter_linux/flutter_linux.h>
#include <gio/gio.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif

#include "flutter/generated_plugin_registrant.h"

struct _MyApplication {
  GtkApplication parent_instance;
  char** dart_entrypoint_arguments;
};

G_DEFINE_TYPE(MyApplication, my_application, GTK_TYPE_APPLICATION)

// App icon bytes, generated from app_icon.png by make_embedded_icon.cmake.
extern "C" const unsigned char kAppIconPng[];
extern "C" const std::size_t kAppIconPngSize;

// Sets the window (taskbar / alt-tab / title-bar) icon from the embedded PNG,
// so the running app shows the Elsewhen logo in every build mode instead of
// falling back to a generic icon.
static void set_window_icon(GtkWindow* window) {
  GBytes* bytes = g_bytes_new_static(kAppIconPng, kAppIconPngSize);
  GInputStream* stream = g_memory_input_stream_new_from_bytes(bytes);
  GError* error = nullptr;
  GdkPixbuf* pixbuf = gdk_pixbuf_new_from_stream(stream, nullptr, &error);
  g_object_unref(stream);
  g_bytes_unref(bytes);

  if (pixbuf != nullptr) {
    gtk_window_set_icon(window, pixbuf);
    g_object_unref(pixbuf);
  } else {
    g_warning("Failed to load embedded app icon: %s",
              error != nullptr ? error->message : "unknown error");
    g_clear_error(&error);
  }
}

// Called when first Flutter frame received.
static void first_frame_cb(MyApplication* self, FlView* view) {
  GtkWidget* toplevel = gtk_widget_get_toplevel(GTK_WIDGET(view));
  // Some Wayland/X11 environments restore decorations while the Flutter
  // surface is being realized. Re-apply this immediately before showing the
  // window; no visual/context is changed here.
  gtk_window_set_titlebar(GTK_WINDOW(toplevel), NULL);
  gtk_window_set_decorated(GTK_WINDOW(toplevel), FALSE);
  gtk_widget_show(toplevel);
}

// Implements GApplication::activate.
static void my_application_activate(GApplication* application) {
  MyApplication* self = MY_APPLICATION(application);
  GtkWindow* window =
      GTK_WINDOW(gtk_application_window_new(GTK_APPLICATION(application)));

  // Show the Elsewhen logo as the window/taskbar icon.
  set_window_icon(window);

  // 无边框窗口。**必须在这里做**（FlView 创建之前），不能在 Dart 侧做：
  //
  // nativeapi 的 `w.titleBarStyle = hidden` 是在 Flutter 已经建好 GL 上下文
  // 之后才执行的，GTK 为此重建窗口的 GdkVisual，与已建好的上下文不匹配，
  // 于是首帧报 `Could not determine GL version` +
  // `Failed to create platform view rendering surface` +
  // `FlutterEngineRunTask returned kInvalidArguments`，表现为窗口只有边框、
  // 完全没有内容。逐项二分定位：跳过 titleBarStyle 即恢复（GL 错误 0），
  // 跳过 backgroundColor / minimumSize / isVisibleInTaskbar / isClosable /
  // title / 尺寸落位 全部无效。
  //
  // 窗口创建时设置则安全（visual 还没被 Flutter 用上）。相应地 Dart 侧
  // window_service_desktop.dart 在 Linux 上不再设 titleBarStyle。
  const gboolean borderless = TRUE;
  gtk_window_set_decorated(window, !borderless);

  // Use a header bar when running in GNOME as this is the common style used
  // by applications and is the setup most users will be using (e.g. Ubuntu
  // desktop).
  // If running on X and not using GNOME then just use a traditional title bar
  // in case the window manager does more exotic layout, e.g. tiling.
  // If running on Wayland assume the header bar will work (may need changing
  // if future cases occur).
  // The Flutter UI supplies its own title bar. Never install a GTK header bar
  // or native title bar, otherwise Linux renders two title bars.
  gtk_window_set_titlebar(window, NULL);
  gtk_window_set_decorated(window, FALSE);
  gtk_window_set_title(window, "Elsewhen");

  gtk_window_set_default_size(window, 1920, 1080);

  g_autoptr(FlDartProject) project = fl_dart_project_new();
  fl_dart_project_set_dart_entrypoint_arguments(
      project, self->dart_entrypoint_arguments);

  FlView* view = fl_view_new(project);
  GdkRGBA background_color;
  // 窗口圆角要求窗口真透明：#00000000 把 FlView 底色设为透明，
  // 角落区域由 Flutter 根部 ClipRRect 裁成圆角后透出桌面（需 RGBA visual，
  // Wayland/X11+compositor 下 GTK3 顶层窗口自动启用）。
  //
  gdk_rgba_parse(&background_color, "#00000000");
  fl_view_set_background_color(view, &background_color);
  gtk_widget_show(GTK_WIDGET(view));
  gtk_container_add(GTK_CONTAINER(window), GTK_WIDGET(view));

  // Show the window when Flutter renders.
  // Requires the view to be realized so we can start rendering.
  g_signal_connect_swapped(view, "first-frame", G_CALLBACK(first_frame_cb),
                           self);
  gtk_widget_realize(GTK_WIDGET(view));

  fl_register_plugins(FL_PLUGIN_REGISTRY(view));

  gtk_widget_grab_focus(GTK_WIDGET(view));
}

// Implements GApplication::local_command_line.
static gboolean my_application_local_command_line(GApplication* application,
                                                  gchar*** arguments,
                                                  int* exit_status) {
  MyApplication* self = MY_APPLICATION(application);
  // Strip out the first argument as it is the binary name.
  self->dart_entrypoint_arguments = g_strdupv(*arguments + 1);

  g_autoptr(GError) error = nullptr;
  if (!g_application_register(application, nullptr, &error)) {
    g_warning("Failed to register: %s", error->message);
    *exit_status = 1;
    return TRUE;
  }

  g_application_activate(application);
  *exit_status = 0;

  return TRUE;
}

// Implements GApplication::startup.
static void my_application_startup(GApplication* application) {
  // MyApplication* self = MY_APPLICATION(object);

  // Perform any actions required at application startup.

  G_APPLICATION_CLASS(my_application_parent_class)->startup(application);
}

// Implements GApplication::shutdown.
static void my_application_shutdown(GApplication* application) {
  // MyApplication* self = MY_APPLICATION(object);

  // Perform any actions required at application shutdown.

  G_APPLICATION_CLASS(my_application_parent_class)->shutdown(application);
}

// Implements GObject::dispose.
static void my_application_dispose(GObject* object) {
  MyApplication* self = MY_APPLICATION(object);
  g_clear_pointer(&self->dart_entrypoint_arguments, g_strfreev);
  G_OBJECT_CLASS(my_application_parent_class)->dispose(object);
}

static void my_application_class_init(MyApplicationClass* klass) {
  G_APPLICATION_CLASS(klass)->activate = my_application_activate;
  G_APPLICATION_CLASS(klass)->local_command_line =
      my_application_local_command_line;
  G_APPLICATION_CLASS(klass)->startup = my_application_startup;
  G_APPLICATION_CLASS(klass)->shutdown = my_application_shutdown;
  G_OBJECT_CLASS(klass)->dispose = my_application_dispose;
}

static void my_application_init(MyApplication* self) {}

MyApplication* my_application_new() {
  // Set the program name to the application ID, which helps various systems
  // like GTK and desktop environments map this running application to its
  // corresponding .desktop file. This ensures better integration by allowing
  // the application to be recognized beyond its binary name.
  g_set_prgname(APPLICATION_ID);

  return MY_APPLICATION(g_object_new(my_application_get_type(),
                                     "application-id", APPLICATION_ID, "flags",
                                     G_APPLICATION_NON_UNIQUE, nullptr));
}

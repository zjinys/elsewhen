#include "my_application.h"

#include <cstddef>
#include <flutter_linux/flutter_linux.h>
#include <gio/gio.h>
#ifdef GDK_WINDOWING_X11
#include <gdk/gdkx.h>
#endif

#ifdef GDK_WINDOWING_WAYLAND
#include <gdk/gdkwayland.h>
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
  // surface is being realized. X11 下重新应用 decorated=FALSE；Wayland 下
  // titlebar（空 header bar）已在 activate 里装好，这里不再覆盖
  // （置 NULL 会把 CSD 拆掉，KWin 立刻画回 SSD 标题栏）。
#ifdef GDK_WINDOWING_WAYLAND
  if (GDK_IS_WAYLAND_DISPLAY(gdk_display_get_default())) {
    // Wayland：空 header bar 已装好，重设 titlebar 为 NULL 会把 CSD 拆掉
    // 导致 KWin 画回 SSD 标题栏；这里只负责 show。
    gtk_widget_show(toplevel);
    return;
  }
#endif
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
  // 运行时（Dart 侧 window_manager / nativeapi 的 setTitleBarStyle(hidden)）
  // 改 GTK 装饰时 GL 上下文已建好，GTK 为此重建窗口的 GdkVisual，与已建好
  // 的上下文不匹配，首帧报 `Could not determine GL version` +
  // `Failed to create platform view rendering surface`，表现为窗口只有边框、
  // 完全没有内容（nativeapi 时代逐项二分定位的坑，window_manager 同病）。
  //
  // 窗口创建时设置则安全（visual 还没被 Flutter 用上）。相应地 Dart 侧
  // window_service_desktop.dart 在 Linux 上跳过 titleBarStyle。
  // X11：gtk_window_set_decorated(FALSE) 走 Motif hints，KWin/任何 WM 都认。
  // Wayland：GTK3 不认 decorated 标志，必须安装 client-side decoration——
  // 给窗口装一个空的 GtkHeaderBar。GTK3 看到应用提供了 titlebar 就会让
  // compositor 关掉服务端装饰；空 header bar 零高度不可见，Flutter 的
  // 自定义标题栏不受影响。仅装 decorated=FALSE 在 KDE Wayland 下无效
  // （KWin 会画 SSD 标题栏，出现双标题栏——Flutter 一条 + KWin 一条）。
#ifdef GDK_WINDOWING_WAYLAND
  if (GDK_IS_WAYLAND_DISPLAY(gdk_display_get_default())) {
    GtkWidget* header_bar = gtk_header_bar_new();
    gtk_header_bar_set_show_close_button(GTK_HEADER_BAR(header_bar), FALSE);
    gtk_header_bar_set_title(GTK_HEADER_BAR(header_bar), NULL);
    gtk_widget_set_size_request(header_bar, 0, 0);
    gtk_window_set_titlebar(window, header_bar);
  } else
#endif
  {
    gtk_window_set_titlebar(window, NULL);
    gtk_window_set_decorated(window, FALSE);
  }
  gtk_window_set_title(window, "Elsewhen");

  gtk_window_set_default_size(window, 1920, 1080);

  g_autoptr(FlDartProject) project = fl_dart_project_new();
  fl_dart_project_set_dart_entrypoint_arguments(
      project, self->dart_entrypoint_arguments);

  FlView* view = fl_view_new(project);
  GdkRGBA background_color;
  // 首帧前窗口底色 = 暗色主题 surface0（0xFF05070C）：设透明曾是圆角
  // 方案的残留，但 Dart 根部早已改为「窗口不透明直角，圆角交给系统」
  // （main.dart FlClash 纪律），透明底色只剩一个效果——Flutter 首帧前
  // 合成器把透明渲成纯黑，每次启动都闪一块黑窗。不透明底色与首帧
  // 无缝衔接；浅色主题用户最多一瞬深底，比合成黑柔和得多。
  gdk_rgba_parse(&background_color, "#05070C");
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

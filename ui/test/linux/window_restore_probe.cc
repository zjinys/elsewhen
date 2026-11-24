#include <flutter_linux/flutter_linux.h>
#include <gdk/gdkwayland.h>

#include <cstdio>
#include <cstring>

#include "window_restore.h"

static GtkWidget* probe_window;
static unsigned int phase = 0;

static gboolean advance(gpointer) {
  if (phase == 11) {
    gtk_main_quit();
    return G_SOURCE_REMOVE;
  }
  if (phase % 2 == 0) {
    std::fprintf(stderr, "PROBE hide=%u\n", phase / 2);
    gtk_widget_hide(probe_window);
  } else {
    std::fprintf(stderr, "PROBE show=%u\n", (phase + 1) / 2);
    gtk_widget_show(probe_window);
    gtk_window_present(GTK_WINDOW(probe_window));
  }
  ++phase;
  return G_SOURCE_CONTINUE;
}

int main(int argc, char** argv) {
  gtk_init(&argc, &argv);
  if (argc < 3 || !GDK_IS_WAYLAND_DISPLAY(gdk_display_get_default())) {
    std::fprintf(stderr, "Usage (Wayland): %s ASSETS ICU [--baseline]\n", argv[0]);
    return 2;
  }
  probe_window = gtk_window_new(GTK_WINDOW_TOPLEVEL);
  gtk_window_set_title(GTK_WINDOW(probe_window), "Elsewhen tray restore probe");
  gtk_window_set_default_size(GTK_WINDOW(probe_window), 640, 400);
  GtkWidget* header_bar = gtk_header_bar_new();
  gtk_header_bar_set_show_close_button(GTK_HEADER_BAR(header_bar), FALSE);
  gtk_widget_set_size_request(header_bar, 0, 0);
  gtk_window_set_titlebar(GTK_WINDOW(probe_window), header_bar);

  g_autoptr(FlDartProject) project = fl_dart_project_new();
  fl_dart_project_set_assets_path(project, argv[1]);
  fl_dart_project_set_icu_data_path(project, argv[2]);
  FlView* view = fl_view_new(project);
  gtk_widget_show(GTK_WIDGET(view));
  gtk_container_add(GTK_CONTAINER(probe_window), GTK_WIDGET(view));
  std::fprintf(stderr, "PROBE show=0\n");
  gtk_widget_show(probe_window);
  if (argc < 4 || std::strcmp(argv[3], "--baseline") != 0) {
    g_signal_connect_object(probe_window, "map", G_CALLBACK(window_mapped_cb),
                            view, G_CONNECT_AFTER);
  }
  g_timeout_add(1500, advance, nullptr);
  gtk_main();
  return 0;
}

#ifndef ELSEWHEN_WINDOW_RESTORE_H_
#define ELSEWHEN_WINDOW_RESTORE_H_

#include <gtk/gtk.h>

inline void queue_view_allocation(GtkWidget* widget, gpointer) {
  if (GTK_IS_CONTAINER(widget)) {
    gtk_container_forall(GTK_CONTAINER(widget), queue_view_allocation, nullptr);
  }
  gtk_widget_queue_allocate(widget);
}

inline void window_mapped_cb(GtkWidget*, GtkWidget* view) {
  queue_view_allocation(view, nullptr);
}

#endif

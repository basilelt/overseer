#include <gtk/gtk.h>

static char **uris;

static void on_get(GtkWidget *w, GdkDragContext *c, GtkSelectionData *d, guint info, guint t, gpointer u) {
  gtk_selection_data_set_uris(d, uris);
}

static void on_end(GtkWidget *w, GdkDragContext *c, gpointer u) {
  gtk_main_quit();
}

int main(int argc, char **argv) {
  gtk_init(&argc, &argv);
  if (argc < 2) {
    g_printerr("usage: dragsrc FILE...\n");
    return 2;
  }
  uris = g_new0(char *, argc);
  for (int i = 1; i < argc; i++) uris[i - 1] = g_filename_to_uri(argv[i], NULL, NULL);
  GtkWidget *win = gtk_window_new(GTK_WINDOW_POPUP);
  gtk_window_move(GTK_WINDOW(win), 0, 0);
  gtk_window_set_default_size(GTK_WINDOW(win), 80, 80);
  GtkWidget *box = gtk_event_box_new();
  gtk_container_add(GTK_CONTAINER(box), gtk_label_new("drag"));
  gtk_container_add(GTK_CONTAINER(win), box);
  gtk_drag_source_set(box, GDK_BUTTON1_MASK, NULL, 0, GDK_ACTION_COPY);
  gtk_drag_source_add_uri_targets(box);
  g_signal_connect(box, "drag-data-get", G_CALLBACK(on_get), NULL);
  g_signal_connect(box, "drag-end", G_CALLBACK(on_end), NULL);
  gtk_widget_show_all(win);
  gtk_main();
  return 0;
}

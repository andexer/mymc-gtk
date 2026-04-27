mod bridge;

use std::cell::RefCell;
use std::rc::Rc;
use std::thread;

use gio::ListStore;
use glib::BoxedAnyObject;
use gtk::prelude::*;
use gtk::{gio, glib};

#[derive(Clone, Debug)]
enum UiEvent {
    Busy(String),
    Loaded {
        entries: Vec<bridge::SaveEntry>,
        free_space: i64,
    },
    Info(String),
    Error(String),
}

fn fill_store(store: &ListStore, entries: &[bridge::SaveEntry]) {
    store.remove_all();
    for entry in entries {
        store.append(&BoxedAnyObject::new(entry.clone()));
    }
}

fn selected_directory(selection: &gtk::SingleSelection) -> Option<String> {
    let obj = selection.selected_item()?;
    let boxed = obj.downcast::<BoxedAnyObject>().ok()?;
    let row = boxed.borrow::<bridge::SaveEntry>();
    Some(row.directory.clone())
}

fn build_text_column(
    title: &str,
    map_fn: impl Fn(&bridge::SaveEntry) -> String + Send + Sync + 'static,
) -> gtk::ColumnViewColumn {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        item.downcast_ref::<gtk::ListItem>()
            .expect("ListItem")
            .set_child(Some(&label));
    });
    factory.connect_bind(move |_, item| {
        let list_item = item.downcast_ref::<gtk::ListItem>().expect("ListItem");
        let child = list_item
            .child()
            .and_downcast::<gtk::Label>()
            .expect("Label");
        let model_item = list_item
            .item()
            .and_downcast::<BoxedAnyObject>()
            .expect("BoxedAnyObject");
        let row = model_item.borrow::<bridge::SaveEntry>();
        child.set_text(&map_fn(&row));
    });

    let column = gtk::ColumnViewColumn::new(Some(title), Some(factory));
    column.set_resizable(true);
    column
}

fn refresh_card(sender: async_channel::Sender<UiEvent>, card_path: String) {
    thread::spawn(move || {
        let _ = sender.send_blocking(UiEvent::Busy(format!("Loading {card_path}...")));
        match (
            bridge::list_saves(&card_path),
            bridge::get_free_space(&card_path),
        ) {
            (Ok(entries), Ok(free_space)) => {
                let _ = sender.send_blocking(UiEvent::Loaded {
                    entries,
                    free_space,
                });
            }
            (Err(err), _) | (_, Err(err)) => {
                let _ = sender.send_blocking(UiEvent::Error(err.to_string()));
            }
        }
    });
}

fn build_ui(app: &gtk::Application) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("mymc GTK")
        .default_width(980)
        .default_height(640)
        .build();

    let header = gtk::HeaderBar::new();
    header.set_title_widget(Some(&gtk::Label::new(Some("mymc"))));

    let open_btn = gtk::Button::with_label("Open");
    let import_btn = gtk::Button::with_label("Import");
    let export_btn = gtk::Button::with_label("Export");
    let delete_btn = gtk::Button::with_label("Delete");

    import_btn.set_sensitive(false);
    export_btn.set_sensitive(false);
    delete_btn.set_sensitive(false);

    header.pack_start(&open_btn);
    header.pack_start(&import_btn);
    header.pack_start(&export_btn);
    header.pack_start(&delete_btn);
    window.set_titlebar(Some(&header));

    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_margin_top(8);
    root.set_margin_bottom(8);
    root.set_margin_start(8);
    root.set_margin_end(8);

    let store = ListStore::new::<BoxedAnyObject>();
    let selection = gtk::SingleSelection::new(Some(store.clone()));
    selection.set_can_unselect(true);
    let view = gtk::ColumnView::new(Some(selection.clone()));
    view.set_vexpand(true);
    view.set_hexpand(true);

    let col_directory = build_text_column("Directory", |e| e.directory.clone());
    let col_size = build_text_column("Size", |e| format!("{} KB", e.size / 1024));
    let col_modified = build_text_column("Modified", |e| e.modified.to_string());
    let col_description = build_text_column("Description", |e| e.description.clone());
    let col_protection = build_text_column("Protection", |e| e.protection.clone());
    view.append_column(&col_directory);
    view.append_column(&col_size);
    view.append_column(&col_modified);
    view.append_column(&col_description);
    view.append_column(&col_protection);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_vexpand(true);
    scroll.set_hexpand(true);
    scroll.set_child(Some(&view));
    root.append(&scroll);

    let status = gtk::Label::new(Some("Open a memory card image."));
    status.set_xalign(0.0);
    root.append(&status);

    window.set_child(Some(&root));

    let current_card = Rc::new(RefCell::new(None::<String>));
    // glib 0.21 removed MainContext::channel/glib::Sender; use async_channel instead
    let (sender, receiver) = async_channel::unbounded::<UiEvent>();

    {
        let store = store.clone();
        let status = status.clone();
        let receiver = receiver.clone();
        glib::spawn_future_local(async move {
            while let Ok(event) = receiver.recv().await {
                match event {
                    UiEvent::Busy(msg) => status.set_text(&msg),
                    UiEvent::Loaded {
                        entries,
                        free_space,
                    } => {
                        fill_store(&store, &entries);
                        status.set_text(&format!("Free space: {} bytes", free_space));
                    }
                    UiEvent::Info(msg) => status.set_text(&msg),
                    UiEvent::Error(msg) => status.set_text(&format!("Error: {msg}")),
                }
            }
        });
    }

    {
        let selection = selection.clone();
        let current_card = current_card.clone();
        let export_btn = export_btn.clone();
        let delete_btn = delete_btn.clone();
        selection.clone().connect_selected_notify(move |_| {
            let has_card = current_card.borrow().is_some();
            let has_selection = selected_directory(&selection).is_some();
            export_btn.set_sensitive(has_card && has_selection);
            delete_btn.set_sensitive(has_card && has_selection);
        });
    }

    {
        let window = window.clone();
        let sender = sender.clone();
        let current_card = current_card.clone();
        let import_btn = import_btn.clone();
        let export_btn = export_btn.clone();
        let delete_btn = delete_btn.clone();
        let selection = selection.clone();
        open_btn.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::builder()
                .title("Open Memory Card Image")
                .build();
            let sender = sender.clone();
            let current_card = current_card.clone();
            let import_btn = import_btn.clone();
            let export_btn = export_btn.clone();
            let delete_btn = delete_btn.clone();
            let selection = selection.clone();
            dialog.open(
                Some(&window),
                None::<&gio::Cancellable>,
                move |result| match result {
                    Ok(file) => {
                        if let Some(path) = file.path() {
                            let path_str = path.to_string_lossy().to_string();
                            *current_card.borrow_mut() = Some(path_str.clone());
                            import_btn.set_sensitive(true);
                            let has_selection = selected_directory(&selection).is_some();
                            export_btn.set_sensitive(has_selection);
                            delete_btn.set_sensitive(has_selection);
                            refresh_card(sender.clone(), path_str);
                        }
                    }
                    Err(err) => {
                        let _ = sender.send_blocking(UiEvent::Error(err.to_string()));
                    }
                },
            );
        });
    }

    {
        let window = window.clone();
        let sender = sender.clone();
        let current_card = current_card.clone();
        import_btn.connect_clicked(move |_| {
            let card = match current_card.borrow().clone() {
                Some(c) => c,
                None => {
                    let _ = sender.send_blocking(UiEvent::Info("Open a card first.".into()));
                    return;
                }
            };
            let dialog = gtk::FileDialog::builder().title("Import Save File").build();
            let sender = sender.clone();
            dialog.open(
                Some(&window),
                None::<&gio::Cancellable>,
                move |result| match result {
                    Ok(file) => {
                        if let Some(path) = file.path() {
                            let save_path = path.to_string_lossy().to_string();
                            let sender = sender.clone();
                            let card = card.clone();
                            thread::spawn(move || {
                                let _ = sender.send_blocking(UiEvent::Busy("Importing save...".into()));
                                match bridge::import_save(&card, &save_path, None, false) {
                                    Ok(()) => {
                                        let _ =
                                            sender.send_blocking(UiEvent::Info("Import completed.".into()));
                                        refresh_card(sender, card);
                                    }
                                    Err(err) => {
                                        let _ = sender.send_blocking(UiEvent::Error(err.to_string()));
                                    }
                                }
                            });
                        }
                    }
                    Err(err) => {
                        let _ = sender.send_blocking(UiEvent::Error(err.to_string()));
                    }
                },
            );
        });
    }

    {
        let window = window.clone();
        let sender = sender.clone();
        let current_card = current_card.clone();
        let selection = selection.clone();
        export_btn.connect_clicked(move |_| {
            let card = match current_card.borrow().clone() {
                Some(c) => c,
                None => return,
            };
            let dirname = match selected_directory(&selection) {
                Some(d) => d,
                None => {
                    let _ = sender.send_blocking(UiEvent::Info("Select a save first.".into()));
                    return;
                }
            };
            let dialog = gtk::FileDialog::builder()
                .title("Export Save")
                .initial_name(format!("{dirname}.psu"))
                .build();
            let sender = sender.clone();
            dialog.save(
                Some(&window),
                None::<&gio::Cancellable>,
                move |result| match result {
                    Ok(file) => {
                        if let Some(path) = file.path() {
                            let output = path.to_string_lossy().to_string();
                            let sender = sender.clone();
                            let card = card.clone();
                            let dirname = dirname.clone();
                            thread::spawn(move || {
                                let _ = sender.send_blocking(UiEvent::Busy("Exporting save...".into()));
                                match bridge::export_save(&card, &dirname, Some(&output), "psu") {
                                    Ok(path) => {
                                        let _ = sender
                                            .send_blocking(UiEvent::Info(format!("Exported to {path}")));
                                    }
                                    Err(err) => {
                                        let _ = sender.send_blocking(UiEvent::Error(err.to_string()));
                                    }
                                }
                            });
                        }
                    }
                    Err(err) => {
                        let _ = sender.send_blocking(UiEvent::Error(err.to_string()));
                    }
                },
            );
        });
    }

    {
        let sender = sender.clone();
        let current_card = current_card.clone();
        let selection = selection.clone();
        delete_btn.connect_clicked(move |_| {
            let card = match current_card.borrow().clone() {
                Some(c) => c,
                None => return,
            };
            let dirname = match selected_directory(&selection) {
                Some(d) => d,
                None => {
                    let _ = sender.send_blocking(UiEvent::Info("Select a save first.".into()));
                    return;
                }
            };
            let sender = sender.clone();
            thread::spawn(move || {
                let _ = sender.send_blocking(UiEvent::Busy(format!("Deleting {dirname}...")));
                match bridge::delete_save(&card, &dirname) {
                    Ok(()) => {
                        let _ = sender.send_blocking(UiEvent::Info("Delete completed.".into()));
                        refresh_card(sender, card);
                    }
                    Err(err) => {
                        let _ = sender.send_blocking(UiEvent::Error(err.to_string()));
                    }
                }
            });
        });
    }

    window.present();
}

fn main() {
    // pyo3 0.28 removed prepare_freethreaded_python();
    // Python is initialized on first Python::attach() call.
    let app = gtk::Application::builder()
        .application_id("com.mymc.gtk")
        .build();
    app.connect_activate(build_ui);
    app.run();
}

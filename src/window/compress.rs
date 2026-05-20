use super::ui::{
    clear_box, file_subtitle, open_pdf_file, pdf_file_row, save_pdf_file,
    single_file_preview_widget,
};
use super::workspace::{open_output, parent_window, show_toast, update_shell_view_mode};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

mod imp {
    use super::super::state::CompressState;
    use super::*;
    use std::cell::Cell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/compress-workspace.ui")]
    pub struct CompressWorkspace {
        #[template_child]
        pub compress_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_detail_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub compress_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub compress_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub compress_preview_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub compress_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub compress_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_open_output_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_prune_row: TemplateChild<adw::SwitchRow>,
        #[template_child]
        pub compress_empty_streams_row: TemplateChild<adw::SwitchRow>,

        pub compress: CompressState,
        pub is_running: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for CompressWorkspace {
        const NAME: &'static str = "CompressWorkspace";
        type Type = super::CompressWorkspace;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for CompressWorkspace {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_callbacks();
            obj.update_view();
        }
    }
    impl WidgetImpl for CompressWorkspace {}
    impl BoxImpl for CompressWorkspace {}
}

glib::wrapper! {
    pub struct CompressWorkspace(ObjectSubclass<imp::CompressWorkspace>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl CompressWorkspace {
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let workspace = self.clone();
        imp.compress_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.compress_empty_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.compress_save_button.connect_clicked(move |_| {
            workspace.choose_output_file();
        });

        let workspace = self.clone();
        imp.compress_open_output_button.connect_clicked(move |_| {
            workspace.open_last_output();
        });
    }

    fn choose_file(&self) {
        let Some(parent) = parent_window(self) else {
            return;
        };
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = open_pdf_file(&parent, &gettext("Open PDF"), &gettext("Open")).await
            {
                workspace.load_pdf(path);
            }
        });
    }

    fn choose_output_file(&self) {
        let Some(input_file) = self.imp().compress.input_file() else {
            return;
        };
        let Some(parent) = parent_window(self) else {
            return;
        };

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = save_pdf_file(
                &parent,
                &gettext("Save Compressed PDF"),
                &gettext("Compress"),
                "Compressed.pdf",
            )
            .await
            {
                workspace.compress_to(input_file, path);
            }
        });
    }

    fn load_pdf(&self, path: PathBuf) {
        let imp = self.imp();
        imp.compress.begin_loading();
        self.update_view();

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::preview::render_single_file_preview(path.clone()).await;
            let imp = workspace.imp();

            match result {
                Ok(preview) => {
                    imp.compress.finish_loading(path, preview);
                }
                Err(error) => {
                    imp.compress.finish_loading_failed();
                    show_toast(&workspace, &error.to_string());
                }
            }

            workspace.update_view();
        });
    }

    fn compress_to(&self, input_file: PathBuf, output_file: PathBuf) {
        let imp = self.imp();
        let options = crate::pdf::CompressOptions {
            remove_empty_streams: imp.compress_empty_streams_row.is_active(),
            prune_objects: imp.compress_prune_row.is_active(),
        };

        imp.is_running.set(true);
        imp.compress.clear_last_output();
        self.update_view();

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            let result = crate::pdf::compress_pdf(input_file, output_file, options).await;
            let imp = workspace.imp();
            imp.is_running.set(false);

            match result {
                Ok(path) => {
                    imp.compress.set_last_output(path);
                    show_toast(&workspace, &gettext("Compressed PDF saved"));
                }
                Err(error) => {
                    show_toast(&workspace, &error.to_string());
                }
            }

            workspace.update_view();
        });
    }

    pub(super) fn supports_view_mode(&self) -> bool {
        false
    }

    pub(super) fn has_view_mode_content(&self) -> bool {
        false
    }

    pub(super) fn set_view_mode(&self, _view_mode: super::ViewMode) {}

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let file = imp.compress.file.borrow();
        let has_file = file.is_some();
        let is_busy = imp.compress.is_busy(imp.is_running.get());
        let preview = imp.compress.preview.borrow();

        imp.compress_file_list.remove_all();
        clear_box(&imp.compress_preview_box);
        if let Some(path) = file.as_ref() {
            imp.compress_file_list
                .append(&pdf_file_row(path, file_subtitle(path)));
            imp.compress_preview_box
                .append(&single_file_preview_widget(preview.as_ref()));
        }

        imp.compress_empty_status.set_visible(!has_file);
        imp.compress_content.set_visible(has_file);
        imp.compress_choose_button.set_visible(has_file);
        imp.compress_save_button.set_visible(has_file);
        imp.compress_open_output_button
            .set_visible(imp.compress.last_output.borrow().is_some());

        imp.compress_choose_button.set_sensitive(!is_busy);
        imp.compress_empty_choose_button.set_sensitive(!is_busy);
        imp.compress_save_button.set_sensitive(has_file && !is_busy);
        imp.compress_open_output_button
            .set_sensitive(imp.compress.last_output.borrow().is_some() && !is_busy);
        imp.compress_prune_row.set_sensitive(has_file && !is_busy);
        imp.compress_empty_streams_row
            .set_sensitive(has_file && !is_busy);

        let detail = if imp.is_running.get() {
            gettext("Compressing PDF...")
        } else if imp.compress.is_loading.get() {
            gettext("Loading PDF...")
        } else if let Some(path) = file.as_ref() {
            file_subtitle(path)
        } else {
            gettext("No PDF selected")
        };
        imp.compress_detail_label.set_label(&detail);
        update_shell_view_mode(self);
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().compress.last_output.borrow().as_ref() {
            open_output(self, path);
        }
    }
}

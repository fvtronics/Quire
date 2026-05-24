use super::ui::{
    clear_box, file_subtitle, open_pdf_file, pdf_file_row, save_pdf_file,
    single_file_preview_widget,
};
use super::workspace::{
    load_single_processable_pdf, open_output, output_option_callback, parent_window,
    run_output_job, setup_advanced_options_menu, update_shell_title, update_shell_view_mode,
    AdvancedOptionsMenu, SinglePdfLoadHandlers,
};
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::glib;
use std::path::PathBuf;

mod imp {
    use super::super::state::CompressState;
    use adw::subclass::prelude::*;
    use gtk::{glib, TemplateChild};
    use std::cell::Cell;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/folios/compress-workspace.ui")]
    pub struct CompressWorkspace {
        #[template_child]
        pub compress_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub compress_actions: TemplateChild<gtk::Box>,
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
        pub compress_advanced_options_button: TemplateChild<gtk::MenuButton>,
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

        let modern_pdf = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().compress.options.set_modern_pdf(active),
            |workspace| workspace.imp().compress.job.clear_last_output(),
            Self::update_view,
        );
        let remove_metadata = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().compress.options.set_remove_metadata(active),
            |workspace| workspace.imp().compress.job.clear_last_output(),
            Self::update_view,
        );
        setup_advanced_options_menu(
            &imp.compress_advanced_options_button,
            &imp.compress.options,
            AdvancedOptionsMenu::new(modern_pdf, remove_metadata),
        );

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
                workspace.load_pdf(path, parent);
            }
        });
    }

    fn choose_output_file(&self) {
        let Some((input_file, password)) = self.imp().compress.input_file() else {
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
                workspace.compress_to(input_file, password, path);
            }
        });
    }

    fn load_pdf(&self, path: PathBuf, parent: gtk::Window) {
        load_single_processable_pdf(
            self.clone(),
            parent,
            path,
            crate::preview::render_single_file_preview,
            SinglePdfLoadHandlers {
                begin_loading: |workspace: &Self| workspace.imp().compress.job.begin_loading(),
                store_loaded: |workspace: &Self, path, password, preview| {
                    workspace
                        .imp()
                        .compress
                        .finish_loading(path, password, preview);
                },
                finish_loading_failed: |workspace: &Self| {
                    workspace.imp().compress.job.finish_loading_failed();
                },
                refresh: Self::update_view,
            },
        );
    }

    fn compress_to(&self, input_file: PathBuf, password: Option<String>, output_file: PathBuf) {
        let imp = self.imp();
        let options = crate::pdf::CompressOptions {
            remove_empty_streams: imp.compress_empty_streams_row.is_active(),
            prune_objects: imp.compress_prune_row.is_active(),
            save: imp.compress.options.options(),
        };

        run_output_job(
            self.clone(),
            crate::pdf::compress_pdf(input_file, password, output_file, options),
            gettext("Compressed PDF saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().compress.job.clear_last_output(),
            |workspace, path| workspace.imp().compress.job.set_last_output(path),
            Self::update_view,
        );
    }

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let file = imp.compress.file.borrow();
        let has_file = file.is_some();
        let is_busy = imp.compress.job.is_busy(imp.is_running.get());
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
        imp.compress_actions.set_visible(has_file);
        imp.compress_content.set_visible(has_file);
        imp.compress_choose_button.set_visible(has_file);
        imp.compress_save_button.set_visible(has_file);
        imp.compress_advanced_options_button.set_visible(has_file);
        imp.compress_open_output_button
            .set_visible(imp.compress.job.has_last_output());

        imp.compress_choose_button.set_sensitive(!is_busy);
        imp.compress_empty_choose_button.set_sensitive(!is_busy);
        imp.compress_save_button.set_sensitive(has_file && !is_busy);
        imp.compress_open_output_button
            .set_sensitive(imp.compress.job.has_last_output() && !is_busy);
        imp.compress_advanced_options_button
            .set_sensitive(has_file && !is_busy);
        imp.compress_prune_row.set_sensitive(has_file && !is_busy);
        imp.compress_empty_streams_row
            .set_sensitive(has_file && !is_busy);

        let detail = if imp.is_running.get() {
            gettext("Compressing PDF...")
        } else if imp.compress.job.is_loading() {
            gettext("Loading PDF...")
        } else if let Some(path) = file.as_ref() {
            file_subtitle(path)
        } else {
            gettext("No PDF selected")
        };
        update_shell_title(self, &gettext("Compress PDF"), &detail);
        update_shell_view_mode(self);
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().compress.job.last_output() {
            open_output(self, &path);
        }
    }
}

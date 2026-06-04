use super::ui::{
    clear_box, connect_delayed_entry_validation, file_subtitle, file_title, open_image_file,
    open_pdf_file, page_count_label, page_ranges_error_message, pdf_file_row, save_pdf_file,
    single_file_preview_widget, DelayedEntryValidationState, EntryValidation,
};
use super::workspace::{
    load_single_processable_pdf, open_output, output_option_callback, parent_window,
    run_output_job, setup_advanced_options_menu, setup_compact_workspace_margins,
    setup_default_height_breakpoint, setup_default_width_breakpoint,
    setup_vertical_layout_breakpoint, show_toast, update_shell_title, update_shell_view_mode,
    AdvancedOptionsMenu, SinglePdfLoadHandlers,
};
use super::PdfTool;
use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::gdk_pixbuf::{InterpType, Pixbuf};
use gtk::glib;
use std::path::{Path, PathBuf};

const WATERMARK_LAYER_BACKGROUND: u32 = 1;
const WATERMARK_PAGES_ALL: u32 = 0;
const WATERMARK_PAGES_FIRST: u32 = 1;
const WATERMARK_PAGES_LAST: u32 = 2;
const WATERMARK_PAGES_SPECIFIC: u32 = 3;
const WATERMARK_PREVIEW_MARGIN_RATIO: f64 = 0.08;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WatermarkPageMode {
    AllPages,
    FirstPage,
    LastPage,
    SpecificPages,
}

impl WatermarkPageMode {
    fn from_index(index: u32) -> Option<Self> {
        match index {
            WATERMARK_PAGES_ALL => Some(Self::AllPages),
            WATERMARK_PAGES_FIRST => Some(Self::FirstPage),
            WATERMARK_PAGES_LAST => Some(Self::LastPage),
            WATERMARK_PAGES_SPECIFIC => Some(Self::SpecificPages),
            _ => None,
        }
    }
}

mod imp {
    use super::super::state::WatermarkState;
    use super::{DelayedEntryValidationState, Pixbuf};
    use adw::subclass::prelude::*;
    use gtk::{glib, TemplateChild};
    use std::cell::{Cell, RefCell};
    use std::rc::Rc;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/fvtronics/Quire/watermark-workspace.ui")]
    pub struct WatermarkWorkspace {
        #[template_child]
        pub watermark_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub watermark_empty_choose_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub watermark_actions: TemplateChild<adw::WrapBox>,
        #[template_child]
        pub watermark_empty_status: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub watermark_content: TemplateChild<gtk::Box>,
        #[template_child]
        pub watermark_preview_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub watermark_file_list: TemplateChild<gtk::ListBox>,
        #[template_child]
        pub watermark_image_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub watermark_layer_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub watermark_opacity_row: TemplateChild<adw::SpinRow>,
        #[template_child]
        pub watermark_pages_row: TemplateChild<adw::ComboRow>,
        #[template_child]
        pub watermark_specific_pages_entry: TemplateChild<adw::EntryRow>,
        #[template_child]
        pub watermark_specific_pages_error_row: TemplateChild<gtk::ListBoxRow>,
        #[template_child]
        pub watermark_specific_pages_error_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub watermark_save_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub watermark_open_output_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub watermark_advanced_options_button: TemplateChild<gtk::MenuButton>,

        pub watermark: WatermarkState,
        pub image_pixbuf: RefCell<Option<Pixbuf>>,
        pub is_running: Cell<bool>,
        pub(super) specific_pages_validation: Rc<DelayedEntryValidationState>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for WatermarkWorkspace {
        const NAME: &'static str = "WatermarkWorkspace";
        type Type = super::WatermarkWorkspace;
        type ParentType = gtk::Box;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for WatermarkWorkspace {
        fn constructed(&self) {
            self.parent_constructed();
            self.watermark_opacity_row.configure(
                Some(&gtk::Adjustment::new(100.0, 10.0, 100.0, 5.0, 10.0, 0.0)),
                1.0,
                0,
            );
            self.watermark_opacity_row
                .set_update_policy(gtk::SpinButtonUpdatePolicy::IfValid);
            self.watermark_opacity_row.set_numeric(true);
            let obj = self.obj();
            obj.setup_callbacks();
            obj.update_view();
        }
    }
    impl WidgetImpl for WatermarkWorkspace {}
    impl BoxImpl for WatermarkWorkspace {}
}

glib::wrapper! {
    pub struct WatermarkWorkspace(ObjectSubclass<imp::WatermarkWorkspace>)
        @extends gtk::Widget, gtk::Box,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Orientable;
}

impl WatermarkWorkspace {
    fn setup_callbacks(&self) {
        let imp = self.imp();

        let layer_options = [gettext("Foreground"), gettext("Background")];
        let layer_options = layer_options.iter().map(String::as_str).collect::<Vec<_>>();
        imp.watermark_layer_row
            .set_model(Some(&gtk::StringList::new(&layer_options)));
        imp.watermark_layer_row
            .set_expression(Some(gtk::PropertyExpression::new(
                gtk::StringObject::static_type(),
                gtk::Expression::NONE,
                "string",
            )));

        let pages_options = [
            gettext("All Pages"),
            gettext("First Page"),
            gettext("Last Page"),
            gettext("Specific Pages"),
        ];
        let pages_options = pages_options.iter().map(String::as_str).collect::<Vec<_>>();
        imp.watermark_pages_row
            .set_model(Some(&gtk::StringList::new(&pages_options)));
        imp.watermark_pages_row
            .set_expression(Some(gtk::PropertyExpression::new(
                gtk::StringObject::static_type(),
                gtk::Expression::NONE,
                "string",
            )));

        let modern_pdf = output_option_callback(
            self.clone(),
            |workspace, active| workspace.imp().watermark.options.set_modern_pdf(active),
            |workspace| workspace.imp().watermark.job.clear_last_output(),
            Self::update_view,
        );
        let remove_metadata = output_option_callback(
            self.clone(),
            |workspace, active| {
                workspace
                    .imp()
                    .watermark
                    .options
                    .set_remove_metadata(active)
            },
            |workspace| workspace.imp().watermark.job.clear_last_output(),
            Self::update_view,
        );
        setup_advanced_options_menu(
            &imp.watermark_advanced_options_button,
            &imp.watermark.options,
            AdvancedOptionsMenu::new(modern_pdf, remove_metadata),
        );

        let workspace = self.clone();
        imp.watermark_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.watermark_empty_choose_button.connect_clicked(move |_| {
            workspace.choose_file();
        });

        let workspace = self.clone();
        imp.watermark_image_row.connect_activated(move |_| {
            workspace.choose_image_file();
        });

        let workspace = self.clone();
        imp.watermark_save_button.connect_clicked(move |_| {
            workspace.choose_output_file();
        });

        let workspace = self.clone();
        imp.watermark_open_output_button.connect_clicked(move |_| {
            workspace.open_last_output();
        });

        let workspace = self.clone();
        imp.watermark_layer_row.connect_selected_notify(move |_| {
            workspace.imp().watermark.job.clear_last_output();
            workspace.update_view();
        });

        let workspace = self.clone();
        imp.watermark_opacity_row.connect_value_notify(move |_| {
            workspace.imp().watermark.job.clear_last_output();
            workspace.update_view();
        });

        let workspace = self.clone();
        imp.watermark_pages_row.connect_selected_notify(move |_| {
            workspace.imp().watermark.job.clear_last_output();
            workspace.update_view();
        });

        let workspace = self.clone();
        let specific_pages_changed = move || {
            workspace.imp().watermark.job.clear_last_output();
            workspace.update_view();
        };

        let workspace = self.clone();
        let refresh_specific_pages_validation = move || {
            workspace.update_view();
        };
        connect_delayed_entry_validation(
            &imp.watermark_specific_pages_entry,
            imp.specific_pages_validation.clone(),
            specific_pages_changed,
            refresh_specific_pages_validation,
        );
    }

    pub(super) fn setup_responsive_layout(&self, breakpoint: &adw::Breakpoint) {
        let imp = self.imp();
        setup_compact_workspace_margins(breakpoint, self);
        setup_vertical_layout_breakpoint(breakpoint, &imp.watermark_content);
        setup_default_width_breakpoint(breakpoint, &*imp.watermark_preview_box);
        setup_default_height_breakpoint(breakpoint, &*imp.watermark_preview_box);
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

    fn choose_image_file(&self) {
        let Some(parent) = parent_window(self) else {
            return;
        };
        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = open_image_file(
                &parent,
                &gettext("Choose Watermark Image"),
                &gettext("Choose"),
            )
            .await
            {
                let pixbuf = match Pixbuf::from_file(&path) {
                    Ok(pixbuf) => pixbuf,
                    Err(error) => {
                        eprintln!("Watermark image preview error: {error}");
                        show_toast(&workspace, &gettext("Could not open image"));
                        return;
                    }
                };

                workspace.imp().image_pixbuf.borrow_mut().replace(pixbuf);
                workspace.imp().watermark.set_image_file(path);
                workspace.update_view();
            }
        });
    }

    fn choose_output_file(&self) {
        let imp = self.imp();
        let Some((input_file, password)) = imp.watermark.input_file() else {
            return;
        };
        let Some(image_file) = imp.watermark.image_file() else {
            return;
        };
        let Ok(target) = self.watermark_target() else {
            return;
        };
        let Some(parent) = parent_window(self) else {
            return;
        };
        let options = crate::pdf::WatermarkOptions {
            image_file,
            layer: self.watermark_layer(),
            target,
            opacity: self.watermark_opacity(),
            save: imp.watermark.options.options(),
        };

        let workspace = self.clone();
        glib::spawn_future_local(async move {
            if let Some(path) = save_pdf_file(
                &parent,
                &gettext("Save Watermarked PDF"),
                &gettext("Add Watermark"),
                "Watermarked.pdf",
            )
            .await
            {
                workspace.watermark_to(input_file, password, path, options);
            }
        });
    }

    fn load_pdf(&self, path: PathBuf, parent: gtk::Window) {
        load_single_processable_pdf(
            self.clone(),
            parent,
            path,
            crate::preview::render_first_page_preview_with_count,
            SinglePdfLoadHandlers {
                begin_loading: |workspace: &Self| workspace.imp().watermark.job.begin_loading(),
                store_loaded: |workspace: &Self, path, password, (preview, page_count)| {
                    workspace
                        .imp()
                        .watermark
                        .finish_loading(path, password, preview, page_count);
                },
                finish_loading_failed: |workspace: &Self| {
                    workspace.imp().watermark.job.finish_loading_failed();
                },
                refresh: Self::update_view,
            },
        );
    }

    fn watermark_to(
        &self,
        input_file: PathBuf,
        password: Option<String>,
        output_file: PathBuf,
        options: crate::pdf::WatermarkOptions,
    ) {
        run_output_job(
            self.clone(),
            crate::pdf::watermark_pdf(input_file, password, output_file, options),
            gettext("Watermarked PDF saved"),
            |workspace, running| workspace.imp().is_running.set(running),
            |workspace| workspace.imp().watermark.job.clear_last_output(),
            |workspace, path| workspace.imp().watermark.job.set_last_output(path),
            Self::update_view,
        );
    }

    pub(super) fn update_view(&self) {
        let imp = self.imp();
        let file = imp.watermark.file.borrow();
        let image_file = imp.watermark.image_file.borrow();
        let image_pixbuf = imp.image_pixbuf.borrow();
        let has_file = file.is_some();
        let has_image = image_file.is_some();
        let is_busy = imp.watermark.job.is_busy(imp.is_running.get());
        let page_mode = WatermarkPageMode::from_index(imp.watermark_pages_row.selected());
        let target = self.watermark_target();
        let has_target = target.is_ok();
        let has_specific_pages_input = !imp.watermark_specific_pages_entry.text().trim().is_empty();
        let specific_pages_error = has_file
            && page_mode == Some(WatermarkPageMode::SpecificPages)
            && has_specific_pages_input
            && target.is_err();
        let preview = imp.watermark.preview.borrow();

        imp.watermark_file_list.remove_all();
        clear_box(&imp.watermark_preview_box);
        if let Some(path) = file.as_ref() {
            imp.watermark_file_list
                .append(&self.watermark_file_row(path, imp.watermark.page_count.get()));
            imp.watermark_preview_box.append(&watermark_preview_widget(
                preview.as_ref(),
                image_pixbuf.as_ref(),
                self.watermark_opacity(),
            ));
        }

        let image_subtitle = image_file
            .as_deref()
            .map(file_title)
            .map(str::to_string)
            .unwrap_or_else(|| gettext("No image selected"));
        imp.watermark_image_row.set_subtitle(&image_subtitle);
        imp.watermark_empty_status.set_visible(!has_file);
        imp.watermark_actions.set_visible(has_file);
        imp.watermark_content.set_visible(has_file);
        imp.watermark_choose_button.set_visible(has_file);
        imp.watermark_save_button.set_visible(has_file);
        imp.watermark_advanced_options_button.set_visible(has_file);
        imp.watermark_open_output_button
            .set_visible(imp.watermark.job.has_last_output());

        imp.watermark_choose_button.set_sensitive(!is_busy);
        imp.watermark_empty_choose_button.set_sensitive(!is_busy);
        imp.watermark_save_button
            .set_sensitive(has_file && has_image && has_target && !is_busy);
        imp.watermark_open_output_button
            .set_sensitive(imp.watermark.job.has_last_output() && !is_busy);
        imp.watermark_advanced_options_button
            .set_sensitive(has_file && !is_busy);
        imp.watermark_image_row.set_sensitive(has_file && !is_busy);
        imp.watermark_layer_row.set_sensitive(has_file && !is_busy);
        imp.watermark_opacity_row
            .set_sensitive(has_file && !is_busy);
        imp.watermark_pages_row.set_sensitive(has_file && !is_busy);
        imp.watermark_specific_pages_entry
            .set_visible(page_mode == Some(WatermarkPageMode::SpecificPages));
        imp.watermark_specific_pages_entry.set_sensitive(
            has_file && page_mode == Some(WatermarkPageMode::SpecificPages) && !is_busy,
        );
        EntryValidation::new(
            &imp.watermark_specific_pages_entry,
            &imp.watermark_specific_pages_error_row,
            &imp.watermark_specific_pages_error_label,
        )
        .set_error(
            imp.specific_pages_validation.display(specific_pages_error),
            &page_ranges_error_message(),
        );

        let detail = if imp.is_running.get() {
            gettext("Adding watermark...")
        } else if imp.watermark.job.is_loading() {
            gettext("Loading PDF...")
        } else if has_file {
            page_count_label(imp.watermark.page_count.get())
        } else {
            gettext("No PDF selected")
        };
        update_shell_title(self, PdfTool::Watermark, &detail);
        update_shell_view_mode(self);
    }

    fn watermark_layer(&self) -> crate::pdf::WatermarkLayer {
        match self.imp().watermark_layer_row.selected() {
            WATERMARK_LAYER_BACKGROUND => crate::pdf::WatermarkLayer::Background,
            _ => crate::pdf::WatermarkLayer::Foreground,
        }
    }

    fn watermark_opacity(&self) -> f32 {
        (self.imp().watermark_opacity_row.value() / 100.0).clamp(0.1, 1.0) as f32
    }

    fn watermark_target(&self) -> Result<crate::pdf::WatermarkTarget, crate::pdf::PdfBackendError> {
        let imp = self.imp();
        match WatermarkPageMode::from_index(imp.watermark_pages_row.selected()) {
            Some(WatermarkPageMode::AllPages) => Ok(crate::pdf::WatermarkTarget::AllPages),
            Some(WatermarkPageMode::FirstPage) => Ok(crate::pdf::WatermarkTarget::FirstPage),
            Some(WatermarkPageMode::LastPage) => Ok(crate::pdf::WatermarkTarget::LastPage),
            Some(WatermarkPageMode::SpecificPages) => {
                let pages = crate::pdf::parse_page_ranges(
                    imp.watermark_specific_pages_entry.text().as_str(),
                    imp.watermark.page_count.get(),
                )?;
                Ok(crate::pdf::WatermarkTarget::SpecificPages(pages))
            }
            None => Err(crate::pdf::PdfBackendError::InvalidPageRange(
                "Choose target pages.".to_string(),
            )),
        }
    }

    fn watermark_file_row(&self, path: &Path, page_count: usize) -> adw::ActionRow {
        pdf_file_row(
            path,
            format!("{} - {}", page_count_label(page_count), file_subtitle(path)),
        )
    }

    fn open_last_output(&self) {
        if let Some(path) = self.imp().watermark.job.last_output() {
            open_output(self, &path);
        }
    }
}

fn watermark_preview_widget(
    preview: Option<&crate::preview::PagePreview>,
    image: Option<&Pixbuf>,
    opacity: f32,
) -> gtk::Widget {
    match preview.and_then(|preview| watermarked_preview_widget(preview, image?, opacity)) {
        Some(preview) => preview.upcast(),
        None => single_file_preview_widget(preview),
    }
}

#[allow(deprecated)]
fn watermarked_preview_widget(
    preview: &crate::preview::PagePreview,
    watermark: &Pixbuf,
    opacity: f32,
) -> Option<gtk::Picture> {
    let page = Pixbuf::from_read(std::io::Cursor::new(preview.png_data.clone())).ok()?;
    let watermarked = page.copy()?;
    let (width, height, x, y) = watermark_preview_placement(&page, watermark)?;
    watermark.composite(
        &watermarked,
        x,
        y,
        width,
        height,
        x.into(),
        y.into(),
        width as f64 / watermark.width() as f64,
        height as f64 / watermark.height() as f64,
        InterpType::Bilinear,
        (opacity as f64 * 255.0).round() as i32,
    );

    let texture = gtk::gdk::Texture::for_pixbuf(&watermarked);
    let picture = gtk::Picture::for_paintable(&texture);
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    Some(picture)
}

fn watermark_preview_placement(page: &Pixbuf, watermark: &Pixbuf) -> Option<(i32, i32, i32, i32)> {
    if page.width() <= 0 || page.height() <= 0 || watermark.width() <= 0 || watermark.height() <= 0
    {
        return None;
    }

    let max_width = page.width() as f64 * (1.0 - WATERMARK_PREVIEW_MARGIN_RATIO * 2.0);
    let max_height = page.height() as f64 * (1.0 - WATERMARK_PREVIEW_MARGIN_RATIO * 2.0);
    let scale = (max_width / watermark.width() as f64).min(max_height / watermark.height() as f64);
    let width = (watermark.width() as f64 * scale).round().max(1.0) as i32;
    let height = (watermark.height() as f64 * scale).round().max(1.0) as i32;
    let x = (page.width() - width) / 2;
    let y = (page.height() - height) / 2;

    Some((width, height, x, y))
}

#[cfg(test)]
mod tests {
    use super::{
        watermark_preview_placement, WatermarkPageMode, WATERMARK_PAGES_ALL, WATERMARK_PAGES_FIRST,
        WATERMARK_PAGES_LAST, WATERMARK_PAGES_SPECIFIC,
    };
    use gtk::gdk_pixbuf::{Colorspace, Pixbuf};

    #[test]
    fn watermark_page_mode_maps_known_indices() {
        assert_eq!(
            WatermarkPageMode::from_index(WATERMARK_PAGES_ALL),
            Some(WatermarkPageMode::AllPages)
        );
        assert_eq!(
            WatermarkPageMode::from_index(WATERMARK_PAGES_FIRST),
            Some(WatermarkPageMode::FirstPage)
        );
        assert_eq!(
            WatermarkPageMode::from_index(WATERMARK_PAGES_LAST),
            Some(WatermarkPageMode::LastPage)
        );
        assert_eq!(
            WatermarkPageMode::from_index(WATERMARK_PAGES_SPECIFIC),
            Some(WatermarkPageMode::SpecificPages)
        );
    }

    #[test]
    fn watermark_page_mode_rejects_unknown_indices() {
        assert_eq!(WatermarkPageMode::from_index(99), None);
    }

    #[test]
    fn watermark_preview_placement_scales_to_page_preview() {
        let page = Pixbuf::new(Colorspace::Rgb, false, 8, 200, 283)
            .expect("test page pixbuf should be created");
        let watermark = Pixbuf::new(Colorspace::Rgb, false, 8, 200, 283)
            .expect("test watermark pixbuf should be created");

        let (width, height, x, y) =
            watermark_preview_placement(&page, &watermark).expect("placement should fit");

        assert_eq!((width, height), (168, 238));
        assert_eq!((x, y), (16, 22));
    }
}

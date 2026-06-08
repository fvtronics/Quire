/* application.rs
 *
 * Copyright 2026 Francisco Vásquez Cuevas
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */

use adw::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::VERSION;

mod imp {
    use crate::QuireWindow;
    use adw::prelude::*;
    use adw::subclass::prelude::*;
    use gtk::glib;

    #[derive(Debug, Default)]
    pub struct QuireApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for QuireApplication {
        const NAME: &'static str = "QuireApplication";
        type Type = super::QuireApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for QuireApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<control>q"]);
            obj.set_accels_for_action("app.shortcuts", &["<control>question"]);
        }
    }

    impl ApplicationImpl for QuireApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = QuireWindow::new(&*application);
                window.upcast()
            });

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for QuireApplication {}
    impl AdwApplicationImpl for QuireApplication {}
}

glib::wrapper! {
    pub struct QuireApplication(ObjectSubclass<imp::QuireApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl QuireApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .property("resource-base-path", "/com/fvtronics/Quire")
            .build()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        let shortcuts_action = gio::ActionEntry::builder("shortcuts")
            .activate(move |app: &Self, _, _| app.show_shortcuts())
            .build();
        self.add_action_entries([quit_action, about_action, shortcuts_action]);
    }

    fn show_shortcuts(&self) {
        let window = self.active_window().unwrap();
        let builder = gtk::Builder::from_resource("/com/fvtronics/Quire/shortcuts-dialog.ui");
        let dialog: adw::ShortcutsDialog = builder
            .object("shortcuts_dialog")
            .expect("shortcuts dialog resource should define shortcuts_dialog");

        dialog.present(Some(&window));
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("Quire")
            .application_icon("com.fvtronics.Quire")
            .developer_name("Francisco Vásquez Cuevas")
            .version(VERSION)
            .developers(vec!["Francisco Vásquez Cuevas"])
            .translator_credits(gettext("translator-credits"))
            .copyright("© 2026 Francisco Vásquez Cuevas")
            .website("https://fvtronics.com/en/projects/quire")
            .issue_url("https://codeberg.org/FVtronics/Quire/issues")
            .license_type(gtk::License::Gpl30)
            .build();

        about.present(Some(&window));
    }
}

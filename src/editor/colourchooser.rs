use gtk4::{
    gdk, glib,
    glib::{ObjectExt, ToValue},
    prelude::*,
    subclass::prelude::*,
    ResponseType,
};

use super::EditorWindow;
use crate::editor::Colour;

glib::wrapper! {
    pub struct ColourChooserWidget(ObjectSubclass<underlying::ColourChooserWidget>)
        @extends gtk4::Widget, gtk4::Box;
}

impl ColourChooserWidget {
    pub fn colour(&self) -> Colour {
        let rgba = self.imp().colour_wheel.get().unwrap().rgba();

        Colour {
            alpha: self.imp().alpha.get(),
            ..Colour::from_gdk_rgba(rgba)
        }
    }

    pub fn set_colour(&self, colour: Colour) {
        let rgba = gdk::RGBA::new(
            colour.red as f32 / 255.0,
            colour.green as f32 / 255.0,
            colour.blue as f32 / 255.0,
            1.0,
        );
        let imp = self.imp();

        imp.colour_wheel
            .get()
            .unwrap()
            .set_property_from_value("rgba", &rgba.to_value());
    }
}

impl Default for ColourChooserWidget {
    fn default() -> Self {
        glib::Object::new(&[])
    }
}

// NOTE: This struct doesn't need to participate in the gtk widget tree because its fields already do
// and it only wraps some functions of its fields.
pub struct Dialog {
    editor: glib::WeakRef<EditorWindow>,
    dialog: gtk4::Dialog,
    colour_chooser: ColourChooserWidget,
}

impl Dialog {
    pub fn show(&self) {
        self.dialog.show();
    }

    pub fn connect_response<F>(&self, func: F)
    where
        F: Fn(&EditorWindow, Colour) + 'static,
    {
        let editor = match self.editor.upgrade() {
            Some(editor) => editor,
            None => {
                tracing::warn!("Failed to upgrade self.editor in `Dialog::connect_response`");
                return;
            }
        };

        self.dialog.connect_response(glib::clone!(
            @weak self.colour_chooser as colour_chooser,
        => move |this, response| {
            if response == ResponseType::Ok {
                func(&editor, colour_chooser.colour());
                this.close();
            } else if response == PICKER_RESPONSE_ID {
                this.hide();

                // This branch is part of the mechanism that handles picking a colour from the image.
                // The actual retrieving a colour part is implemented directly in the editor's click
                // event handler, which checks the `is_picking_a_colour` field on the impl struct of
                // EditorWindow.
                // Once the colour is picked, the receive end of the channel will receive the colour
                // of the pixel the user clicked on, set the colour_chooser's colour to that, and show
                // the dialog again, as such eventually one of the other two branches of this `if` will
                // be reached.

                let (colour_tx, colour_rx) = glib::MainContext::channel(glib::PRIORITY_DEFAULT);

                editor.start_picking_a_colour(colour_tx);

                colour_rx.attach(None, glib::clone!(
                    @weak this
                => @default-return Continue(false), move |colour| {
                    colour_chooser.set_colour(colour);
                    this.show();
                    Continue(false)
                }));
            } else {
                this.close();
            }
        }));
    }
}

/// The Response ID used by the colour picker when a colour was picked from the image being edited
const PICKER_RESPONSE_ID: ResponseType = ResponseType::Other(123);

pub fn dialog(editor: &EditorWindow) -> Dialog {
    let colour_chooser = ColourChooserWidget::default();
    colour_chooser.set_margin_bottom(10);
    colour_chooser.set_margin_top(10);
    colour_chooser.set_margin_start(10);
    colour_chooser.set_margin_end(10);

    let dialog = gtk4::Dialog::with_buttons(
        Some("kcshot - Pick a colour"),
        Some(editor),
        gtk4::DialogFlags::MODAL | gtk4::DialogFlags::DESTROY_WITH_PARENT,
        &[],
    );
    dialog.set_resizable(false);

    let cancel_button = dialog.add_button("Cancel", ResponseType::Cancel);
    cancel_button.add_css_class("destructive-action");
    cancel_button.set_margin_bottom(10);
    cancel_button.set_margin_end(5);

    // Part of the "pick a colour from the image" mechanism, see big comment in `Dialog::connect_response`
    let colour_picker = gtk4::Button::new();
    colour_picker.set_child(Some(&gtk4::Image::from_resource(
        "/kc/kcshot/editor/tool-colourpicker.png",
    )));
    colour_picker.set_margin_bottom(10);
    colour_picker.set_tooltip_text(Some("Pick a colour from the image"));
    colour_picker.set_halign(gtk4::Align::Start);
    dialog.add_action_widget(&colour_picker, PICKER_RESPONSE_ID);

    let ok_button = dialog.add_button("OK", ResponseType::Ok);
    ok_button.add_css_class("suggested-action");
    ok_button.set_margin_start(5);
    ok_button.set_margin_end(10);
    ok_button.set_margin_bottom(10);

    dialog.content_area().append(&colour_chooser);

    Dialog {
        editor: editor.downgrade(),
        dialog,
        colour_chooser,
    }
}

mod underlying {
    use std::cell::Cell;

    use cairo::glib::{ParamSpec, Value};
    use gtk4::{gdk, gdk::prelude::*, glib, pango, prelude::*, subclass::prelude::*};
    use kcshot_data::colour::Hsv;
    use once_cell::{sync::Lazy, unsync::OnceCell};

    use crate::editor::colourwheel::ColourWheel;

    #[derive(Default, Debug)]
    pub struct ColourChooserWidget {
        pub(super) colour_wheel: OnceCell<ColourWheel>,
        pub(super) alpha: Cell<u8>,

        colour_button: OnceCell<gtk4::ColorButton>,
        vbox: OnceCell<gtk4::Box>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for ColourChooserWidget {
        const NAME: &'static str = "KCShotColourChooserWidget";
        type Type = super::ColourChooserWidget;
        type ParentType = gtk4::Box;
    }

    impl ObjectImpl for ColourChooserWidget {
        fn constructed(&self) {
            let vbox = self
                .vbox
                .get_or_init(|| gtk4::Box::new(gtk4::Orientation::Vertical, 2));

            let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
            let colour_wheel = self.colour_wheel.get_or_init(ColourWheel::default);
            colour_wheel.set_size_request(256, 256);
            hbox.append(colour_wheel);

            let buttons = make_colour_component_entries(colour_wheel);
            colour_wheel.notify_all_colour_properties();
            hbox.append(&buttons);

            vbox.append(&hbox);

            let alpha_button = self.make_alpha_button(&self.obj(), colour_wheel);
            vbox.append(&alpha_button);

            self.obj().append(vbox);
        }
        fn dispose(&self) {
            if let Some(vbox) = self.vbox.get() {
                vbox.unparent();
            }
        }

        fn properties() -> &'static [ParamSpec] {
            static PROPERTIES: Lazy<Vec<ParamSpec>> = Lazy::new(|| {
                vec![glib::ParamSpecInt::builder("alpha")
                    .minimum(0)
                    .maximum(256)
                    .default_value(255)
                    .readwrite()
                    .build()]
            });

            PROPERTIES.as_ref()
        }

        #[tracing::instrument]
        fn property(&self, _id: usize, pspec: &ParamSpec) -> Value {
            match pspec.name() {
                "alpha" => self.alpha.get().to_value(),
                property => {
                    tracing::error!("Unknown property: {property}");
                    panic!()
                }
            }
        }

        #[tracing::instrument]
        fn set_property(&self, _id: usize, value: &Value, pspec: &ParamSpec) {
            match pspec.name() {
                "alpha" => match value.get::<i32>() {
                    Ok(value) => {
                        self.alpha.set(value as u8);

                        if let Some(colour_button) = self.colour_button.get() {
                            let mut rgba = colour_button.rgba();
                            rgba.set_alpha(value as f32 / 255.0);
                            colour_button.set_rgba(&rgba);
                        }

                        self.obj().notify("alpha");
                    }
                    Err(why) => tracing::error!("'alpha' not an i32: {why}"),
                },
                property => tracing::error!("Unknown property: {property}"),
            }
        }
    }

    impl ColourChooserWidget {
        fn make_alpha_button(
            &self,
            colour_chooser: &super::ColourChooserWidget,
            colour_wheel: &ColourWheel,
        ) -> gtk4::Box {
            let flags = glib::BindingFlags::BIDIRECTIONAL | glib::BindingFlags::SYNC_CREATE;

            let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);

            let label = gtk4::Label::new(Some("Alpha"));
            label.set_valign(gtk4::Align::Center);
            hbox.append(&label);

            let adjustment = gtk4::Adjustment::new(255.0, 0.0, 256.0, 1.0, 1.0, 1.0);

            let entry = gtk4::SpinButton::new(Some(&adjustment), 1.0, 0);
            entry
                .bind_property("value", colour_chooser, "alpha")
                .flags(flags)
                .build();
            entry.set_vexpand(false);
            entry.set_valign(gtk4::Align::Center);
            entry.set_margin_start(5);
            hbox.append(&entry);

            let scale = gtk4::Scale::new(gtk4::Orientation::Horizontal, Some(&adjustment));
            for i in (0..201).step_by(50) {
                scale.add_mark(i as f64, gtk4::PositionType::Bottom, Some(&i.to_string()));
            }
            scale.add_mark(255.0, gtk4::PositionType::Bottom, Some("255"));
            scale.set_vexpand(false);
            scale.set_hexpand(true);
            hbox.append(&scale);

            let colour_button = self
                .colour_button
                .get_or_init(|| gtk4::ColorButton::with_rgba(&colour_wheel.rgba()));
            colour_wheel
                .bind_property("rgba", colour_button, "rgba")
                .flags(flags)
                .transform_to(|binding, mut rgba: gdk::RGBA| {
                    let target = binding
                        .target()
                        .unwrap()
                        .downcast::<gtk4::ColorButton>()
                        .unwrap();

                    rgba.set_alpha(target.rgba().alpha());

                    Some(rgba)
                })
                .build();
            colour_button.set_size_request(50, 50);
            colour_button.set_vexpand(false);
            colour_button.set_hexpand(false);
            colour_button.set_valign(gtk4::Align::Center);
            hbox.append(colour_button);

            hbox
        }
    }

    impl WidgetImpl for ColourChooserWidget {}
    impl BoxImpl for ColourChooserWidget {}

    fn make_colour_component_entries(colour_wheel: &ColourWheel) -> gtk4::Box {
        let buttons = gtk4::Box::new(gtk4::Orientation::Vertical, 2);

        let flags = glib::BindingFlags::BIDIRECTIONAL | glib::BindingFlags::SYNC_CREATE;

        let rgba: gdk::RGBA = colour_wheel.hsv().into();
        let Hsv { h, s, v } = colour_wheel.hsv();
        let (h_component, h_entry) = make_component_button("Hue (°)", h as f64, 0.0, 361.0);
        h_entry
            .bind_property("value", colour_wheel, "h")
            .flags(flags)
            .build();
        buttons.append(&h_component);

        let (s_component, s_entry) = make_component_button("Saturation (%)", s as f64, 0.0, 101.0);
        s_entry
            .bind_property("value", colour_wheel, "s")
            .flags(flags)
            .build();
        buttons.append(&s_component);

        let (v_component, v_entry) = make_component_button("Value (%)", v as f64, 0.0, 101.0);
        v_entry
            .bind_property("value", colour_wheel, "v")
            .flags(flags)
            .build();
        buttons.append(&v_component);

        let (red_component, red_entry) =
            make_component_button("Red", (rgba.red() * 255.0) as f64, 0.0, 256.0);
        red_entry
            .bind_property("value", colour_wheel, "r")
            .flags(flags)
            .build();
        buttons.append(&red_component);

        let (green_component, green_entry) =
            make_component_button("Green", (rgba.green() * 255.0) as f64, 0.0, 256.0);
        green_entry
            .bind_property("value", colour_wheel, "g")
            .flags(flags)
            .build();
        buttons.append(&green_component);

        let (blue_component, blue_entry) =
            make_component_button("Blue", (rgba.blue() * 255.0) as f64, 0.0, 256.0);
        blue_entry
            .bind_property("value", colour_wheel, "b")
            .flags(flags)
            .build();
        buttons.append(&blue_component);

        buttons.append(&make_css_colour_entry(colour_wheel));

        buttons
    }

    fn make_component_button(
        component_name: &str,
        init: f64,
        min: f64,
        max: f64,
    ) -> (gtk4::Box, gtk4::SpinButton) {
        let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        hbox.set_width_request(100);

        let label = gtk4::Label::new(Some(component_name));
        label.set_halign(gtk4::Align::Fill);
        label.set_hexpand(true);
        hbox.append(&label);

        let adjustment = gtk4::Adjustment::new(init, min, max, 1.0, 1.0, 1.0);
        let entry = gtk4::SpinButton::new(Some(&adjustment), 1.0, 0);
        entry.set_halign(gtk4::Align::End);
        hbox.append(&entry);

        (hbox, entry)
    }

    fn make_css_colour_entry(colour_wheel: &ColourWheel) -> gtk4::Box {
        let hbox = gtk4::Box::new(gtk4::Orientation::Horizontal, 2);
        hbox.set_width_request(100);

        let label = gtk4::Label::new(Some("CSS Colour"));
        label.set_halign(gtk4::Align::Center);
        label.set_hexpand(true);
        hbox.append(&label);

        let entry = gtk4::Entry::new();
        colour_wheel
            .bind_property("rgba", &entry, "buffer")
            .transform_to(|_, rgba: gdk::RGBA| {
                let convert = |c: f32| (c * 255.0) as u8;

                let r = convert(rgba.red());
                let g = convert(rgba.green());
                let b = convert(rgba.blue());

                let text = format!("#{r:0>2x}{g:0>2x}{b:0>2x}");

                let buffer = gtk4::EntryBuffer::new(Some(&text));

                Some(buffer.to_value())
            })
            .sync_create()
            .build();
        entry.set_hexpand(false);
        entry.set_halign(gtk4::Align::End);
        entry.connect_activate(glib::clone!(@weak colour_wheel => move |this| {
            let text = this.text();

            if let Ok(colour) = pango::Color::parse(&text) {
                let convert = |c: u16| (c as f32) / 65535.0;
                let r = convert(colour.red());
                let g = convert(colour.green());
                let b = convert(colour.blue());
                let rgba = gdk::RGBA::new(r, g, b, 1.0);

                colour_wheel.set_property("rgba", rgba);
            }
        }));

        hbox.append(&entry);

        hbox
    }
}

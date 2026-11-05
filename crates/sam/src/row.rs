//! One result row: icon, title, subtitle, kind.

use gtk::prelude::*;
use gtk::{gio, pango};

use sam_core::rank::Candidate;

pub struct Row {
    pub widget: gtk::ListBoxRow,
    subtitle: gtk::Label,
    original_subtitle: String,
}

fn icon(spec: &str) -> gtk::Widget {
    let image = gio::Icon::for_string(spec)
        .map(|i| gtk::Image::from_gicon(&i))
        .unwrap_or_else(|_| gtk::Image::from_icon_name("text-x-generic-symbolic"));
    let symbolic = spec.ends_with("-symbolic") || spec.is_empty();
    image.set_pixel_size(if symbolic { 16 } else { 32 });
    image.set_size_request(32, 32);
    if symbolic {
        image.add_css_class("sam-symbolic");
    }
    image.upcast()
}

impl Row {
    pub fn new(c: &Candidate) -> Row {
        Row::with_side(c, c.kind.label())
    }

    /// A row whose right-hand label is `side` rather than its kind.
    pub fn with_side(c: &Candidate, side: &str) -> Row {
        let title = gtk::Label::builder()
            .label(&c.title)
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::End)
            .css_classes(["sam-title"])
            .build();
        let subtitle = gtk::Label::builder()
            .label(&c.subtitle)
            .xalign(0.0)
            .ellipsize(pango::EllipsizeMode::Middle)
            .css_classes(["sam-subtitle"])
            .visible(!c.subtitle.is_empty())
            .build();
        let text = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .valign(gtk::Align::Center)
            .hexpand(true)
            .build();
        text.append(&title);
        text.append(&subtitle);

        let kind = gtk::Label::builder()
            .label(side)
            .valign(gtk::Align::Center)
            .css_classes(["sam-kind"])
            .visible(!side.is_empty())
            .build();

        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .build();
        content.append(&icon(&c.icon));
        content.append(&text);
        content.append(&kind);

        let widget = gtk::ListBoxRow::builder()
            .child(&content)
            .focusable(false)
            .css_classes(["sam-row"])
            .build();
        // Tooltips carry no em-dashes (Luis, 0.0.45).
        widget.set_tooltip_text(Some(&c.title.replace('—', "-")));
        Row {
            widget,
            subtitle,
            original_subtitle: c.subtitle.clone(),
        }
    }

    /// A row left over from the previous file search, shown dimmed until the
    /// new results replace it. It cannot be selected or run.
    pub fn held(c: &Candidate) -> Row {
        let row = Row::new(c);
        row.widget.set_selectable(false);
        row.widget.set_activatable(false);
        row.widget.add_css_class("sam-held");
        row
    }

    /// The row asks before running: its subtitle says how to answer.
    pub fn ask(&self, question: &str) {
        self.widget.add_css_class("sam-confirm");
        self.subtitle.set_label(question);
        self.subtitle.set_visible(true);
    }

    pub fn unask(&self) {
        self.widget.remove_css_class("sam-confirm");
        self.subtitle.set_label(&self.original_subtitle);
        self.subtitle
            .set_visible(!self.original_subtitle.is_empty());
    }
}

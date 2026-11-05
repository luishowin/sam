//! Surfaces (§13): the rounded shapes the panel is made of — the search
//! pill, which becomes the orb and then the answer card, and the results
//! card. A surface paints its own opaque background (F24), so that its size
//! and corner radius can be animated together.
//!
//! The fill colour comes from CSS: a surface's own `color` is its fill (see
//! `style.css`), and its child's `color` is the text colour.

use std::cell::{Cell, RefCell};

use adw::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gdk, glib, graphene, gsk};

/// A surface's outline, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    pub width: f64,
    pub height: f64,
    pub radius: f64,
}

impl Shape {
    pub const fn new(width: f64, height: f64, radius: f64) -> Shape {
        Shape {
            width,
            height,
            radius,
        }
    }

    /// A capsule: both ends fully round.
    pub const fn pill(width: f64, height: f64) -> Shape {
        Shape::new(width, height, height / 2.0)
    }

    /// The shape `t` of the way from `self` to `to`. `t` may overshoot
    /// (an easing with a bounce); sizes never go below one pixel.
    pub fn lerp(self, to: Shape, t: f64) -> Shape {
        let mix = |a: f64, b: f64| a + (b - a) * t;
        Shape {
            width: mix(self.width, to.width).max(1.0),
            height: mix(self.height, to.height).max(1.0),
            radius: mix(self.radius, to.radius).max(0.0),
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Surface {
        pub child: RefCell<Option<gtk::Widget>>,
        /// The target (or, while animating, current) shape.
        pub shape: Cell<Option<Shape>>,
        /// The height follows the child's natural height instead…
        pub natural: Cell<bool>,
        /// …up to this, if set.
        pub max_height: Cell<Option<i32>>,
        pub animation: RefCell<Option<adw::Animation>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Surface {
        const NAME: &'static str = "SamSurface";
        type Type = super::Surface;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("sam-surface");
            klass.set_accessible_role(gtk::AccessibleRole::Group);
        }
    }

    impl ObjectImpl for Surface {
        fn dispose(&self) {
            if let Some(a) = self.animation.take() {
                a.pause();
            }
            if let Some(child) = self.child.take() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Surface {
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let shape = self.shape.get().unwrap_or(Shape::new(1.0, 1.0, 0.0));
            let fixed = |v: f64| {
                let v = v.round() as i32;
                (v, v, -1, -1)
            };
            match orientation {
                gtk::Orientation::Horizontal => fixed(shape.width),
                _ if self.natural.get() => {
                    let width = if for_size >= 0 {
                        for_size
                    } else {
                        shape.width.round() as i32
                    };
                    let natural = self
                        .child
                        .borrow()
                        .as_ref()
                        .filter(|c| c.should_layout())
                        .map_or(0, |c| c.measure(gtk::Orientation::Vertical, width).1);
                    let natural = self
                        .max_height
                        .get()
                        .map_or(natural, |max| natural.min(max));
                    (natural, natural, -1, -1)
                }
                _ => fixed(shape.height),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            if let Some(child) = self.child.borrow().as_ref()
                && child.should_layout()
            {
                // While shrinking, the child keeps at least its minimum size,
                // centred, and the rounded clip hides what sticks out.
                let min_w = child.measure(gtk::Orientation::Horizontal, -1).0;
                let w = width.max(min_w);
                let min_h = child.measure(gtk::Orientation::Vertical, w).0;
                let h = height.max(min_h);
                let offset =
                    graphene::Point::new(((width - w) / 2) as f32, ((height - h) / 2) as f32);
                child.allocate(w, h, -1, Some(gsk::Transform::new().translate(&offset)));
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            if w < 1.0 || h < 1.0 {
                return;
            }
            let radius = obj.radius_for(w, h);
            let outline = gsk::RoundedRect::from_rect(graphene::Rect::new(0.0, 0.0, w, h), radius);
            let style = adw::StyleManager::default();
            let fill = obj.color();
            let text = self
                .child
                .borrow()
                .as_ref()
                .map_or(gdk::RGBA::BLACK, |c| c.color());

            append_shadow(
                snapshot,
                w,
                h,
                radius,
                if style.is_dark() { 0.3 } else { 0.12 },
            );

            // A flat fill, no gradient (Luis, 2026-09-17).
            snapshot.push_rounded_clip(&outline);
            snapshot.append_color(&fill, outline.bounds());
            if let Some(child) = self.child.borrow().as_ref() {
                obj.snapshot_child(child, snapshot);
            }
            snapshot.pop();

            let edge = text.with_alpha(if style.is_high_contrast() { 0.7 } else { 0.32 });
            snapshot.append_border(&outline, &[1.0; 4], &[edge; 4]);
        }
    }
}

/// A soft shadow around a `w`×`h` outline with corner `radius`, drawn as
/// fading one-pixel rings. A blurred shadow looks the same, but the software
/// renderer (F17) would blur it again on every summon: 6 ms of a 10 ms first
/// frame (SPIKE-C).
fn append_shadow(snapshot: &gtk::Snapshot, w: f32, h: f32, radius: f32, strength: f32) {
    const RINGS: u8 = 12;
    for i in 0..RINGS {
        let grow = f32::from(i) + 1.0;
        let ring = gsk::RoundedRect::from_rect(
            graphene::Rect::new(-grow, -grow, w + 2.0 * grow, h + 2.0 * grow),
            radius + grow,
        );
        let fade = 1.0 - f32::from(i) / f32::from(RINGS);
        let color = gdk::RGBA::new(0.0, 0.0, 0.0, strength * fade * fade * 0.25);
        snapshot.append_border(&ring, &[1.0; 4], &[color; 4]);
    }
}

glib::wrapper! {
    pub struct Surface(ObjectSubclass<imp::Surface>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Surface {
    /// A surface of a fixed `shape`, holding `child`.
    pub fn new(child: &impl IsA<gtk::Widget>, shape: Shape) -> Surface {
        let surface: Surface = glib::Object::new();
        surface.imp().shape.set(Some(shape));
        surface.set_child(child);
        surface
    }

    /// A surface `width` wide whose height follows `child`.
    pub fn natural(child: &impl IsA<gtk::Widget>, width: f64, radius: f64) -> Surface {
        let surface = Surface::new(child, Shape::new(width, 1.0, radius));
        surface.imp().natural.set(true);
        surface
    }

    fn set_child(&self, child: &impl IsA<gtk::Widget>) {
        let child = child.as_ref();
        child.set_parent(self);
        self.imp().child.replace(Some(child.clone()));
    }

    /// The corner radius as drawn at `w`×`h`: never more than half a side.
    fn radius_for(&self, w: f32, h: f32) -> f32 {
        let radius = self.imp().shape.get().map_or(0.0, |s| s.radius) as f32;
        radius.min(w / 2.0).min(h / 2.0)
    }

    /// The shape as it is on screen now.
    pub fn current(&self) -> Shape {
        let shape = self.imp().shape.get().unwrap_or(Shape::new(1.0, 1.0, 0.0));
        if self.imp().natural.get() {
            Shape {
                height: f64::from(self.height().max(1)),
                ..shape
            }
        } else {
            shape
        }
    }

    /// From now on the height follows the child's natural height, up to
    /// `max`; a child taller than that has to scroll.
    pub fn follow_content(&self, max: i32) {
        self.imp().max_height.set(Some(max));
        self.imp().natural.set(true);
        self.queue_resize();
    }

    /// Takes `shape` at once, without animating (while hidden, F11).
    pub fn jump_to(&self, shape: Shape) {
        if let Some(previous) = self.imp().animation.take() {
            previous.pause();
        }
        self.imp().natural.set(false);
        self.imp().max_height.set(None);
        self.imp().shape.set(Some(shape));
        self.queue_resize();
    }

    /// Animates to `target` over `ms` milliseconds, then runs `done`. An
    /// animation already running jumps to its end first. With animations
    /// turned off in GNOME, it lands at once.
    pub fn morph_to(
        &self,
        target: Shape,
        ms: u32,
        easing: adw::Easing,
        done: impl FnOnce() + 'static,
    ) {
        if let Some(previous) = self.imp().animation.take() {
            previous.skip();
        }
        let from = self.current();
        self.imp().natural.set(false);
        self.imp().max_height.set(None);
        let weak = self.downgrade();
        let step = adw::CallbackAnimationTarget::new(move |t| {
            if let Some(surface) = weak.upgrade() {
                surface.imp().shape.set(Some(from.lerp(target, t)));
                surface.queue_resize();
            }
        });
        let animation = adw::TimedAnimation::builder()
            .widget(self)
            .value_from(0.0)
            .value_to(1.0)
            .duration(ms)
            .easing(easing)
            .target(&step)
            .build();
        let done = RefCell::new(Some(done));
        let weak = self.downgrade();
        animation.connect_done(move |_| {
            if let Some(surface) = weak.upgrade() {
                surface.imp().shape.set(Some(target));
                surface.queue_resize();
            }
            if let Some(f) = done.borrow_mut().take() {
                f();
            }
        });
        self.imp()
            .animation
            .replace(Some(animation.clone().upcast()));
        animation.play();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_blend_and_never_collapse() {
        let pill = Shape::pill(648.0, 56.0);
        let orb = Shape::pill(64.0, 64.0);
        assert_eq!(pill.radius, 28.0);
        assert_eq!(pill.lerp(orb, 0.0), pill);
        assert_eq!(pill.lerp(orb, 1.0), orb);
        let half = pill.lerp(orb, 0.5);
        assert_eq!((half.width, half.height, half.radius), (356.0, 60.0, 30.0));
        // An overshooting easing past the far end cannot go negative.
        let shrunk = orb.lerp(Shape::new(1.0, 1.0, 0.0), 1.2);
        assert!(shrunk.width >= 1.0 && shrunk.radius >= 0.0);
    }
}

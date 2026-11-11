//! A widget drawn by a closure, through GTK's own renderer (snapshot), so
//! custom drawing is GPU-composited like everything else in the window.

use std::cell::{Cell, RefCell};

use gtk::glib;
use gtk::graphene;
use gtk::gsk;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

type DrawFn = Box<dyn Fn(&Canvas, &gtk::Snapshot, f32, f32)>;
type ResizeFn = Box<dyn Fn(&Canvas, i32, i32)>;
type HeightFn = Box<dyn Fn(i32) -> i32>;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Canvas {
        pub draw: RefCell<Option<DrawFn>>,
        pub resize: RefCell<Option<ResizeFn>>,
        pub height_for_width: RefCell<Option<HeightFn>>,
        pub min_width: Cell<i32>,
        pub nat_width: Cell<i32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Canvas {
        const NAME: &'static str = "HfpCanvas";
        type Type = super::Canvas;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Canvas {}

    impl WidgetImpl for Canvas {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => {
                    let min = self.min_width.get();
                    (min, self.nat_width.get().max(min), -1, -1)
                }
                _ => {
                    let f = self.height_for_width.borrow();
                    let of = |w: i32| f.as_ref().map_or(w, |f| f(w));
                    if for_size > 0 {
                        let h = of(for_size);
                        (h, h, -1, -1)
                    } else {
                        (
                            of(self.min_width.get()),
                            of(self.nat_width.get().max(self.min_width.get())),
                            -1,
                            -1,
                        )
                    }
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            self.parent_size_allocate(width, height, baseline);
            if let Some(f) = self.resize.borrow().as_ref() {
                f(&self.obj(), width, height);
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            if let Some(f) = self.draw.borrow().as_ref() {
                f(&obj, snapshot, obj.width() as f32, obj.height() as f32);
            }
        }
    }
}

glib::wrapper! {
    pub struct Canvas(ObjectSubclass<imp::Canvas>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Canvas {
    fn default() -> Self {
        Self::new()
    }
}

impl Canvas {
    pub fn new() -> Canvas {
        let c: Canvas = glib::Object::new();
        c.imp().min_width.set(120);
        c.imp().nat_width.set(480);
        c
    }

    pub fn set_draw(&self, f: impl Fn(&Canvas, &gtk::Snapshot, f32, f32) + 'static) {
        *self.imp().draw.borrow_mut() = Some(Box::new(f));
        self.queue_draw();
    }

    pub fn set_on_resize(&self, f: impl Fn(&Canvas, i32, i32) + 'static) {
        *self.imp().resize.borrow_mut() = Some(Box::new(f));
    }

    pub fn set_height_for_width(&self, f: impl Fn(i32) -> i32 + 'static) {
        *self.imp().height_for_width.borrow_mut() = Some(Box::new(f));
        self.queue_resize();
    }

    pub fn set_widths(&self, min: i32, natural: i32) {
        self.imp().min_width.set(min);
        self.imp().nat_width.set(natural);
        self.queue_resize();
    }
}

// ---- Small drawing helpers shared by the custom widgets ----------------

pub fn rect(x: f32, y: f32, w: f32, h: f32) -> graphene::Rect {
    graphene::Rect::new(x, y, w, h)
}

pub fn fill_round(s: &gtk::Snapshot, r: graphene::Rect, radius: f32, color: &gtk::gdk::RGBA) {
    s.push_rounded_clip(&gsk::RoundedRect::from_rect(r, radius));
    s.append_color(color, &r);
    s.pop();
}

pub fn outline_round(
    s: &gtk::Snapshot,
    r: graphene::Rect,
    radius: f32,
    width: f32,
    color: &gtk::gdk::RGBA,
) {
    let rr = gsk::RoundedRect::from_rect(r, radius);
    s.append_border(&rr, &[width; 4], &[*color; 4]);
}

/// Text with its top-left corner at (x, y); returns its size.
pub fn text(
    widget: &impl IsA<gtk::Widget>,
    s: &gtk::Snapshot,
    x: f32,
    y: f32,
    markup: &str,
    color: &gtk::gdk::RGBA,
) -> (f32, f32) {
    let layout = widget.create_pango_layout(None);
    layout.set_markup(markup);
    let (w, h) = layout.pixel_size();
    s.save();
    s.translate(&graphene::Point::new(x, y));
    s.append_layout(&layout, color);
    s.restore();
    (w as f32, h as f32)
}

pub fn text_size(widget: &impl IsA<gtk::Widget>, markup: &str) -> (f32, f32) {
    let layout = widget.create_pango_layout(None);
    layout.set_markup(markup);
    let (w, h) = layout.pixel_size();
    (w as f32, h as f32)
}

/// A straight line as a thin rotated rectangle.
pub fn line(
    s: &gtk::Snapshot,
    x1: f32,
    y1: f32,
    x2: f32,
    y2: f32,
    width: f32,
    color: &gtk::gdk::RGBA,
) {
    let (dx, dy) = (x2 - x1, y2 - y1);
    let len = dx.hypot(dy);
    if len <= 0.0 {
        return;
    }
    s.save();
    s.translate(&graphene::Point::new(x1, y1));
    s.rotate(dy.atan2(dx).to_degrees());
    s.append_color(color, &rect(0.0, -width / 2.0, len, width));
    s.restore();
}

pub fn circle(s: &gtk::Snapshot, cx: f32, cy: f32, r: f32, color: &gtk::gdk::RGBA) {
    fill_round(s, rect(cx - r, cy - r, 2.0 * r, 2.0 * r), r, color);
}

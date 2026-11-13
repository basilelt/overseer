//! WebKitGTK 2.54.0 and 2.54.1 crash the whole app (SIGSEGV in the UI process) on any paste from a
//! selection nobody owns: GTK reports its targets as (NULL, -1) and `Clipboard::formats()` walks
//! them as a span of `(size_t)-1` atoms (https://bugs.webkit.org/show_bug.cgi?id=324466, fixed in
//! 2.54.2). A middle-click or Ctrl+V with an empty clipboard was enough. Keep CLIPBOARD and PRIMARY
//! owned: claim an empty one when nobody answers, at startup, on each owner change and on focus.

use std::rc::Rc;

pub(crate) trait Selection: 'static {
    fn probe(&self, done: Box<dyn FnOnce(bool)>);
    fn claim(&self);
    fn on_owner_change(&self, f: Box<dyn Fn()>);
}

pub(crate) fn check<S: Selection>(s: &Rc<S>) {
    let c = s.clone();
    s.probe(Box::new(move |owned| {
        if !owned {
            c.claim();
        }
    }));
}

pub(crate) fn guard<S: Selection>(s: Rc<S>) {
    check(&s);
    let c = s.clone();
    s.on_owner_change(Box::new(move || check(&c)));
}

#[cfg(target_os = "linux")]
mod gtk_impl {
    use super::Selection;
    use gtk::glib::object::ObjectExt;
    use std::cell::RefCell;
    use std::rc::Rc;

    pub(crate) struct Gtk(gtk::Clipboard);

    impl Selection for Gtk {
        fn probe(&self, done: Box<dyn FnOnce(bool)>) {
            self.0
                .request_contents(&gtk::gdk::Atom::intern("TARGETS"), move |_, d| {
                    done(d.length() >= 0)
                });
        }

        fn claim(&self) {
            self.0.set_text("");
        }

        fn on_owner_change(&self, f: Box<dyn Fn()>) {
            self.0.connect_local("owner-change", false, move |_| {
                f();
                None
            });
        }
    }

    thread_local! {
        static GUARDED: RefCell<Vec<Rc<Gtk>>> = const { RefCell::new(Vec::new()) };
    }

    pub fn install() {
        for atom in [gtk::gdk::SELECTION_CLIPBOARD, gtk::gdk::SELECTION_PRIMARY] {
            let s = Rc::new(Gtk(gtk::Clipboard::get(&atom)));
            super::guard(s.clone());
            GUARDED.with(|g| g.borrow_mut().push(s));
        }
    }

    pub fn recheck() {
        GUARDED.with(|g| g.borrow().iter().for_each(super::check));
    }
}

#[cfg(target_os = "linux")]
pub use gtk_impl::{install, recheck};

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    struct Fake {
        owned: Cell<bool>,
        probes: Cell<u32>,
        claims: Cell<u32>,
        listener: RefCell<Option<Box<dyn Fn()>>>,
    }

    impl Fake {
        fn owner_changes(&self, owned: bool) {
            self.owned.set(owned);
            if let Some(f) = self.listener.borrow().as_ref() {
                f();
            }
        }
    }

    impl Selection for Fake {
        fn probe(&self, done: Box<dyn FnOnce(bool)>) {
            self.probes.set(self.probes.get() + 1);
            done(self.owned.get());
        }

        fn claim(&self) {
            self.claims.set(self.claims.get() + 1);
            self.owned.set(true);
        }

        fn on_owner_change(&self, f: Box<dyn Fn()>) {
            *self.listener.borrow_mut() = Some(f);
        }
    }

    #[test]
    fn an_unowned_selection_is_claimed_at_install() {
        let s = Rc::new(Fake::default());
        guard(s.clone());
        assert_eq!((s.probes.get(), s.claims.get()), (1, 1));
        assert!(s.owned.get());
    }

    #[test]
    fn an_owned_selection_is_left_to_its_owner() {
        let s = Rc::new(Fake {
            owned: Cell::new(true),
            ..Default::default()
        });
        guard(s.clone());
        s.owner_changes(true);
        assert_eq!((s.probes.get(), s.claims.get()), (2, 0));
    }

    #[test]
    fn the_selection_is_claimed_again_when_its_owner_goes_away() {
        let s = Rc::new(Fake {
            owned: Cell::new(true),
            ..Default::default()
        });
        guard(s.clone());
        s.owner_changes(false);
        assert_eq!(s.claims.get(), 1);
        assert!(s.owned.get());
        s.owner_changes(true);
        s.owner_changes(false);
        assert_eq!(s.claims.get(), 2);
    }

    #[test]
    fn a_recheck_claims_only_a_selection_left_without_owner() {
        let s = Rc::new(Fake {
            owned: Cell::new(true),
            ..Default::default()
        });
        check(&s);
        assert_eq!(s.claims.get(), 0);
        s.owned.set(false);
        check(&s);
        assert_eq!(s.claims.get(), 1);
    }
}
